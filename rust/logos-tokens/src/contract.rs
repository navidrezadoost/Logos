//! Typed, versioned DTOs for the proposed design-token contract.
//!
//! These types are an in-memory domain boundary. They do not define how a
//! document is serialized or persisted. Known objects reject unknown fields;
//! vendor extensions are carried explicitly under `extensions`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

pub const CONTRACT_VERSION: &str = "0.1.0";
pub const MAX_ALIAS_HOPS: usize = 32;
pub const MAX_EXPRESSION_DEPTH: usize = 32;
pub const MAX_EXPRESSION_NODES: usize = 256;
pub const MAX_CONTEXT_DIMENSIONS: usize = 8;
pub const MAX_PREVIEW_CONTEXTS: usize = 128;
pub const MAX_ABS_NUMBER: f64 = 1_000_000_000_000.0;

pub type ContextMap = BTreeMap<String, String>;
pub type Extensions = BTreeMap<String, JsonValue>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContractStatus {
    Proposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenType {
    Color,
    Number,
    Dimension,
    String,
    Boolean,
    Opacity,
    FontFamily,
    FontWeight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimensionUnit {
    Px,
    Ms,
    S,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorValue {
    pub space: String,
    pub hex: String,
    pub alpha: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum TypedValue {
    Color { value: ColorValue },
    Number { value: f64 },
    Dimension { value: f64, unit: DimensionUnit },
    String { value: String },
    Boolean { value: bool },
    Opacity { value: f64 },
    FontFamily { value: String },
    FontWeight { value: f64 },
}

impl TypedValue {
    pub fn token_type(&self) -> TokenType {
        match self {
            Self::Color { .. } => TokenType::Color,
            Self::Number { .. } => TokenType::Number,
            Self::Dimension { .. } => TokenType::Dimension,
            Self::String { .. } => TokenType::String,
            Self::Boolean { .. } => TokenType::Boolean,
            Self::Opacity { .. } => TokenType::Opacity,
            Self::FontFamily { .. } => TokenType::FontFamily,
            Self::FontWeight { .. } => TokenType::FontWeight,
        }
    }

    pub fn validate(&self) -> Result<(), DiagnosticCode> {
        match self {
            Self::Color { value } => {
                let hex = value.hex.as_bytes();
                if value.space != "srgb"
                    || hex.len() != 7
                    || hex[0] != b'#'
                    || !hex[1..].iter().all(u8::is_ascii_hexdigit)
                    || !value.alpha.is_finite()
                    || !(0.0..=1.0).contains(&value.alpha)
                {
                    return Err(DiagnosticCode::ExpressionInvalid);
                }
            }
            Self::Number { value } => validate_number(*value)?,
            Self::FontWeight { value } => {
                validate_number(*value)?;
                if !(1.0..=1_000.0).contains(value) {
                    return Err(DiagnosticCode::NumericRange);
                }
            }
            Self::Dimension { value, .. } => validate_number(*value)?,
            Self::Opacity { value } => {
                validate_number(*value)?;
                if !(0.0..=1.0).contains(value) {
                    return Err(DiagnosticCode::NumericRange);
                }
            }
            Self::FontFamily { value } if value.is_empty() => {
                return Err(DiagnosticCode::ExpressionInvalid)
            }
            Self::String { .. } | Self::Boolean { .. } | Self::FontFamily { .. } => {}
        }
        Ok(())
    }
}

pub fn validate_number(value: f64) -> Result<(), DiagnosticCode> {
    if !value.is_finite() || value.abs() > MAX_ABS_NUMBER {
        Err(DiagnosticCode::NumericRange)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenReference {
    pub namespace: String,
    pub token_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Expression {
    Literal {
        value: TypedValue,
    },
    Reference {
        target: TokenReference,
    },
    Operation {
        operator: ExpressionOperator,
        arguments: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpressionOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Clamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenValueDefinition {
    #[serde(default)]
    pub when: ContextMap,
    pub expression: Expression,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenDefinition {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    #[serde(rename = "type")]
    pub token_type: TokenType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_id: Option<Uuid>,
    pub values: Vec<TokenValueDefinition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Mode {
    pub id: Uuid,
    pub option: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub mode_dimension: String,
    pub default_mode_id: Uuid,
    pub modes: Vec<Mode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenSet {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub when: ContextMap,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextDimension {
    pub id: Uuid,
    pub name: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TokenSnapshot {
    pub contract_version: String,
    pub status: ContractStatus,
    pub revision: String,
    pub document_namespace: String,
    pub tokens: Vec<TokenDefinition>,
    pub collections: Vec<Collection>,
    pub sets: Vec<TokenSet>,
    pub context_dimensions: Vec<ContextDimension>,
    pub active_set_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ModeStrategy {
    Inherit,
    Explicit { mode_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PropertyValue {
    Literal {
        value: TypedValue,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        extensions: Extensions,
    },
    Binding {
        target: TokenReference,
        mode_strategy: ModeStrategy,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback: Option<TypedValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        local_override: Option<TypedValue>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        extensions: Extensions,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResolutionContext {
    #[serde(default)]
    pub explicit: ContextMap,
    #[serde(default)]
    pub ancestors_nearest_first: Vec<ContextMap>,
    #[serde(default)]
    pub page: ContextMap,
    #[serde(default)]
    pub document: ContextMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResolveRequest {
    pub contract_version: String,
    pub snapshot_revision: String,
    pub property_type: TokenType,
    pub property: PropertyValue,
    pub context: ResolutionContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    MissingToken,
    MissingMode,
    AliasCycle,
    AliasBudget,
    TypeMismatch,
    ScopeDenied,
    ExpressionInvalid,
    ExpressionBudget,
    ContextBudget,
    DivisionByZero,
    ContextConflict,
    StaleRevision,
    UnknownExtension,
    UnsupportedFeature,
    NumericRange,
    InvalidSnapshot,
    InvalidPlan,
    DependencyOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_chain: Vec<TokenReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolveStatus {
    Resolved,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionSource {
    Direct,
    Token,
    LocalOverride,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ResolveResult {
    pub contract_version: String,
    pub snapshot_revision: String,
    pub status: ResolveStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<TypedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ResolutionSource>,
    pub diagnostics: Vec<Diagnostic>,
    pub dependency_trace: Vec<TokenReference>,
    pub effective_context: ContextMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserIntent {
    pub protocol_version: String,
    pub intent_id: Uuid,
    pub text: String,
    pub requested_capabilities: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentContext {
    pub protocol_version: String,
    pub document_id: Uuid,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_id: Option<Uuid>,
    pub selected_shape_ids: Vec<Uuid>,
    pub property_values: Vec<DocumentProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ResolutionContext>,
    pub allowed_namespaces: Vec<String>,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProperty {
    pub shape_id: Uuid,
    pub property_path: String,
    pub value: PropertyValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProposedOperation {
    CreateToken {
        definition: TokenDefinition,
    },
    SetTokenValue {
        target: TokenReference,
        #[serde(default)]
        context: ContextMap,
        #[serde(default)]
        expected_value: Option<Expression>,
        next_value: Expression,
    },
    SetProperty {
        shape_id: Uuid,
        property_path: String,
        #[serde(default)]
        expected_property: Option<PropertyValue>,
        next_property: PropertyValue,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedPlan {
    pub protocol_version: String,
    pub plan_id: Uuid,
    pub base_revision: String,
    pub intent_id: Uuid,
    pub summary: String,
    pub operations: Vec<ProposedOperation>,
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub preview_contexts: Vec<ContextMap>,
    pub requires_user_approval: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: Extensions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImpactSummary {
    pub created_tokens: usize,
    pub updated_tokens: usize,
    pub bound_properties: usize,
    pub affected_property_paths: Vec<String>,
    pub contexts_evaluated: Vec<ContextMap>,
    pub property_previews: Vec<PropertyPreview>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyPreview {
    pub shape_id: Uuid,
    pub property_path: String,
    pub context: ContextMap,
    pub property_value: PropertyValue,
    pub resolution: ResolveResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValidationStatus {
    Ready,
    Rejected,
    NeedsConfirmation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationResult {
    pub protocol_version: String,
    pub plan_id: Uuid,
    pub base_revision: String,
    pub status: ValidationStatus,
    pub plan_digest: String,
    pub impact: ImpactSummary,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitRequest {
    pub protocol_version: String,
    pub plan_id: Uuid,
    pub expected_revision: String,
    pub validation_digest: String,
    pub explicit_user_approval: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReceiptStatus {
    Applied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransactionReceipt {
    pub protocol_version: String,
    pub transaction_id: Uuid,
    pub status: ReceiptStatus,
    pub base_revision: String,
    pub resulting_revision: String,
    pub operation_ids: Vec<String>,
    pub actor_id: String,
    pub approved_by_user: bool,
    pub undo_reference: String,
    pub replay_reference: String,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}
