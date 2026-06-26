use std::collections::HashSet;

use logos_types::{
    ModeStrategy, PropertyScope, SemanticToken, TokenRegistry, TokenValue, TokenValueKind,
    VariableBinding,
};
use uuid::Uuid;

const MAX_ALIAS_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct VariableResolver {
    pub registry: TokenRegistry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeContext {
    /// The explicitly requested mode, if any.
    pub explicit_mode: Option<Uuid>,
    /// The collection this context belongs to.
    pub collection_id: Uuid,
    /// Parent mode contexts for inheritance, nearest first.
    pub parent_contexts: Vec<ModeContext>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeContext {
    /// The binding to resolve.
    pub binding: VariableBinding,
    /// The shape's own mode context.
    pub mode_context: ModeContext,
    /// The property being resolved, used for scope checking.
    pub property: PropertyScope,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedValue {
    pub value: TokenValue,
    pub source: ResolutionSource,
    /// Full chain from the requested semantic token to the final value token.
    pub alias_chain: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionSource {
    Primitive,
    Alias(Uuid),
    ModeOverride(Uuid),
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionError {
    CircularAlias {
        chain: Vec<Uuid>,
    },
    MaxDepthExceeded {
        chain: Vec<Uuid>,
    },
    VariableNotFound(Uuid),
    ModeNotFound(Uuid),
    ScopeViolation {
        variable_id: Uuid,
        property: PropertyScope,
    },
    DeletedVariable {
        variable_id: Uuid,
    },
}

impl VariableResolver {
    /// Resolve a semantic token to its final primitive value.
    ///
    /// This walks alias chains and applies mode inheritance. It does not use
    /// binding fallback values; call [`resolve_binding`](Self::resolve_binding)
    /// when resolving a shape property.
    pub fn resolve_variable(
        &self,
        variable_id: Uuid,
        mode_context: &ModeContext,
    ) -> Result<ResolvedValue, ResolutionError> {
        let mut current_id = variable_id;
        let mut visited = HashSet::new();
        let mut chain = Vec::new();
        let mut alias_depth = 0;

        loop {
            if !visited.insert(current_id) {
                chain.push(current_id);
                return Err(ResolutionError::CircularAlias { chain });
            }
            chain.push(current_id);

            let token = self
                .registry
                .semantic_tokens
                .get(&current_id)
                .ok_or(ResolutionError::VariableNotFound(current_id))?;
            let value = self.value_for_mode(token, mode_context)?;

            match value.value {
                TokenValueKind::Alias(next_id) => {
                    if alias_depth >= MAX_ALIAS_DEPTH {
                        chain.push(next_id);
                        return Err(ResolutionError::MaxDepthExceeded { chain });
                    }
                    alias_depth += 1;
                    current_id = next_id;
                }
                _ => {
                    let source =
                        self.resolution_source(&chain, token.collection_id, value.mode_id)?;
                    return Ok(ResolvedValue {
                        value,
                        source,
                        alias_chain: chain,
                    });
                }
            }
        }
    }

    /// Resolve a binding on a shape property.
    ///
    /// This checks property scope, applies the binding's mode strategy, and
    /// uses the binding fallback if the token has been removed or corrupted.
    pub fn resolve_binding(
        &self,
        binding: &VariableBinding,
        shape_context: &ShapeContext,
    ) -> Result<ResolvedValue, ResolutionError> {
        debug_assert_eq!(
            binding, &shape_context.binding,
            "resolve_binding binding argument must match ShapeContext.binding"
        );

        let token = match self.registry.semantic_tokens.get(&binding.token_id) {
            Some(token) => token,
            None => {
                return self.binding_fallback(
                    binding,
                    &shape_context.mode_context,
                    ResolutionError::DeletedVariable {
                        variable_id: binding.token_id,
                    },
                );
            }
        };

        if !token.scopes.contains(&shape_context.property) {
            return Err(ResolutionError::ScopeViolation {
                variable_id: binding.token_id,
                property: shape_context.property,
            });
        }

        let mode_context = self.binding_mode_context(binding, &shape_context.mode_context);
        match self.resolve_variable(binding.token_id, &mode_context) {
            Ok(value) => Ok(value),
            Err(error @ ResolutionError::VariableNotFound(_))
            | Err(error @ ResolutionError::DeletedVariable { .. }) => {
                self.binding_fallback(binding, &mode_context, error)
            }
            Err(error) => Err(error),
        }
    }

    fn binding_mode_context(
        &self,
        binding: &VariableBinding,
        mode_context: &ModeContext,
    ) -> ModeContext {
        match binding.mode_strategy {
            ModeStrategy::Inherit => mode_context.clone(),
            ModeStrategy::Explicit(mode_id) => ModeContext {
                explicit_mode: Some(mode_id),
                collection_id: mode_context.collection_id,
                parent_contexts: Vec::new(),
            },
        }
    }

    fn binding_fallback(
        &self,
        binding: &VariableBinding,
        mode_context: &ModeContext,
        error: ResolutionError,
    ) -> Result<ResolvedValue, ResolutionError> {
        let Some(fallback_value) = binding.fallback_value.clone() else {
            return Err(error);
        };

        Ok(ResolvedValue {
            value: TokenValue {
                token_id: binding.token_id,
                mode_id: self.fallback_mode_id(mode_context),
                value: fallback_value,
            },
            source: ResolutionSource::Fallback,
            alias_chain: vec![binding.token_id],
        })
    }

    fn value_for_mode(
        &self,
        token: &SemanticToken,
        mode_context: &ModeContext,
    ) -> Result<TokenValue, ResolutionError> {
        let candidates = self.mode_candidates(mode_context, token.collection_id)?;
        for mode_id in &candidates {
            if let Some(value) = self.registry.token_value(token.id, *mode_id) {
                return Ok(value.clone());
            }
        }

        let missing_mode = candidates.last().copied().unwrap_or(token.collection_id);
        Err(ResolutionError::ModeNotFound(missing_mode))
    }

    fn mode_candidates(
        &self,
        mode_context: &ModeContext,
        collection_id: Uuid,
    ) -> Result<Vec<Uuid>, ResolutionError> {
        let mut candidates = Vec::new();

        self.push_context_modes(mode_context, collection_id, &mut candidates)?;

        let collection = self
            .registry
            .collections
            .get(&collection_id)
            .ok_or(ResolutionError::ModeNotFound(collection_id))?;
        self.push_mode(collection.default_mode_id, collection_id, &mut candidates)?;

        Ok(candidates)
    }

    fn push_context_modes(
        &self,
        mode_context: &ModeContext,
        collection_id: Uuid,
        candidates: &mut Vec<Uuid>,
    ) -> Result<(), ResolutionError> {
        if let Some(mode_id) = mode_context.explicit_mode {
            self.push_mode(mode_id, collection_id, candidates)?;
        }

        for parent in &mode_context.parent_contexts {
            self.push_context_modes(parent, collection_id, candidates)?;
        }

        Ok(())
    }

    fn push_mode(
        &self,
        mode_id: Uuid,
        collection_id: Uuid,
        candidates: &mut Vec<Uuid>,
    ) -> Result<(), ResolutionError> {
        let mode = self
            .registry
            .modes
            .get(&mode_id)
            .ok_or(ResolutionError::ModeNotFound(mode_id))?;

        if mode.collection_id != collection_id {
            return Err(ResolutionError::ModeNotFound(mode_id));
        }

        if !candidates.contains(&mode_id) {
            candidates.push(mode_id);
        }

        Ok(())
    }

    fn fallback_mode_id(&self, mode_context: &ModeContext) -> Uuid {
        mode_context
            .explicit_mode
            .or_else(|| self.first_parent_mode(mode_context))
            .or_else(|| {
                self.registry
                    .collections
                    .get(&mode_context.collection_id)
                    .map(|collection| collection.default_mode_id)
            })
            .unwrap_or_else(Uuid::nil)
    }

    fn first_parent_mode(&self, mode_context: &ModeContext) -> Option<Uuid> {
        for parent in &mode_context.parent_contexts {
            if let Some(mode_id) = parent.explicit_mode {
                return Some(mode_id);
            }
            if let Some(mode_id) = self.first_parent_mode(parent) {
                return Some(mode_id);
            }
        }
        None
    }

    fn resolution_source(
        &self,
        chain: &[Uuid],
        collection_id: Uuid,
        mode_id: Uuid,
    ) -> Result<ResolutionSource, ResolutionError> {
        if chain.len() > 1 {
            return Ok(ResolutionSource::Alias(
                *chain.last().expect("chain is non-empty"),
            ));
        }

        let collection = self
            .registry
            .collections
            .get(&collection_id)
            .ok_or(ResolutionError::ModeNotFound(collection_id))?;
        if mode_id != collection.default_mode_id {
            Ok(ResolutionSource::ModeOverride(mode_id))
        } else {
            Ok(ResolutionSource::Primitive)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logos_types::{TokenSource, TokenType, VariableCollection, VariableMode};

    #[derive(Clone, Copy)]
    struct TestIds {
        collection: Uuid,
        light: Uuid,
        dark: Uuid,
    }

    fn ids() -> TestIds {
        TestIds {
            collection: Uuid::new_v4(),
            light: Uuid::new_v4(),
            dark: Uuid::new_v4(),
        }
    }

    fn base_registry(ids: TestIds) -> TokenRegistry {
        let mut registry = TokenRegistry::new();
        registry.add_collection(VariableCollection::new(
            ids.collection,
            "Brand",
            vec![ids.light, ids.dark],
            ids.light,
        ));
        registry.add_mode(VariableMode::new(ids.light, ids.collection, "Light"));
        registry.add_mode(VariableMode::new(ids.dark, ids.collection, "Dark"));
        registry
    }

    fn token(id: Uuid, collection_id: Uuid, scopes: Vec<PropertyScope>) -> SemanticToken {
        SemanticToken::new(
            id,
            format!("Token {id}"),
            format!("token-{id}"),
            TokenType::Color,
            collection_id,
            scopes,
            TokenSource::File,
        )
    }

    fn mode_context(ids: TestIds, explicit_mode: Option<Uuid>) -> ModeContext {
        ModeContext {
            explicit_mode,
            collection_id: ids.collection,
            parent_contexts: Vec::new(),
        }
    }

    fn resolver_with_token_value(
        ids: TestIds,
        token_id: Uuid,
        mode_id: Uuid,
        value: TokenValueKind,
        scopes: Vec<PropertyScope>,
    ) -> VariableResolver {
        let mut registry = base_registry(ids);
        registry.add_semantic_token(token(token_id, ids.collection, scopes));
        registry.add_token_value(TokenValue {
            token_id,
            mode_id,
            value,
        });
        VariableResolver { registry }
    }

    #[test]
    fn resolve_primitive_variable() {
        let ids = ids();
        let token_id = Uuid::new_v4();
        let resolver = resolver_with_token_value(
            ids,
            token_id,
            ids.light,
            TokenValueKind::Color("#0055ff".into()),
            vec![PropertyScope::Fill],
        );

        let resolved = resolver
            .resolve_variable(token_id, &mode_context(ids, Some(ids.light)))
            .unwrap();

        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#0055ff".into())
        );
        assert_eq!(resolved.source, ResolutionSource::Primitive);
        assert_eq!(resolved.alias_chain, vec![token_id]);
    }

    #[test]
    fn resolve_alias_chain() {
        let ids = ids();
        let token_a = Uuid::new_v4();
        let token_b = Uuid::new_v4();
        let token_c = Uuid::new_v4();
        let mut registry = base_registry(ids);

        for token_id in [token_a, token_b, token_c] {
            registry.add_semantic_token(token(token_id, ids.collection, vec![PropertyScope::Fill]));
        }
        registry.add_token_value(TokenValue {
            token_id: token_a,
            mode_id: ids.light,
            value: TokenValueKind::Alias(token_b),
        });
        registry.add_token_value(TokenValue {
            token_id: token_b,
            mode_id: ids.light,
            value: TokenValueKind::Alias(token_c),
        });
        registry.add_token_value(TokenValue {
            token_id: token_c,
            mode_id: ids.light,
            value: TokenValueKind::Color("#0033aa".into()),
        });

        let resolver = VariableResolver { registry };
        let resolved = resolver
            .resolve_variable(token_a, &mode_context(ids, Some(ids.light)))
            .unwrap();

        assert_eq!(resolved.value.token_id, token_c);
        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#0033aa".into())
        );
        assert_eq!(resolved.source, ResolutionSource::Alias(token_c));
        assert_eq!(resolved.alias_chain, vec![token_a, token_b, token_c]);
    }

    #[test]
    fn resolve_circular_alias() {
        let ids = ids();
        let token_a = Uuid::new_v4();
        let token_b = Uuid::new_v4();
        let mut registry = base_registry(ids);

        for token_id in [token_a, token_b] {
            registry.add_semantic_token(token(token_id, ids.collection, vec![PropertyScope::Fill]));
        }
        registry.add_token_value(TokenValue {
            token_id: token_a,
            mode_id: ids.light,
            value: TokenValueKind::Alias(token_b),
        });
        registry.add_token_value(TokenValue {
            token_id: token_b,
            mode_id: ids.light,
            value: TokenValueKind::Alias(token_a),
        });

        let resolver = VariableResolver { registry };
        let error = resolver
            .resolve_variable(token_a, &mode_context(ids, Some(ids.light)))
            .unwrap_err();

        assert_eq!(
            error,
            ResolutionError::CircularAlias {
                chain: vec![token_a, token_b, token_a]
            }
        );
    }

    #[test]
    fn resolve_mode_inheritance() {
        let ids = ids();
        let token_id = Uuid::new_v4();
        let mut resolver = resolver_with_token_value(
            ids,
            token_id,
            ids.light,
            TokenValueKind::Color("#0055ff".into()),
            vec![PropertyScope::Fill],
        );
        resolver.registry.add_token_value(TokenValue {
            token_id,
            mode_id: ids.dark,
            value: TokenValueKind::Color("#0033aa".into()),
        });
        let child_context = ModeContext {
            explicit_mode: None,
            collection_id: ids.collection,
            parent_contexts: vec![mode_context(ids, Some(ids.dark))],
        };

        let resolved = resolver.resolve_variable(token_id, &child_context).unwrap();

        assert_eq!(resolved.value.mode_id, ids.dark);
        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#0033aa".into())
        );
        assert_eq!(resolved.source, ResolutionSource::ModeOverride(ids.dark));
    }

    #[test]
    fn resolve_mode_fallback() {
        let ids = ids();
        let token_id = Uuid::new_v4();
        let resolver = resolver_with_token_value(
            ids,
            token_id,
            ids.light,
            TokenValueKind::Color("#0055ff".into()),
            vec![PropertyScope::Fill],
        );

        let resolved = resolver
            .resolve_variable(token_id, &mode_context(ids, None))
            .unwrap();

        assert_eq!(resolved.value.mode_id, ids.light);
        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#0055ff".into())
        );
        assert_eq!(resolved.source, ResolutionSource::Primitive);
    }

    #[test]
    fn resolve_scope_violation() {
        let ids = ids();
        let token_id = Uuid::new_v4();
        let resolver = resolver_with_token_value(
            ids,
            token_id,
            ids.light,
            TokenValueKind::Color("#0055ff".into()),
            vec![PropertyScope::TextColor],
        );
        let binding = VariableBinding {
            token_id,
            mode_strategy: ModeStrategy::Inherit,
            fallback_value: Some(TokenValueKind::Color("#000000".into())),
        };
        let shape_context = ShapeContext {
            binding: binding.clone(),
            mode_context: mode_context(ids, None),
            property: PropertyScope::Fill,
        };

        let error = resolver
            .resolve_binding(&binding, &shape_context)
            .unwrap_err();

        assert_eq!(
            error,
            ResolutionError::ScopeViolation {
                variable_id: token_id,
                property: PropertyScope::Fill
            }
        );
    }

    #[test]
    fn resolve_deleted_variable() {
        let ids = ids();
        let resolver = VariableResolver {
            registry: base_registry(ids),
        };
        let token_id = Uuid::new_v4();
        let binding = VariableBinding {
            token_id,
            mode_strategy: ModeStrategy::Inherit,
            fallback_value: Some(TokenValueKind::Color("#0033aa".into())),
        };
        let shape_context = ShapeContext {
            binding: binding.clone(),
            mode_context: mode_context(ids, None),
            property: PropertyScope::Fill,
        };

        let resolved = resolver.resolve_binding(&binding, &shape_context).unwrap();

        assert_eq!(resolved.value.token_id, token_id);
        assert_eq!(resolved.value.mode_id, ids.light);
        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#0033aa".into())
        );
        assert_eq!(resolved.source, ResolutionSource::Fallback);
    }

    #[test]
    fn resolve_missing_alias_target_uses_binding_fallback() {
        let ids = ids();
        let token_id = Uuid::new_v4();
        let missing_alias_target = Uuid::new_v4();
        let resolver = resolver_with_token_value(
            ids,
            token_id,
            ids.light,
            TokenValueKind::Alias(missing_alias_target),
            vec![PropertyScope::Fill],
        );
        let binding = VariableBinding {
            token_id,
            mode_strategy: ModeStrategy::Inherit,
            fallback_value: Some(TokenValueKind::Color("#123456".into())),
        };
        let shape_context = ShapeContext {
            binding: binding.clone(),
            mode_context: mode_context(ids, None),
            property: PropertyScope::Fill,
        };

        let resolved = resolver.resolve_binding(&binding, &shape_context).unwrap();

        assert_eq!(resolved.value.token_id, token_id);
        assert_eq!(resolved.value.mode_id, ids.light);
        assert_eq!(
            resolved.value.value,
            TokenValueKind::Color("#123456".into())
        );
        assert_eq!(resolved.source, ResolutionSource::Fallback);
        assert_eq!(resolved.alias_chain, vec![token_id]);
    }

    #[test]
    fn resolve_max_depth() {
        let ids = ids();
        let mut registry = base_registry(ids);
        let chain: Vec<Uuid> = (0..=MAX_ALIAS_DEPTH + 1).map(|_| Uuid::new_v4()).collect();

        for token_id in &chain {
            registry.add_semantic_token(token(
                *token_id,
                ids.collection,
                vec![PropertyScope::Fill],
            ));
        }
        for pair in chain.windows(2) {
            registry.add_token_value(TokenValue {
                token_id: pair[0],
                mode_id: ids.light,
                value: TokenValueKind::Alias(pair[1]),
            });
        }
        let last = *chain.last().unwrap();
        registry.add_token_value(TokenValue {
            token_id: last,
            mode_id: ids.light,
            value: TokenValueKind::Color("#ffffff".into()),
        });

        let resolver = VariableResolver { registry };
        let error = resolver
            .resolve_variable(chain[0], &mode_context(ids, Some(ids.light)))
            .unwrap_err();

        assert!(matches!(error, ResolutionError::MaxDepthExceeded { .. }));
    }
}
