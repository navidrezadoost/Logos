//! Pure plan validation. This module never commits or mutates document state.

use sha2::{Digest, Sha256};

use crate::contract::{
    ContextMap, Diagnostic, DiagnosticCode, DocumentContext, Expression, ImpactSummary,
    ModeStrategy, PropertyValue, ProposedOperation, ProposedPlan, ResolutionContext,
    ResolveRequest, TokenReference, TokenSnapshot, UserIntent, ValidationResult, ValidationStatus,
    CONTRACT_VERSION, MAX_PREVIEW_CONTEXTS,
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
        diagnostics: Vec::new(),
    };
    let reject = |diagnostics: Vec<Diagnostic>, impact: ImpactSummary| ValidationResult {
        protocol_version: CONTRACT_VERSION.to_owned(),
        plan_id: plan.plan_id,
        base_revision: plan.base_revision.clone(),
        status: ValidationStatus::Rejected,
        plan_digest: plan_digest.clone(),
        impact,
        diagnostics,
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

    // Work on a private preview copy. It is discarded on every return path.
    let mut preview = snapshot.clone();
    for operation in &plan.operations {
        match operation {
            ProposedOperation::CreateToken { definition } => {
                if !context.allowed_namespaces.contains(&definition.namespace)
                    || preview.tokens.iter().any(|token| {
                        token.namespace == definition.namespace
                            && token.id == definition.id
                            && token.set_id == definition.set_id
                    })
                {
                    diagnostics.push(diagnostic(DiagnosticCode::ScopeDenied));
                    break;
                }
                if !expressions_reference_existing(
                    definition.values.iter().map(|v| &v.expression),
                    &preview,
                ) {
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
                if !expression_references_existing(next_value, &preview) {
                    diagnostics.push(diagnostic(DiagnosticCode::DependencyOrder));
                    break;
                }
                let Some(token) = preview.tokens.iter_mut().find(|token| {
                    token.namespace == target.namespace && token.id == target.token_id
                }) else {
                    diagnostics.push(diagnostic(DiagnosticCode::MissingToken));
                    break;
                };
                let Some(value) = token.values.iter_mut().find(|value| &value.when == when) else {
                    diagnostics.push(diagnostic(DiagnosticCode::DependencyOrder));
                    break;
                };
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
                let Some(property) = context.property_values.iter().find(|property| {
                    property.shape_id == *shape_id && property.property_path == *property_path
                }) else {
                    diagnostics.push(diagnostic(DiagnosticCode::MissingToken));
                    break;
                };
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
                    if !reference_exists(target, &preview) {
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
                impact.bound_properties += 1;
                impact.affected_property_paths.push(property_path.clone());
            }
        }
        if !diagnostics.is_empty() {
            break;
        }
    }

    if diagnostics.is_empty() {
        diagnostics.extend(validate_dependency_graph(&preview));
    }
    if diagnostics.is_empty() {
        let mut contexts = plan.preview_contexts.clone();
        if contexts.is_empty() {
            contexts.push(ContextMap::new());
        }
        diagnostics.extend(validate_preview_resolution(&preview, &contexts));
    }
    if !diagnostics.is_empty() {
        impact.diagnostics = diagnostics.clone();
        return reject(diagnostics, impact);
    }
    impact.affected_property_paths.sort();
    impact.affected_property_paths.dedup();
    let mut contexts = plan.preview_contexts.clone();
    if contexts.is_empty() {
        contexts.push(ContextMap::new());
    }
    impact.contexts_evaluated = contexts;
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
    let bytes = serde_json::to_vec(plan).expect("typed plan serializes");
    format!("sha256:{}", hex(&Sha256::digest(bytes)))
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
            selected_shape_ids: vec![],
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
    fn plan() -> ProposedPlan {
        ProposedPlan {
            protocol_version: CONTRACT_VERSION.into(),
            plan_id: Uuid::nil(),
            base_revision: "rev-1".into(),
            intent_id: Uuid::nil(),
            summary: "create color".into(),
            operations: vec![ProposedOperation::CreateToken {
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
            }],
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
        let plan = plan();
        let before = serde_json::to_vec(&(&snapshot, &context)).unwrap();
        let first = validate_plan(&snapshot, &intent, &context, &plan);
        let second = validate_plan(&snapshot, &intent, &context, &plan);
        assert_eq!(first.status, ValidationStatus::Ready);
        assert_eq!(first, second);
        assert_eq!(first.impact.created_tokens, 1);
        assert_eq!(serde_json::to_vec(&(&snapshot, &context)).unwrap(), before);
    }

    #[test]
    fn missing_permission_and_stale_revision_fail_closed_without_mutation() {
        let snapshot = snapshot();
        let intent = intent();
        let context = context(false);
        let mut plan = plan();
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
        let mut plan = plan();
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
}
