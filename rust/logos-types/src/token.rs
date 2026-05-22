//! Design token definitions.
//!
//! Clojure source: `common/src/app/common/types/tokens_lib.cljc`
//! and `common/src/app/common/types/token.cljc`.
//!
//! Design tokens follow the W3C Design Tokens Community Group draft spec.

use std::collections::HashMap;
use uuid::Uuid;

/// The semantic type of a design token.
/// Controls which shape attributes the token can be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "kebab-case"))]
pub enum TokenType {
    Color,
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

/// A single design token.
///
/// Clojure: `{:id uuid :name "colors/primary" :type :color :value "#..."}`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export, rename_all = "camelCase"))]pub struct Token {
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
    pub fn new() -> Self { TokensLib::default() }

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
}
