//! Pure plan validation. This module never commits or mutates document state.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::contract::{
    ContextMap, Diagnostic, DiagnosticCode, DocumentContext, Expression, ImpactSummary,
    ModeStrategy, PropertyPreview, PropertyValue, ProposedOperation, ProposedPlan,
    ResolutionContext, ResolveRequest, TokenReference, TokenSnapshot, UserIntent, ValidationResult,
    ValidationStatus, CONTRACT_VERSION, MAX_PREVIEW_CONTEXTS,
};

/// Validates a proposed plan against immutable, revision-bound snapshots.
/// A `READY` result is a preview only and is not authorization to commit.
pub fn validate_plan(
    snapshot: &TokenSnapshot,
    intent: &UserIntent,
    context: &DocumentContext,
    plan: &ProposedPlan,
) -> ValidationResult {
    let plan_digest = digest(plan);
    let mut diagnostics = Vec::new();
    let mut impact = ImpactSummary {
        created_tokens: 0,
        updated_tokens: 0,
        bound_properties: 0,
        affected_property_paths: Vec::new(),
        contexts_evaluated: Vec::new(),
        property_previews: Vec::new(),
        diagnostics: Vec::new(),
    };
    let reject = |diagnostics: Vec<Diagnostic>, mut impact: ImpactSummary| {
        impact.affected_property_paths.sort();
        impact.affected_property_paths.dedup();
        impact.property_previews.sort_by(|a, b| {
            (&a.shape_id, &a.property_path, &a.context).cmp(&(
                &b.shape_id,
                &b.property_path,
                &b.context,
            ))
        });
        if !impact.property_previews.is_empty() {
            impact.contexts_evaluated = impact
                .property_previews
                .iter()
                .map(|preview| preview.context.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
        }
        impact.diagnostics = diagnostics.clone();
        ValidationResult {
            protocol_version: CONTRACT_VERSION.to_owned(),
            plan_id: plan.plan_id,
            base_revision: plan.base_revision.clone(),
            status: ValidationStatus::Rejected,
            plan_digest: plan_digest.clone(),
            impact,
            diagnostics,
        }
    };

    if intent.protocol_version != CONTRACT_VERSION
        || context.protocol_version != CONTRACT_VERSION
        || plan.protocol_version != CONTRACT_VERSION
        || intent.intent_id != plan.intent_id
    {
        diagnostics.push(diagnostic(DiagnosticCode::UnsupportedFeature));
    }
    if plan.base_revision != snapshot.revision || context.revision != snapshot.revision {
        diagnostics.push(diagnostic(DiagnosticCode::StaleRevision));
    }
    if !context
        .permissions
        .iter()
        .any(|permission| permission == "proposeChanges")
    {
        diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
    }
    if !plan.requires_user_approval
        || plan.operations.is_empty()
        || plan.preview_contexts.len() > MAX_PREVIEW_CONTEXTS
    {
        diagnostics.push(diagnostic(DiagnosticCode::InvalidPlan));
    }
    if !diagnostics.is_empty() {
        return reject(diagnostics, impact);
    }
    let mut property_keys = BTreeSet::new();
    if context
        .property_values
        .iter()
        .any(|property| !property_keys.insert((property.shape_id, property.property_path.clone())))
    {
        diagnostics.push(diagnostic(DiagnosticCode::InvalidPlan));
        return reject(diagnostics, impact);
    }

    // Work on a private preview copy. It is discarded on every return path.
    let mut preview = snapshot.clone();
    let mut preview_document = context.clone();
    let mut changed_properties = BTreeSet::<(Uuid, String)>::new();
    for operation in &plan.operations {
        match operation {
            ProposedOperation::CreateToken { definition } => {
                if !context.allowed_namespaces.contains(&definition.namespace) {
                    diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
                    break;
                }
                if preview.tokens.iter().any(|token| {
                    token.namespace == definition.namespace
                        && token.id == definition.id
                        && token.set_id == definition.set_id
                }) {
                    diagnostics.push(diagnostic(DiagnosticCode::InvalidPlan));
                    break;
                }
                if !expressions_reference_existing(
                    definition.values.iter().map(|v| &v.expression),
                    &preview,
                ) || !definition.values.iter().all(|value| {
                    expression_references_authorized(
                        &value.expression,
                        &definition.namespace,
                        context,
                    )
                }) {
                    diagnostics.push(diagnostic(DiagnosticCode::DependencyOrder));
                    break;
                }
                preview.tokens.push(definition.clone());
                impact.created_tokens += 1;
            }
            ProposedOperation::SetTokenValue {
                target,
                context: when,
                expected_value,
                next_value,
            } => {
                if !context.allowed_namespaces.contains(&target.namespace) {
                    diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
                    break;
                }
                if !expression_references_existing(next_value, &preview)
                    || !expression_references_authorized(next_value, &target.namespace, context)
                {
                    diagnostics.push(diagnostic(DiagnosticCode::DependencyOrder));
                    break;
                }
                let set_context = flatten_context(context.context.as_ref(), when);
                let definition_index =
                    match select_token_definition_index(&preview, target, &set_context) {
                        Ok(index) => index,
                        Err(code) => {
                            diagnostics.push(diagnostic(code));
                            break;
                        }
                    };
                let Some(value) = preview.tokens[definition_index]
                    .values
                    .iter_mut()
                    .find(|value| &value.when == when)
                else {
                    diagnostics.push(diagnostic(DiagnosticCode::DependencyOrder));
                    break;
                };
                if expected_value.is_none() {
                    diagnostics.push(diagnostic(DiagnosticCode::InvalidPlan));
                    break;
                }
                if expected_value
                    .as_ref()
                    .is_some_and(|expected| expected != &value.expression)
                {
                    diagnostics.push(diagnostic(DiagnosticCode::StaleRevision));
                    break;
                }
                value.expression = next_value.clone();
                impact.updated_tokens += 1;
            }
            ProposedOperation::SetProperty {
                shape_id,
                property_path,
                expected_property,
                next_property,
            } => {
                if !context.selected_shape_ids.contains(shape_id) {
                    diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
                    break;
                }
                let Some(property) = preview_document
                    .property_values
                    .iter_mut()
                    .find(|property| {
                        property.shape_id == *shape_id && property.property_path == *property_path
                    })
                else {
                    diagnostics.push(diagnostic(DiagnosticCode::MissingToken));
                    break;
                };
                if expected_property.is_none() {
                    diagnostics.push(diagnostic(DiagnosticCode::InvalidPlan));
                    break;
                }
                if expected_property
                    .as_ref()
                    .is_some_and(|expected| expected != &property.value)
                {
                    diagnostics.push(diagnostic(DiagnosticCode::StaleRevision));
                    break;
                }
                if let crate::contract::PropertyValue::Binding { target, .. } = next_property {
                    if !context.allowed_namespaces.contains(&target.namespace) {
                        diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
                        break;
                    }
                    let has_fallback = matches!(
                        next_property,
                        PropertyValue::Binding {
                            fallback: Some(_),
                            ..
                        }
                    );
                    if !reference_exists(target, &preview) && !has_fallback {
                        diagnostics.push(diagnostic(DiagnosticCode::MissingToken));
                        break;
                    }
                }
                let Some(expected_type) = property_type(&property.value, &preview) else {
                    diagnostics.push(diagnostic(DiagnosticCode::UnsupportedFeature));
                    break;
                };
                if property_type(next_property, &preview).as_ref() != Some(&expected_type) {
                    diagnostics.push(diagnostic(DiagnosticCode::TypeMismatch));
                    break;
                }
                property.value = next_property.clone();
                if matches!(next_property, PropertyValue::Binding { .. }) {
                    impact.bound_properties += 1;
                }
                impact.affected_property_paths.push(property_path.clone());
                changed_properties.insert((*shape_id, property_path.clone()));
            }
        }
        if !diagnostics.is_empty() {
            break;
        }
    }

    let operations_valid = diagnostics.is_empty();
    if operations_valid {
        diagnostics.extend(validate_dependency_graph(&preview));
    }
    if diagnostics.is_empty() {
        let mut contexts = plan.preview_contexts.clone();
        if contexts.is_empty() {
            contexts.push(ContextMap::new());
        }
        diagnostics.extend(validate_preview_resolution(&preview, &contexts));
    }
    if operations_valid && diagnostics.is_empty() {
        let contexts = if plan.preview_contexts.is_empty() {
            vec![ContextMap::new()]
        } else {
            plan.preview_contexts.clone()
        };
        let (property_previews, property_diagnostics) = resolve_property_previews(
            &preview,
            &preview_document,
            context.context.as_ref(),
            &changed_properties,
            &contexts,
        );
        impact.property_previews = property_previews;
        diagnostics.extend(property_diagnostics);
    }
    if !diagnostics.is_empty() {
        impact.diagnostics = diagnostics.clone();
        return reject(diagnostics, impact);
    }
    impact.affected_property_paths.sort();
    impact.affected_property_paths.dedup();
    impact.property_previews.sort_by(|a, b| {
        (&a.shape_id, &a.property_path, &a.context).cmp(&(
            &b.shape_id,
            &b.property_path,
            &b.context,
        ))
    });
    let mut contexts: BTreeSet<ContextMap> = impact
        .property_previews
        .iter()
        .map(|preview| preview.context.clone())
        .collect();
    if contexts.is_empty() {
        contexts.extend(plan.preview_contexts.iter().cloned());
        if contexts.is_empty() {
            contexts.insert(ContextMap::new());
        }
    }
    impact.contexts_evaluated = contexts.into_iter().collect();
    ValidationResult {
        protocol_version: CONTRACT_VERSION.to_owned(),
        plan_id: plan.plan_id,
        base_revision: plan.base_revision.clone(),
        status: ValidationStatus::Ready,
        plan_digest,
        impact,
        diagnostics: Vec::new(),
    }
}

fn property_type(
    value: &PropertyValue,
    snapshot: &TokenSnapshot,
) -> Option<crate::contract::TokenType> {
    match value {
        PropertyValue::Literal { value, .. } => Some(value.token_type()),
        PropertyValue::Binding {
            target,
            local_override,
            fallback,
            ..
        } => local_override
            .as_ref()
            .or(fallback.as_ref())
            .map(|v| v.token_type())
            .or_else(|| {
                snapshot
                    .tokens
                    .iter()
                    .find(|token| {
                        token.id == target.token_id && token.namespace == target.namespace
                    })
                    .map(|token| token.token_type.clone())
            }),
    }
}

fn select_token_definition_index(
    snapshot: &TokenSnapshot,
    target: &TokenReference,
    effective_context: &ContextMap,
) -> Result<usize, DiagnosticCode> {
    for set_id in snapshot.active_set_ids.iter().rev() {
        let set = snapshot
            .sets
            .iter()
            .find(|set| set.id == *set_id)
            .ok_or(DiagnosticCode::InvalidSnapshot)?;
        if !set
            .when
            .iter()
            .all(|(key, value)| effective_context.get(key) == Some(value))
        {
            continue;
        }
        let mut matches = snapshot.tokens.iter().enumerate().filter(|(_, token)| {
            token.namespace == target.namespace
                && token.id == target.token_id
                && token.set_id == Some(*set_id)
        });
        if let Some((index, _)) = matches.next() {
            if matches.next().is_some() {
                return Err(DiagnosticCode::InvalidSnapshot);
            }
            return Ok(index);
        }
    }
    snapshot
        .tokens
        .iter()
        .position(|token| {
            token.namespace == target.namespace
                && token.id == target.token_id
                && token.set_id.is_none()
        })
        .ok_or(DiagnosticCode::MissingToken)
}

fn flatten_context(base: Option<&ResolutionContext>, operation_context: &ContextMap) -> ContextMap {
    let mut effective = ContextMap::new();
    if let Some(base) = base {
        effective.extend(base.document.clone());
        effective.extend(base.page.clone());
        for ancestor in base.ancestors_nearest_first.iter().rev() {
            effective.extend(ancestor.clone());
        }
        effective.extend(base.explicit.clone());
    }
    effective.extend(operation_context.clone());
    effective
}

fn resolve_property_previews(
    snapshot: &TokenSnapshot,
    document: &DocumentContext,
    base_context: Option<&ResolutionContext>,
    changed_properties: &BTreeSet<(Uuid, String)>,
    contexts: &[ContextMap],
) -> (Vec<PropertyPreview>, Vec<Diagnostic>) {
    let mut previews = Vec::new();
    let mut diagnostics = Vec::new();
    for (shape_id, property_path) in changed_properties {
        let Some(property) = document.property_values.iter().find(|property| {
            property.shape_id == *shape_id && property.property_path == *property_path
        }) else {
            diagnostics.push(diagnostic(DiagnosticCode::MissingToken));
            continue;
        };
        let Some(property_type) = property_type(&property.value, snapshot) else {
            diagnostics.push(diagnostic(DiagnosticCode::UnsupportedFeature));
            continue;
        };
        for preview_context in contexts {
            let mut resolution_context =
                base_context.cloned().unwrap_or_else(|| ResolutionContext {
                    explicit: ContextMap::new(),
                    ancestors_nearest_first: Vec::new(),
                    page: ContextMap::new(),
                    document: ContextMap::new(),
                });
            resolution_context.explicit.extend(preview_context.clone());
            let request = ResolveRequest {
                contract_version: CONTRACT_VERSION.to_owned(),
                snapshot_revision: snapshot.revision.clone(),
                property_type: property_type.clone(),
                property: property.value.clone(),
                context: resolution_context,
            };
            let resolution = crate::contract_resolver::resolve(snapshot, &request);
            if resolution.status == crate::contract::ResolveStatus::Error {
                diagnostics.extend(
                    resolution
                        .diagnostics
                        .iter()
                        .cloned()
                        .map(|mut diagnostic| {
                            diagnostic.property_path = Some(property_path.clone());
                            diagnostic
                        }),
                );
            }
            previews.push(PropertyPreview {
                shape_id: *shape_id,
                property_path: property_path.clone(),
                context: resolution.effective_context.clone(),
                property_value: property.value.clone(),
                resolution,
            });
        }
    }
    (previews, diagnostics)
}

fn validate_preview_resolution(
    snapshot: &TokenSnapshot,
    contexts: &[ContextMap],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for token in &snapshot.tokens {
        for token_value in &token.values {
            for preview_context in contexts {
                let mut explicit = token_value.when.clone();
                explicit.extend(preview_context.clone());
                let request = ResolveRequest {
                    contract_version: CONTRACT_VERSION.to_owned(),
                    snapshot_revision: snapshot.revision.clone(),
                    property_type: token.token_type.clone(),
                    property: PropertyValue::Binding {
                        target: TokenReference {
                            namespace: token.namespace.clone(),
                            token_id: token.id,
                        },
                        mode_strategy: ModeStrategy::Inherit,
                        fallback: None,
                        local_override: None,
                        extensions: Default::default(),
                    },
                    context: ResolutionContext {
                        explicit,
                        ancestors_nearest_first: Vec::new(),
                        page: ContextMap::new(),
                        document: ContextMap::new(),
                    },
                };
                let result = crate::contract_resolver::resolve(snapshot, &request);
                if result.status == crate::contract::ResolveStatus::Error {
                    for item in result.diagnostics {
                        if !diagnostics
                            .iter()
                            .any(|existing: &Diagnostic| existing.code == item.code)
                        {
                            diagnostics.push(item);
                        }
                    }
                    if !diagnostics.is_empty() {
                        return diagnostics;
                    }
                }
            }
        }
    }
    diagnostics
}

fn digest(plan: &ProposedPlan) -> String {
    // serde_json's default Value map is key-sorted, matching the protocol's
    // cross-language canonical JSON digest rule.
    let canonical = serde_json::to_value(plan).expect("typed plan serializes");
    let bytes = serde_json::to_vec(&canonical).expect("canonical plan serializes");
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn diagnostic(code: DiagnosticCode) -> Diagnostic {
    Diagnostic {
        code,
        severity: crate::contract::Severity::Error,
        message: code.message().to_owned(),
        token_chain: Vec::new(),
        property_path: None,
    }
}

fn reference_exists(reference: &TokenReference, snapshot: &TokenSnapshot) -> bool {
    snapshot
        .tokens
        .iter()
        .any(|token| token.namespace == reference.namespace && token.id == reference.token_id)
}

fn expressions_reference_existing<'a>(
    expressions: impl Iterator<Item = &'a Expression>,
    snapshot: &TokenSnapshot,
) -> bool {
    expressions
        .into_iter()
        .all(|expression| expression_references_existing(expression, snapshot))
}

fn expression_references_existing(expression: &Expression, snapshot: &TokenSnapshot) -> bool {
    match expression {
        Expression::Literal { .. } => true,
        Expression::Reference { target } => reference_exists(target, snapshot),
        Expression::Operation { arguments, .. } => arguments
            .iter()
            .all(|argument| expression_references_existing(argument, snapshot)),
    }
}

fn expression_references_authorized(
    expression: &Expression,
    source_namespace: &str,
    context: &DocumentContext,
) -> bool {
    match expression {
        Expression::Literal { .. } => true,
        Expression::Reference { target } => {
            target.namespace == source_namespace
                || context.allowed_namespaces.contains(&target.namespace)
        }
        Expression::Operation { arguments, .. } => arguments
            .iter()
            .all(|argument| expression_references_authorized(argument, source_namespace, context)),
    }
}

fn validate_dependency_graph(snapshot: &TokenSnapshot) -> Vec<Diagnostic> {
    fn visit(
        reference: &TokenReference,
        snapshot: &TokenSnapshot,
        active: &mut Vec<TokenReference>,
        done: &mut Vec<TokenReference>,
    ) -> Option<DiagnosticCode> {
        if active.contains(reference) {
            return Some(DiagnosticCode::AliasCycle);
        }
        if done.contains(reference) {
            return None;
        }
        let Some(token) = snapshot
            .tokens
            .iter()
            .find(|token| token.id == reference.token_id && token.namespace == reference.namespace)
        else {
            return Some(DiagnosticCode::MissingToken);
        };
        active.push(reference.clone());
        for value in &token.values {
            let mut refs = Vec::new();
            collect_references(&value.expression, &mut refs);
            for dependency in refs {
                if let Some(code) = visit(&dependency, snapshot, active, done) {
                    return Some(code);
                }
            }
        }
        active.pop();
        done.push(reference.clone());
        None
    }
    let mut done = Vec::new();
    for token in &snapshot.tokens {
        let reference = TokenReference {
            namespace: token.namespace.clone(),
            token_id: token.id,
        };
        if let Some(code) = visit(&reference, snapshot, &mut Vec::new(), &mut done) {
            return vec![diagnostic(code)];
        }
    }
    Vec::new()
}

fn collect_references(expression: &Expression, output: &mut Vec<TokenReference>) {
    match expression {
        Expression::Reference { target } => output.push(target.clone()),
        Expression::Operation { arguments, .. } => {
            for argument in arguments {
                collect_references(argument, output);
            }
        }
        Expression::Literal { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        ColorValue, ContractStatus, DocumentProperty, PropertyValue, TokenDefinition, TokenType,
        TokenValueDefinition, TypedValue,
    };
    use std::collections::BTreeMap;
    use uuid::Uuid;

    fn snapshot() -> TokenSnapshot {
        TokenSnapshot {
            contract_version: CONTRACT_VERSION.into(),
            status: ContractStatus::Proposed,
            revision: "rev-1".into(),
            document_namespace: "doc:test".into(),
            tokens: vec![],
            collections: vec![],
            sets: vec![],
            context_dimensions: vec![],
            active_set_ids: vec![],
            extensions: Default::default(),
        }
    }
    fn intent() -> UserIntent {
        UserIntent {
            protocol_version: CONTRACT_VERSION.into(),
            intent_id: Uuid::nil(),
            text: "create token".into(),
            requested_capabilities: vec![],
            constraints: vec![],
        }
    }
    fn context(permission: bool) -> DocumentContext {
        DocumentContext {
            protocol_version: CONTRACT_VERSION.into(),
            document_id: Uuid::nil(),
            revision: "rev-1".into(),
            page_id: None,
            selected_shape_ids: vec![Uuid::nil()],
            property_values: vec![DocumentProperty {
                shape_id: Uuid::nil(),
                property_path: "fill.color".into(),
                value: PropertyValue::Literal {
                    value: TypedValue::Color {
                        value: ColorValue {
                            space: "srgb".into(),
                            hex: "#123456".into(),
                            alpha: 1.0,
                        },
                    },
                    extensions: Default::default(),
                },
            }],
            context: None,
            allowed_namespaces: vec!["doc:test".into()],
            permissions: if permission {
                vec!["proposeChanges".into()]
            } else {
                vec![]
            },
        }
    }
    fn color(hex: &str) -> PropertyValue {
        PropertyValue::Literal {
            value: TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: hex.into(),
                    alpha: 1.0,
                },
            },
            extensions: Default::default(),
        }
    }
    fn binding(target: u128, strategy: ModeStrategy) -> PropertyValue {
        PropertyValue::Binding {
            target: TokenReference {
                namespace: "doc:test".into(),
                token_id: Uuid::from_u128(target),
            },
            mode_strategy: strategy,
            fallback: None,
            local_override: None,
            extensions: Default::default(),
        }
    }
    fn set_property(expected: Option<PropertyValue>, next: PropertyValue) -> ProposedOperation {
        ProposedOperation::SetProperty {
            shape_id: Uuid::nil(),
            property_path: "fill.color".into(),
            expected_property: expected,
            next_property: next,
        }
    }
    fn literal_expression(value: PropertyValue) -> Expression {
        match value {
            PropertyValue::Literal { value, .. } => Expression::Literal { value },
            _ => panic!("test helper expects a literal"),
        }
    }
    fn definition(
        id: u128,
        expression: Expression,
        collection_id: Option<Uuid>,
    ) -> TokenDefinition {
        TokenDefinition {
            id: Uuid::from_u128(id),
            namespace: "doc:test".into(),
            name: format!("token/{id}"),
            token_type: TokenType::Color,
            group_path: vec![],
            collection_id,
            set_id: None,
            values: vec![TokenValueDefinition {
                when: ContextMap::new(),
                expression,
            }],
            extensions: Default::default(),
        }
    }
    fn sample_plan() -> ProposedPlan {
        ProposedPlan {
            protocol_version: CONTRACT_VERSION.into(),
            plan_id: Uuid::nil(),
            base_revision: "rev-1".into(),
            intent_id: Uuid::nil(),
            summary: "create color".into(),
            operations: vec![
                ProposedOperation::CreateToken {
                    definition: TokenDefinition {
                        id: Uuid::from_u128(1),
                        namespace: "doc:test".into(),
                        name: "brand/primary".into(),
                        token_type: TokenType::Color,
                        group_path: vec!["brand".into()],
                        collection_id: None,
                        set_id: None,
                        values: vec![TokenValueDefinition {
                            when: ContextMap::new(),
                            expression: Expression::Literal {
                                value: TypedValue::Color {
                                    value: ColorValue {
                                        space: "srgb".into(),
                                        hex: "#123456".into(),
                                        alpha: 1.0,
                                    },
                                },
                            },
                        }],
                        extensions: Default::default(),
                    },
                },
                ProposedOperation::SetProperty {
                    shape_id: Uuid::nil(),
                    property_path: "fill.color".into(),
                    expected_property: Some(PropertyValue::Literal {
                        value: TypedValue::Color {
                            value: ColorValue {
                                space: "srgb".into(),
                                hex: "#123456".into(),
                                alpha: 1.0,
                            },
                        },
                        extensions: Default::default(),
                    }),
                    next_property: PropertyValue::Binding {
                        target: TokenReference {
                            namespace: "doc:test".into(),
                            token_id: Uuid::from_u128(1),
                        },
                        mode_strategy: ModeStrategy::Inherit,
                        fallback: None,
                        local_override: None,
                        extensions: Default::default(),
                    },
                },
            ],
            assumptions: vec![],
            preview_contexts: vec![],
            requires_user_approval: true,
            extensions: Default::default(),
        }
    }

    #[test]
    fn valid_create_plan_is_preview_only_and_deterministic() {
        let snapshot = snapshot();
        let intent = intent();
        let context = context(true);
        let plan = sample_plan();
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let first = validate_plan(&snapshot, &intent, &context, &plan);
        let second = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(first.status, ValidationStatus::Ready);
        assert_eq!(first, second);
        assert_eq!(first.impact.created_tokens, 1);
        assert_eq!(first.impact.bound_properties, 1);
        assert_eq!(first.impact.property_previews.len(), 1);
        assert_eq!(
            first.impact.property_previews[0].resolution.status,
            crate::contract::ResolveStatus::Resolved
        );
        assert_eq!(
            first.impact.property_previews[0].resolution.source,
            Some(crate::contract::ResolutionSource::Token)
        );
        assert_eq!(
            first.impact.property_previews[0].resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#123456".into(),
                    alpha: 1.0
                }
            })
        );
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn plan_operations_round_trip_the_versioned_camel_case_wire_contract() {
        let mut plan = sample_plan();
        if let ProposedOperation::SetProperty { next_property, .. } = &mut plan.operations[1] {
            *next_property = binding(
                1,
                ModeStrategy::Explicit {
                    mode_id: Uuid::from_u128(77),
                },
            );
        }
        let json = serde_json::to_value(&plan).unwrap();
        let operation = &json["operations"][1];
        assert!(operation.get("expectedProperty").is_some());
        assert!(operation["nextProperty"].get("modeStrategy").is_some());
        assert_eq!(
            operation["nextProperty"]["modeStrategy"]["modeId"],
            Uuid::from_u128(77).to_string()
        );
        assert!(operation.get("expected_property").is_none());
        assert_eq!(serde_json::from_value::<ProposedPlan>(json).unwrap(), plan);
    }

    #[test]
    fn missing_permission_and_stale_revision_fail_closed_without_mutation() {
        let snapshot = snapshot();
        let intent = intent();
        let context = context(false);
        let mut plan = sample_plan();
        plan.base_revision = "old-revision".into();
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::ScopeDenied));
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::StaleRevision));
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn proposal_type_errors_are_rejected_without_mutating_inputs() {
        let snapshot = snapshot();
        let intent = intent();
        let context = context(true);
        let mut plan = sample_plan();
        if let ProposedOperation::CreateToken { definition } = &mut plan.operations[0] {
            definition.values[0].expression = Expression::Literal {
                value: TypedValue::Number { value: 5.0 },
            };
        }
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::TypeMismatch));
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn create_alias_then_bind_resolves_the_complete_proposed_chain() {
        let snapshot = snapshot();
        let intent = intent();
        let context = context(true);
        let mut plan = sample_plan();
        plan.operations.insert(
            1,
            ProposedOperation::CreateToken {
                definition: definition(
                    2,
                    Expression::Reference {
                        target: TokenReference {
                            namespace: "doc:test".into(),
                            token_id: Uuid::from_u128(1),
                        },
                    },
                    None,
                ),
            },
        );
        plan.operations[2] =
            set_property(Some(color("#123456")), binding(2, ModeStrategy::Inherit));
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(
            result.status,
            ValidationStatus::Ready,
            "{:?}",
            result.diagnostics
        );
        assert_eq!(result.impact.created_tokens, 2);
        let preview = &result.impact.property_previews[0];
        assert_eq!(preview.resolution.dependency_trace.len(), 2);
        assert_eq!(
            preview.resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#123456".into(),
                    alpha: 1.0
                }
            })
        );
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn token_update_is_visible_through_a_dependent_binding_in_preview() {
        let original = literal_expression(color("#123456"));
        let base = definition(1, original.clone(), None);
        let dependent = definition(
            2,
            Expression::Reference {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(1),
                },
            },
            None,
        );
        let mut snapshot = snapshot();
        snapshot.tokens = vec![base, dependent];
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        plan.operations = vec![
            ProposedOperation::SetTokenValue {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(1),
                },
                context: ContextMap::new(),
                expected_value: Some(original),
                next_value: literal_expression(color("#ff0000")),
            },
            set_property(Some(color("#123456")), binding(2, ModeStrategy::Inherit)),
        ];
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(
            result.status,
            ValidationStatus::Ready,
            "{:?}",
            result.diagnostics
        );
        assert_eq!(result.impact.updated_tokens, 1);
        assert_eq!(
            result.impact.property_previews[0].resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#ff0000".into(),
                    alpha: 1.0
                }
            })
        );
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn repeated_property_writes_use_sequential_pre_state_and_report_final_value() {
        let snapshot = snapshot();
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        let first = binding(1, ModeStrategy::Inherit);
        plan.operations.push(set_property(
            Some(first.clone()),
            PropertyValue::Binding {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(1),
                },
                mode_strategy: ModeStrategy::Inherit,
                fallback: None,
                local_override: Some(TypedValue::Color {
                    value: ColorValue {
                        space: "srgb".into(),
                        hex: "#ff0000".into(),
                        alpha: 1.0,
                    },
                }),
                extensions: Default::default(),
            },
        ));
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(
            result.status,
            ValidationStatus::Ready,
            "{:?}",
            result.diagnostics
        );
        assert_eq!(result.impact.bound_properties, 2);
        assert_eq!(result.impact.property_previews.len(), 1);
        assert_eq!(
            result.impact.property_previews[0].resolution.source,
            Some(crate::contract::ResolutionSource::LocalOverride)
        );
        assert_eq!(
            result.impact.property_previews[0].resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#ff0000".into(),
                    alpha: 1.0
                }
            })
        );
    }

    #[test]
    fn conflicting_repeat_pre_state_and_missing_pre_state_are_rejected_atomically() {
        let snapshot = snapshot();
        let context = context(true);
        let intent = intent();
        let mut conflict = sample_plan();
        conflict.operations.push(set_property(
            Some(color("#123456")),
            binding(1, ModeStrategy::Inherit),
        ));
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &conflict);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::StaleRevision));
        assert!(result.impact.property_previews.is_empty());
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);

        let mut omitted = sample_plan();
        omitted.operations[1] = set_property(None, binding(1, ModeStrategy::Inherit));
        let result = validate_plan(&snapshot, &intent, &context, &omitted);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPlan));

        let mut token_snapshot = snapshot.clone();
        let old_expression = literal_expression(color("#123456"));
        token_snapshot
            .tokens
            .push(definition(1, old_expression.clone(), None));
        let mut token_update = sample_plan();
        token_update.operations = vec![ProposedOperation::SetTokenValue {
            target: TokenReference {
                namespace: "doc:test".into(),
                token_id: Uuid::from_u128(1),
            },
            context: ContextMap::new(),
            expected_value: None,
            next_value: literal_expression(color("#ff0000")),
        }];
        let result = validate_plan(&token_snapshot, &intent, &context, &token_update);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPlan));
    }

    #[test]
    fn missing_explicit_mode_and_unauthorized_namespace_bindings_fail_resolution() {
        use crate::contract::{Collection, Mode};
        let mut snapshot = snapshot();
        let collection_id = Uuid::from_u128(30);
        let mode_id = Uuid::from_u128(31);
        snapshot.collections.push(Collection {
            id: collection_id,
            name: "Theme".into(),
            mode_dimension: "theme".into(),
            default_mode_id: mode_id,
            modes: vec![Mode {
                id: mode_id,
                option: "light".into(),
            }],
            extensions: Default::default(),
        });
        snapshot.tokens.push(definition(
            3,
            literal_expression(color("#123456")),
            Some(collection_id),
        ));
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        plan.operations = vec![set_property(
            Some(color("#123456")),
            binding(
                3,
                ModeStrategy::Explicit {
                    mode_id: Uuid::from_u128(99),
                },
            ),
        )];
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::MissingMode
                && d.property_path.as_deref() == Some("fill.color")));
        assert_eq!(
            result.impact.property_previews[0].resolution.status,
            crate::contract::ResolveStatus::Error
        );

        let mut unauthorized = sample_plan();
        unauthorized.operations = vec![set_property(
            Some(color("#123456")),
            PropertyValue::Binding {
                target: TokenReference {
                    namespace: "foreign:library".into(),
                    token_id: Uuid::from_u128(3),
                },
                mode_strategy: ModeStrategy::Inherit,
                fallback: None,
                local_override: None,
                extensions: Default::default(),
            },
        )];
        assert_eq!(
            validate_plan(&snapshot, &intent, &context, &unauthorized).status,
            ValidationStatus::Rejected
        );
    }

    #[test]
    fn property_preview_uses_document_context_and_reports_effective_mode() {
        use crate::contract::{Collection, Mode};
        let collection_id = Uuid::from_u128(40);
        let light_id = Uuid::from_u128(41);
        let dark_id = Uuid::from_u128(42);
        let mut snapshot = snapshot();
        snapshot.collections.push(Collection {
            id: collection_id,
            name: "Theme".into(),
            mode_dimension: "theme".into(),
            default_mode_id: light_id,
            modes: vec![
                Mode {
                    id: light_id,
                    option: "light".into(),
                },
                Mode {
                    id: dark_id,
                    option: "dark".into(),
                },
            ],
            extensions: Default::default(),
        });
        let mut token = definition(4, literal_expression(color("#ffffff")), Some(collection_id));
        token.values = vec![
            TokenValueDefinition {
                when: BTreeMap::from([("theme".into(), "light".into())]),
                expression: literal_expression(color("#ffffff")),
            },
            TokenValueDefinition {
                when: BTreeMap::from([("theme".into(), "dark".into())]),
                expression: literal_expression(color("#111111")),
            },
        ];
        snapshot.tokens.push(token);
        let mut context = context(true);
        context.context = Some(ResolutionContext {
            explicit: ContextMap::new(),
            ancestors_nearest_first: vec![],
            page: ContextMap::new(),
            document: BTreeMap::from([("theme".into(), "dark".into())]),
        });
        let intent = intent();
        let mut plan = sample_plan();
        plan.operations = vec![set_property(
            Some(color("#123456")),
            binding(4, ModeStrategy::Inherit),
        )];
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(
            result.status,
            ValidationStatus::Ready,
            "{:?}",
            result.diagnostics
        );
        let preview = &result.impact.property_previews[0];
        assert_eq!(
            preview.resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#111111".into(),
                    alpha: 1.0
                }
            })
        );
        assert_eq!(
            preview.context.get("theme").map(String::as_str),
            Some("dark")
        );
        assert!(result
            .impact
            .contexts_evaluated
            .iter()
            .any(|c| c.get("theme").is_some_and(|v| v == "dark")));
    }

    #[test]
    fn fallback_resolution_is_recorded_with_warning_and_effective_value() {
        let snapshot = snapshot();
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        let fallback = TypedValue::Color {
            value: ColorValue {
                space: "srgb".into(),
                hex: "#ff0000".into(),
                alpha: 1.0,
            },
        };
        plan.operations = vec![set_property(
            Some(color("#123456")),
            PropertyValue::Binding {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(999),
                },
                mode_strategy: ModeStrategy::Inherit,
                fallback: Some(fallback.clone()),
                local_override: None,
                extensions: Default::default(),
            },
        )];
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Ready);
        let resolution = &result.impact.property_previews[0].resolution;
        assert_eq!(
            resolution.source,
            Some(crate::contract::ResolutionSource::Fallback)
        );
        assert_eq!(resolution.value, Some(fallback));
        assert_eq!(
            resolution.diagnostics[0].severity,
            crate::contract::Severity::Warning
        );
    }

    #[test]
    fn token_value_update_targets_the_active_matching_set_definition() {
        use crate::contract::TokenSet;
        let set_id = Uuid::from_u128(50);
        let mut snapshot = snapshot();
        snapshot.sets.push(TokenSet {
            id: set_id,
            name: "brand override".into(),
            when: BTreeMap::from([("brand".into(), "acme".into())]),
            extensions: Default::default(),
        });
        snapshot.active_set_ids.push(set_id);
        let original = literal_expression(color("#123456"));
        let base = definition(5, original.clone(), None);
        let mut overlay = definition(5, literal_expression(color("#000000")), None);
        overlay.set_id = Some(set_id);
        snapshot.tokens = vec![base, overlay];
        let mut context = context(true);
        context.context = Some(ResolutionContext {
            explicit: ContextMap::new(),
            ancestors_nearest_first: vec![],
            page: ContextMap::new(),
            document: BTreeMap::from([("brand".into(), "acme".into())]),
        });
        let mut plan = sample_plan();
        plan.operations = vec![
            ProposedOperation::SetTokenValue {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(5),
                },
                context: ContextMap::new(),
                expected_value: Some(literal_expression(color("#000000"))),
                next_value: literal_expression(color("#ff0000")),
            },
            set_property(Some(color("#123456")), binding(5, ModeStrategy::Inherit)),
        ];
        let result = validate_plan(&snapshot, &intent(), &context, &plan);
        assert_eq!(
            result.status,
            ValidationStatus::Ready,
            "{:?}",
            result.diagnostics
        );
        assert_eq!(
            result.impact.property_previews[0].resolution.value,
            Some(TypedValue::Color {
                value: ColorValue {
                    space: "srgb".into(),
                    hex: "#ff0000".into(),
                    alpha: 1.0
                }
            })
        );
        assert_eq!(snapshot.tokens[0].values[0].expression, original);
        assert_eq!(
            snapshot.tokens[1].values[0].expression,
            literal_expression(color("#000000"))
        );
    }

    #[test]
    fn alias_cycle_introduced_by_ordered_operations_rejects_the_whole_plan() {
        let mut snapshot = snapshot();
        let original = literal_expression(color("#123456"));
        snapshot.tokens.push(definition(1, original.clone(), None));
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        plan.operations = vec![
            ProposedOperation::CreateToken {
                definition: definition(
                    2,
                    Expression::Reference {
                        target: TokenReference {
                            namespace: "doc:test".into(),
                            token_id: Uuid::from_u128(1),
                        },
                    },
                    None,
                ),
            },
            ProposedOperation::SetTokenValue {
                target: TokenReference {
                    namespace: "doc:test".into(),
                    token_id: Uuid::from_u128(1),
                },
                context: ContextMap::new(),
                expected_value: Some(original),
                next_value: Expression::Reference {
                    target: TokenReference {
                        namespace: "doc:test".into(),
                        token_id: Uuid::from_u128(2),
                    },
                },
            },
            set_property(Some(color("#123456")), binding(1, ModeStrategy::Inherit)),
        ];
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::AliasCycle));
        assert!(result.impact.property_previews.is_empty());
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn a_late_pre_state_conflict_rejects_the_complete_plan_without_partial_preview() {
        let snapshot = snapshot();
        let context = context(true);
        let intent = intent();
        let mut plan = sample_plan();
        plan.operations.push(set_property(
            Some(color("#000000")),
            binding(1, ModeStrategy::Inherit),
        ));
        let result = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(result.status, ValidationStatus::Rejected);
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::StaleRevision));
        assert!(result.impact.property_previews.is_empty());
    }
}
