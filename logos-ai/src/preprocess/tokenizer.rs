//! Text tokenizer — simple BPE-style tokenizer for text-to-embedding.
//!
//! Converts text prompts into token IDs for text encoder models.

use crate::error::{AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tokenizer configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenizerConfig {
    /// Maximum sequence length (token count).
    pub max_length: usize,
    /// Padding token ID.
    pub pad_token_id: u32,
    /// Unknown token ID.
    pub unk_token_id: u32,
    /// Start-of-sequence token ID.
    pub bos_token_id: u32,
    /// End-of-sequence token ID.
    pub eos_token_id: u32,
    /// Whether to lowercase input.
    pub lowercase: bool,
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self {
            max_length: 77,  // CLIP standard
            pad_token_id: 0,
            unk_token_id: 1,
            bos_token_id: 49406,
            eos_token_id: 49407,
            lowercase: true,
        }
    }
}

impl TokenizerConfig {
    /// Set max sequence length.
    pub fn with_max_length(mut self, len: usize) -> Self {
        self.max_length = len.max(1);
        self
    }

    /// Set lowercase mode.
    pub fn with_lowercase(mut self, lc: bool) -> Self {
        self.lowercase = lc;
        self
    }
}

/// Simple text tokenizer for AI model input.
///
/// In production, this would load a full BPE vocabulary.
/// Currently uses a word-level tokenizer with a small built-in vocab.
pub struct TextTokenizer {
    /// Configuration.
    config: TokenizerConfig,
    /// Vocabulary mapping word → token ID.
    vocab: HashMap<String, u32>,
    /// Reverse mapping token ID → word.
    reverse_vocab: HashMap<u32, String>,
    /// Next available token ID.
    next_id: u32,
}

impl TextTokenizer {
    /// Create a tokenizer with default config.
    pub fn new() -> Self {
        Self::with_config(TokenizerConfig::default())
    }

    /// Create a tokenizer with custom config.
    pub fn with_config(config: TokenizerConfig) -> Self {
        let mut tokenizer = Self {
            config,
            vocab: HashMap::new(),
            reverse_vocab: HashMap::new(),
            next_id: 2, // 0=pad, 1=unk
        };
        tokenizer.build_default_vocab();
        tokenizer
    }

    /// Build a small default vocabulary.
    fn build_default_vocab(&mut self) {
        let words = vec![
            "a", "an", "the", "and", "or", "but", "in", "on", "at", "to",
            "for", "of", "with", "by", "from", "is", "are", "was", "were",
            "be", "been", "have", "has", "had", "do", "does", "did", "will",
            "would", "could", "should", "can", "may", "might", "must",
            "not", "no", "yes", "this", "that", "these", "those",
            // Design vocabulary
            "design", "layout", "color", "style", "font", "text", "image",
            "button", "header", "footer", "sidebar", "card", "grid", "flex",
            "padding", "margin", "border", "background", "foreground",
            "red", "green", "blue", "white", "black", "gray", "yellow",
            "orange", "purple", "pink", "brown", "dark", "light", "bright",
            "beautiful", "modern", "minimal", "clean", "simple", "elegant",
            "bold", "subtle", "vibrant", "muted", "warm", "cool",
            "landscape", "portrait", "abstract", "realistic", "photo",
            "illustration", "icon", "logo", "banner", "poster", "flyer",
            "website", "app", "mobile", "desktop", "responsive",
            "circle", "square", "rectangle", "triangle", "line", "curve",
            "gradient", "shadow", "blur", "opacity", "transparent",
            "small", "medium", "large", "big", "tiny", "huge",
            "top", "bottom", "left", "right", "center", "middle",
            "high", "low", "wide", "narrow", "tall", "short",
            "sunset", "sunrise", "ocean", "mountain", "forest", "sky",
            "flower", "tree", "water", "fire", "earth", "air",
        ];

        for word in words {
            self.add_word(word);
        }
    }

    /// Add a word to the vocabulary.
    pub fn add_word(&mut self, word: &str) -> u32 {
        let key = if self.config.lowercase {
            word.to_lowercase()
        } else {
            word.to_string()
        };

        if let Some(&id) = self.vocab.get(&key) {
            return id;
        }

        let id = self.next_id;
        self.vocab.insert(key.clone(), id);
        self.reverse_vocab.insert(id, key);
        self.next_id += 1;
        id
    }

    /// Vocabulary size.
    pub fn vocab_size(&self) -> usize {
        self.vocab.len() + 2 // +2 for pad and unk
    }

    /// Tokenize text into token IDs.
    ///
    /// Adds BOS at start, tokenizes words, adds EOS, pads to max_length.
    pub fn encode(&self, text: &str) -> AiResult<Vec<u32>> {
        if text.is_empty() {
            return Err(AiError::TokenizationFailed("empty input".into()));
        }

        let normalized = if self.config.lowercase {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        let mut tokens = vec![self.config.bos_token_id];

        // Tokenize each word
        for word in normalized.split_whitespace() {
            // Strip punctuation
            let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
            if clean.is_empty() {
                continue;
            }

            let id = self.vocab.get(&clean).copied().unwrap_or(self.config.unk_token_id);
            tokens.push(id);

            if tokens.len() >= self.config.max_length - 1 {
                break;
            }
        }

        tokens.push(self.config.eos_token_id);

        // Pad to max_length
        while tokens.len() < self.config.max_length {
            tokens.push(self.config.pad_token_id);
        }

        tokens.truncate(self.config.max_length);
        Ok(tokens)
    }

    /// Decode token IDs back to text.
    pub fn decode(&self, tokens: &[u32]) -> String {
        let words: Vec<&str> = tokens
            .iter()
            .filter(|&&id| {
                id != self.config.pad_token_id
                    && id != self.config.bos_token_id
                    && id != self.config.eos_token_id
            })
            .map(|id| {
                self.reverse_vocab
                    .get(id)
                    .map(|s| s.as_str())
                    .unwrap_or("<unk>")
            })
            .collect();
        words.join(" ")
    }

    /// Get token ID for a word.
    pub fn token_id(&self, word: &str) -> Option<u32> {
        let key = if self.config.lowercase {
            word.to_lowercase()
        } else {
            word.to_string()
        };
        self.vocab.get(&key).copied()
    }

    /// Get the config.
    pub fn config(&self) -> &TokenizerConfig {
        &self.config
    }
}

impl Default for TextTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_config_default() {
        let config = TokenizerConfig::default();
        assert_eq!(config.max_length, 77);
        assert_eq!(config.pad_token_id, 0);
        assert!(config.lowercase);
    }

    #[test]
    fn test_tokenizer_config_builder() {
        let config = TokenizerConfig::default()
            .with_max_length(128)
            .with_lowercase(false);
        assert_eq!(config.max_length, 128);
        assert!(!config.lowercase);
    }

    #[test]
    fn test_tokenizer_new() {
        let t = TextTokenizer::new();
        assert!(t.vocab_size() > 50);
    }

    #[test]
    fn test_encode_simple() {
        let t = TextTokenizer::new();
        let tokens = t.encode("a beautiful sunset").unwrap();
        assert_eq!(tokens.len(), 77); // padded to max_length
        assert_eq!(tokens[0], t.config.bos_token_id); // BOS
        assert_ne!(tokens[1], t.config.unk_token_id); // "a" is in vocab
    }

    #[test]
    fn test_encode_empty() {
        let t = TextTokenizer::new();
        assert!(t.encode("").is_err());
    }

    #[test]
    fn test_encode_with_padding() {
        let t = TextTokenizer::new();
        let tokens = t.encode("hello").unwrap();
        assert_eq!(tokens.len(), 77);
        // Most tokens should be padding
        let pad_count = tokens.iter().filter(|&&id| id == 0).count();
        assert!(pad_count > 70);
    }

    #[test]
    fn test_encode_unknown_word() {
        let t = TextTokenizer::new();
        let tokens = t.encode("xyzzy").unwrap();
        // xyzzy should be unknown
        assert_eq!(tokens[1], t.config.unk_token_id);
    }

    #[test]
    fn test_encode_case_insensitive() {
        let t = TextTokenizer::new();
        let tokens1 = t.encode("Design").unwrap();
        let tokens2 = t.encode("design").unwrap();
        assert_eq!(tokens1, tokens2);
    }

    #[test]
    fn test_decode() {
        let t = TextTokenizer::new();
        let tokens = t.encode("a beautiful sunset").unwrap();
        let decoded = t.decode(&tokens);
        assert!(decoded.contains("beautiful"));
        assert!(decoded.contains("sunset"));
    }

    #[test]
    fn test_add_word() {
        let mut t = TextTokenizer::new();
        let id1 = t.add_word("newword");
        let id2 = t.add_word("newword");
        assert_eq!(id1, id2); // same word, same id
    }

    #[test]
    fn test_token_id() {
        let t = TextTokenizer::new();
        assert!(t.token_id("design").is_some());
        assert!(t.token_id("qwertyuiop").is_none());
    }

    #[test]
    fn test_tokenizer_config_serialization() {
        let config = TokenizerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: TokenizerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_length, 77);
    }

    #[test]
    fn test_custom_max_length() {
        let config = TokenizerConfig::default().with_max_length(32);
        let t = TextTokenizer::with_config(config);
        let tokens = t.encode("a beautiful sunset").unwrap();
        assert_eq!(tokens.len(), 32);
    }
}
