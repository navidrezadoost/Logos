//! Design token definitions.
//!
//! Clojure source: `common/src/app/common/types/tokens_lib.cljc`
//! and `common/src/app/common/types/token.cljc`.
//!
//! Design tokens follow the W3C Design Tokens Community Group draft spec.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

pub const TOKEN_REGISTRY_SCHEMA_VERSION: u32 = 1;

/// The semantic type of a design token.
/// Controls which shape attributes the token can be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum TokenType {
    Color,
    Number,
    String,
    Boolean,
    Spacing,
    Sizing,
    FontFamily,
    FontSize,
    FontWeight,
    FontStyle,
    LineHeight,
    LetterSpacing,
    Opacity,
    Rotation,
    BorderRadius,
    StrokeWidth,
    Duration,
    Timing,
    Dimension,
    Shadow,
    Blur,
    Typography,
    Other,
}

/// Property-level places where a semantic token can be applied in the UI.
///
/// This intentionally stays separate from contract-level design aspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum PropertyScope {
    Fill,
    Stroke,
    TextColor,
    Background,
    Border,
    Shadow,
    Opacity,
}

/// Contract-level semantic aspect.
///
/// Mapping from [`PropertyScope`] to `DesignAspect` belongs in resolver/UI
/// code later, not in this registry model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum DesignAspect {
    Background,
    Text,
    Border,
    Shadow,
    Opacity,
}

/// Origin of a token. V1 is file-local; library and remote sources are stored
/// without changing the registry shape later.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "scope", rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, tag = "scope", rename_all = "kebab-case"))]
pub enum TokenSource {
    File,
    Library { library_id: Uuid },
    Remote { library_id: Uuid },
}

/// The Rust core entity behind the UI's "variable" projection.
///
/// Values are deliberately not stored here. A semantic token owns identity,
/// naming, type, scope, and source; per-mode values live in [`TokenValue`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct SemanticToken {
    pub id: Uuid,
    pub display_name: String,
    pub technical_name: String,
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub token_type: TokenType,
    pub collection_id: Uuid,
    pub scopes: Vec<PropertyScope>,
    pub source: TokenSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SemanticToken {
    pub fn new(
        id: Uuid,
        display_name: impl Into<String>,
        technical_name: impl Into<String>,
        token_type: TokenType,
        collection_id: Uuid,
        scopes: Vec<PropertyScope>,
        source: TokenSource,
    ) -> Self {
        let now = Utc::now();
        SemanticToken {
            id,
            display_name: display_name.into(),
            technical_name: technical_name.into(),
            token_type,
            collection_id,
            scopes,
            source,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A raw or aliased value for a token in a specific mode.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "type", content = "value", rename_all = "kebab-case")
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export, tag = "type", content = "value", rename_all = "kebab-case")
)]
pub enum TokenValueKind {
    Color(String),
    Number(f64),
    String(String),
    Boolean(bool),
    Alias(Uuid),
}

/// A semantic token value for one mode.
///
/// Aliases live here, not on [`SemanticToken`], so a token can be raw in one
/// mode and aliased in another.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct TokenValue {
    pub token_id: Uuid,
    pub mode_id: Uuid,
    pub value: TokenValueKind,
}

/// V1 keeps primitive tokens available without forcing all semantic tokens to
/// normalize into primitives immediately.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct PrimitiveToken {
    pub id: Uuid,
    pub display_name: String,
    pub technical_name: String,
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub token_type: TokenType,
    pub value: TokenValueKind,
    pub source: TokenSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PrimitiveToken {
    pub fn new(
        id: Uuid,
        display_name: impl Into<String>,
        technical_name: impl Into<String>,
        token_type: TokenType,
        value: TokenValueKind,
        source: TokenSource,
    ) -> Self {
        let now = Utc::now();
        PrimitiveToken {
            id,
            display_name: display_name.into(),
            technical_name: technical_name.into(),
            token_type,
            value,
            source,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A collection groups semantic tokens and owns the available modes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct VariableCollection {
    pub id: Uuid,
    pub name: String,
    pub mode_ids: Vec<Uuid>,
    pub default_mode_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VariableCollection {
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        mode_ids: Vec<Uuid>,
        default_mode_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        VariableCollection {
            id,
            name: name.into(),
            mode_ids,
            default_mode_id,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A mode within a collection, such as Light or Dark.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct VariableMode {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VariableMode {
    pub fn new(id: Uuid, collection_id: Uuid, name: impl Into<String>) -> Self {
        let now = Utc::now();
        VariableMode {
            id,
            collection_id,
            name: name.into(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// How a property binding chooses a mode during resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "strategy", content = "mode-id", rename_all = "kebab-case")
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(
        export,
        tag = "strategy",
        content = "modeId",
        rename_all = "kebab-case"
    )
)]
pub enum ModeStrategy {
    Inherit,
    Explicit(Uuid),
}

/// Shape/property binding to a semantic token.
///
/// The raw fallback lets a design recover if the token is removed, missing, or
/// corrupted without requiring resolver work in this first model commit.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct VariableBinding {
    pub token_id: Uuid,
    pub mode_strategy: ModeStrategy,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub fallback_value: Option<TokenValueKind>,
}

/// Schema-versioned semantic token registry.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct TokenRegistry {
    pub schema_version: u32,
    pub collections: HashMap<Uuid, VariableCollection>,
    pub modes: HashMap<Uuid, VariableMode>,
    pub semantic_tokens: HashMap<Uuid, SemanticToken>,
    pub token_values: Vec<TokenValue>,
    pub primitive_tokens: HashMap<Uuid, PrimitiveToken>,
}

impl Default for TokenRegistry {
    fn default() -> Self {
        TokenRegistry {
            schema_version: TOKEN_REGISTRY_SCHEMA_VERSION,
            collections: HashMap::new(),
            modes: HashMap::new(),
            semantic_tokens: HashMap::new(),
            token_values: Vec::new(),
            primitive_tokens: HashMap::new(),
        }
    }
}

impl TokenRegistry {
    pub fn new() -> Self {
        TokenRegistry::default()
    }

    pub fn add_collection(&mut self, collection: VariableCollection) -> Option<VariableCollection> {
        self.collections.insert(collection.id, collection)
    }

    pub fn update_collection(
        &mut self,
        collection: VariableCollection,
    ) -> Option<VariableCollection> {
        self.collections.insert(collection.id, collection)
    }

    pub fn remove_collection(&mut self, id: Uuid) -> Option<VariableCollection> {
        let removed = self.collections.remove(&id)?;
        let removed_modes: Vec<Uuid> = removed.mode_ids.clone();
        for mode_id in &removed_modes {
            self.modes.remove(mode_id);
        }

        let removed_tokens: Vec<Uuid> = self
            .semantic_tokens
            .values()
            .filter(|token| token.collection_id == id)
            .map(|token| token.id)
            .collect();
        for token_id in &removed_tokens {
            self.semantic_tokens.remove(token_id);
        }

        self.token_values.retain(|value| {
            !removed_tokens.contains(&value.token_id) && !removed_modes.contains(&value.mode_id)
        });

        Some(removed)
    }

    pub fn add_mode(&mut self, mode: VariableMode) -> Option<VariableMode> {
        self.modes.insert(mode.id, mode)
    }

    pub fn update_mode(&mut self, mode: VariableMode) -> Option<VariableMode> {
        self.modes.insert(mode.id, mode)
    }

    pub fn remove_mode(&mut self, id: Uuid) -> Option<VariableMode> {
        let removed = self.modes.remove(&id)?;
        for collection in self.collections.values_mut() {
            collection.mode_ids.retain(|mode_id| *mode_id != id);
        }
        self.token_values.retain(|value| value.mode_id != id);
        Some(removed)
    }

    pub fn add_semantic_token(&mut self, token: SemanticToken) -> Option<SemanticToken> {
        self.semantic_tokens.insert(token.id, token)
    }

    pub fn update_semantic_token(&mut self, token: SemanticToken) -> Option<SemanticToken> {
        self.semantic_tokens.insert(token.id, token)
    }

    pub fn remove_semantic_token(&mut self, id: Uuid) -> Option<SemanticToken> {
        let removed = self.semantic_tokens.remove(&id)?;
        self.token_values.retain(|value| value.token_id != id);
        Some(removed)
    }

    pub fn add_token_value(&mut self, value: TokenValue) -> Option<TokenValue> {
        self.upsert_token_value(value)
    }

    pub fn update_token_value(&mut self, value: TokenValue) -> Option<TokenValue> {
        self.upsert_token_value(value)
    }

    pub fn remove_token_value(&mut self, token_id: Uuid, mode_id: Uuid) -> Option<TokenValue> {
        let index = self.token_value_index(token_id, mode_id)?;
        Some(self.token_values.remove(index))
    }

    pub fn token_value(&self, token_id: Uuid, mode_id: Uuid) -> Option<&TokenValue> {
        self.token_values
            .iter()
            .find(|value| value.token_id == token_id && value.mode_id == mode_id)
    }

    fn upsert_token_value(&mut self, value: TokenValue) -> Option<TokenValue> {
        if let Some(index) = self.token_value_index(value.token_id, value.mode_id) {
            Some(std::mem::replace(&mut self.token_values[index], value))
        } else {
            self.token_values.push(value);
            None
        }
    }

    fn token_value_index(&self, token_id: Uuid, mode_id: Uuid) -> Option<usize> {
        self.token_values
            .iter()
            .position(|value| value.token_id == token_id && value.mode_id == mode_id)
    }
}

/// A single design token.
///
/// Clojure: `{:id uuid :name "colors/primary" :type :color :value "#..."}`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct Token {
    pub id: Uuid,
    /// Slash-delimited name path, e.g. `"colors/primary"`.
    pub name: String,
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub token_type: TokenType,
    /// Raw string value (may be a literal or a `{other.token}` reference).
    pub value: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub description: Option<String>,
}

impl Token {
    pub fn new(
        id: Uuid,
        name: impl Into<String>,
        token_type: TokenType,
        value: impl Into<String>,
    ) -> Self {
        Token {
            id,
            name: name.into(),
            token_type,
            value: value.into(),
            description: None,
        }
    }
}

/// A group of tokens sharing the same path prefix.
/// Clojure: a nested map inside a `TokensLib` set.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct TokenGroup {
    pub name: String,
    pub tokens: HashMap<String, Token>,
}

/// The top-level token library for a file.
/// Clojure: `tokens-lib` atom — a map of set-name → `TokenSet`.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]
pub struct TokensLib {
    /// Set name → flat map of token path → Token.
    pub sets: HashMap<String, HashMap<String, Token>>,
    /// Ordered list of active set names (resolve order).
    #[cfg_attr(feature = "serde", serde(default))]
    pub active_sets: Vec<String>,
}

impl TokensLib {
    pub fn new() -> Self {
        TokensLib::default()
    }

    /// Insert a token into `set_name`, creating the set if absent.
    pub fn insert(&mut self, set_name: impl Into<String>, token: Token) {
        self.sets
            .entry(set_name.into())
            .or_default()
            .insert(token.name.clone(), token);
    }

    /// Resolve a token value, following `{references}` one level deep.
    pub fn resolve(&self, token_path: &str) -> Option<&str> {
        for set in self.sets.values() {
            if let Some(t) = set.get(token_path) {
                return Some(&t.value);
            }
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_token(collection_id: Uuid) -> SemanticToken {
        SemanticToken::new(
            Uuid::new_v4(),
            "Brand Blue",
            "brand-blue",
            TokenType::Color,
            collection_id,
            vec![PropertyScope::Fill, PropertyScope::TextColor],
            TokenSource::File,
        )
    }

    #[test]
    fn insert_and_resolve() {
        let mut lib = TokensLib::new();
        let id = Uuid::new_v4();
        let t = Token::new(id, "colors/primary", TokenType::Color, "#ff0000");
        lib.insert("global", t);

        let v = lib.resolve("colors/primary").unwrap();
        assert_eq!(v, "#ff0000");
    }

    #[test]
    fn resolve_missing_returns_none() {
        let lib = TokensLib::new();
        assert!(lib.resolve("nope").is_none());
    }

    #[test]
    fn token_registry_defaults_schema_version() {
        let registry = TokenRegistry::new();
        assert_eq!(registry.schema_version, TOKEN_REGISTRY_SCHEMA_VERSION);
    }

    #[test]
    fn token_registry_add_update_remove_collection() {
        let mut registry = TokenRegistry::new();
        let collection_id = Uuid::new_v4();
        let light_id = Uuid::new_v4();
        let dark_id = Uuid::new_v4();

        let collection = VariableCollection::new(collection_id, "Brand", vec![light_id], light_id);
        assert!(registry.add_collection(collection).is_none());
        assert!(registry.collections.contains_key(&collection_id));

        let updated = VariableCollection::new(
            collection_id,
            "Brand Updated",
            vec![light_id, dark_id],
            light_id,
        );
        let previous = registry.update_collection(updated).unwrap();
        assert_eq!(previous.name, "Brand");
        assert_eq!(
            registry
                .collections
                .get(&collection_id)
                .unwrap()
                .mode_ids
                .len(),
            2
        );

        let removed = registry.remove_collection(collection_id).unwrap();
        assert_eq!(removed.name, "Brand Updated");
        assert!(!registry.collections.contains_key(&collection_id));
    }

    #[test]
    fn token_registry_add_update_remove_semantic_token() {
        let mut registry = TokenRegistry::new();
        let collection_id = Uuid::new_v4();
        let mut token = semantic_token(collection_id);
        let token_id = token.id;

        assert!(registry.add_semantic_token(token.clone()).is_none());
        assert!(registry.semantic_tokens.contains_key(&token_id));

        token.display_name = "Brand Blue Updated".into();
        let previous = registry.update_semantic_token(token).unwrap();
        assert_eq!(previous.display_name, "Brand Blue");
        assert_eq!(
            registry
                .semantic_tokens
                .get(&token_id)
                .unwrap()
                .display_name,
            "Brand Blue Updated"
        );

        let removed = registry.remove_semantic_token(token_id).unwrap();
        assert_eq!(removed.display_name, "Brand Blue Updated");
        assert!(!registry.semantic_tokens.contains_key(&token_id));
    }

    #[test]
    fn token_registry_add_update_remove_token_value() {
        let mut registry = TokenRegistry::new();
        let token_id = Uuid::new_v4();
        let mode_id = Uuid::new_v4();

        let raw = TokenValue {
            token_id,
            mode_id,
            value: TokenValueKind::Color("#0055ff".into()),
        };
        assert!(registry.add_token_value(raw).is_none());
        assert_eq!(
            registry.token_value(token_id, mode_id).unwrap().value,
            TokenValueKind::Color("#0055ff".into())
        );

        let alias_target_id = Uuid::new_v4();
        let alias = TokenValue {
            token_id,
            mode_id,
            value: TokenValueKind::Alias(alias_target_id),
        };
        let previous = registry.update_token_value(alias).unwrap();
        assert_eq!(previous.value, TokenValueKind::Color("#0055ff".into()));
        assert_eq!(
            registry.token_value(token_id, mode_id).unwrap().value,
            TokenValueKind::Alias(alias_target_id)
        );

        let removed = registry.remove_token_value(token_id, mode_id).unwrap();
        assert_eq!(removed.value, TokenValueKind::Alias(alias_target_id));
        assert!(registry.token_value(token_id, mode_id).is_none());
    }

    #[test]
    fn rust_core_has_no_entity_named_variable() {
        let source = include_str!("token.rs");
        let public_struct = ["pub struct ", "Variable {"].concat();
        let private_struct = ["struct ", "Variable {"].concat();
        assert!(!source.contains(&public_struct));
        assert!(!source.contains(&private_struct));
    }

    #[cfg(feature = "serde")]
    mod serialization {
        use super::*;

        #[test]
        fn semantic_token_serializes_and_deserializes() {
            let token = semantic_token(Uuid::new_v4());
            let json = serde_json::to_string(&token).unwrap();
            let decoded: SemanticToken = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, token);
        }

        #[test]
        fn token_value_raw_color_serializes_and_deserializes() {
            let value = TokenValue {
                token_id: Uuid::new_v4(),
                mode_id: Uuid::new_v4(),
                value: TokenValueKind::Color("#0055ff".into()),
            };
            let json = serde_json::to_string(&value).unwrap();
            let decoded: TokenValue = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, value);
        }

        #[test]
        fn token_value_alias_serializes_and_deserializes() {
            let value = TokenValue {
                token_id: Uuid::new_v4(),
                mode_id: Uuid::new_v4(),
                value: TokenValueKind::Alias(Uuid::new_v4()),
            };
            let json = serde_json::to_string(&value).unwrap();
            let decoded: TokenValue = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, value);
        }

        #[test]
        fn variable_collection_with_light_dark_modes_serializes_and_deserializes() {
            let collection_id = Uuid::new_v4();
            let light_id = Uuid::new_v4();
            let dark_id = Uuid::new_v4();
            let collection =
                VariableCollection::new(collection_id, "Brand", vec![light_id, dark_id], light_id);
            let modes = vec![
                VariableMode::new(light_id, collection_id, "Light"),
                VariableMode::new(dark_id, collection_id, "Dark"),
            ];

            let json = serde_json::to_string(&(collection.clone(), modes.clone())).unwrap();
            let decoded: (VariableCollection, Vec<VariableMode>) =
                serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, (collection, modes));
        }

        #[test]
        fn variable_binding_serializes_and_deserializes() {
            let binding = VariableBinding {
                token_id: Uuid::new_v4(),
                mode_strategy: ModeStrategy::Explicit(Uuid::new_v4()),
                fallback_value: Some(TokenValueKind::Color("#0033aa".into())),
            };
            let json = serde_json::to_string(&binding).unwrap();
            let decoded: VariableBinding = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, binding);
        }
    }
}
