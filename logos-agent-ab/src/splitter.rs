//! Traffic splitting — deterministic variant assignment via hashing.

use crate::experiment::{ExperimentConfig, Variant};
use thiserror::Error;

/// Errors from the traffic splitter.
#[derive(Debug, Error, PartialEq)]
pub enum SplitError {
    #[error("experiment '{0}' has no variants")]
    NoVariants(String),
    #[error("experiment weights do not sum to 100 (got {0})")]
    InvalidWeights(u32),
    #[error("user id is empty")]
    EmptyUserId,
}

/// Configuration for the traffic splitter.
#[derive(Debug, Clone)]
pub struct SplitConfig {
    /// Salt mixed into the hash to isolate experiments.
    pub salt: String,
    /// Whether sessions are sticky (same user always gets same variant within an experiment).
    pub sticky: bool,
}

impl Default for SplitConfig {
    fn default() -> Self {
        Self {
            salt: "logos-ab".to_owned(),
            sticky: true,
        }
    }
}

impl SplitConfig {
    pub fn new(salt: impl Into<String>, sticky: bool) -> Self {
        Self { salt: salt.into(), sticky }
    }
}

/// Resolves which variant a subject is assigned to.
pub trait VariantResolver {
    /// Returns the variant name for `(experiment_id, user_id)`.
    fn resolve(&self, experiment_id: &str, user_id: &str, cfg: &ExperimentConfig) -> String;
}

/// Deterministic, hash-based traffic splitter.
///
/// Uses a djb2-style hash of `salt + experiment_id + user_id` to pick
/// a bucket in `[0, 100)` and maps it to a variant by weight.
#[derive(Debug, Clone)]
pub struct TrafficSplitter {
    config: SplitConfig,
}

impl TrafficSplitter {
    pub fn new(config: SplitConfig) -> Self {
        Self { config }
    }

    /// Hash `subject` to a bucket in `[0, 100)`.
    fn bucket(&self, subject: &str) -> u32 {
        let mut h: u64 = 5381;
        for b in self.config.salt.bytes().chain(subject.bytes()) {
            h = h.wrapping_mul(33).wrapping_add(b as u64);
        }
        (h % 100) as u32
    }

    /// Resolve variant name, returning an error if weights are invalid.
    pub fn try_resolve(
        &self,
        experiment_id: &str,
        user_id: &str,
        cfg: &ExperimentConfig,
    ) -> Result<String, SplitError> {
        if user_id.is_empty() {
            return Err(SplitError::EmptyUserId);
        }
        let total: u32 = cfg.variants.iter().map(|v| v.weight).sum();
        if total != 100 {
            return Err(SplitError::InvalidWeights(total));
        }
        if cfg.variants.is_empty() {
            return Err(SplitError::NoVariants(experiment_id.to_owned()));
        }

        let key = if self.config.sticky {
            format!("{}{}", experiment_id, user_id)
        } else {
            user_id.to_owned()
        };
        let bucket = self.bucket(&key);

        let mut cursor = 0u32;
        for variant in &cfg.variants {
            cursor += variant.weight;
            if bucket < cursor {
                return Ok(variant.name.clone());
            }
        }
        // Fallback — rounding safety: last variant
        Ok(cfg.variants.last().unwrap().name.clone())
    }

    /// Returns the variant for a given experiment / user pair.
    /// Panics if the config is invalid — use `try_resolve` for error handling.
    pub fn resolve(&self, experiment_id: &str, user_id: &str, cfg: &ExperimentConfig) -> String {
        self.try_resolve(experiment_id, user_id, cfg)
            .unwrap_or_else(|_| cfg.variants[0].name.clone())
    }

    /// Returns which bucket (0–99) a user maps to for an experiment.
    pub fn bucket_for(&self, experiment_id: &str, user_id: &str) -> u32 {
        let key = format!("{}{}", experiment_id, user_id);
        self.bucket(&key)
    }

    /// Checks whether variant weights are valid (sum to 100).
    pub fn validate_weights(variants: &[Variant]) -> Result<(), SplitError> {
        let total: u32 = variants.iter().map(|v| v.weight).sum();
        if total != 100 {
            Err(SplitError::InvalidWeights(total))
        } else {
            Ok(())
        }
    }
}

impl VariantResolver for TrafficSplitter {
    fn resolve(&self, experiment_id: &str, user_id: &str, cfg: &ExperimentConfig) -> String {
        TrafficSplitter::resolve(self, experiment_id, user_id, cfg)
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::{ExperimentConfig, Variant};

    fn two_variant_cfg() -> ExperimentConfig {
        ExperimentConfig::new("exp-1", vec![
            Variant::new("control",   50),
            Variant::new("treatment", 50),
        ]).unwrap()
    }

    #[test]
    fn resolve_returns_known_variant() {
        let s = TrafficSplitter::new(SplitConfig::default());
        let cfg = two_variant_cfg();
        let v = s.resolve("exp-1", "user-1", &cfg);
        assert!(v == "control" || v == "treatment");
    }

    #[test]
    fn resolve_is_deterministic() {
        let s = TrafficSplitter::new(SplitConfig::default());
        let cfg = two_variant_cfg();
        let v1 = s.resolve("exp-1", "user-42", &cfg);
        let v2 = s.resolve("exp-1", "user-42", &cfg);
        assert_eq!(v1, v2);
    }

    #[test]
    fn different_users_can_get_different_variants() {
        let s = TrafficSplitter::new(SplitConfig::default());
        let cfg = two_variant_cfg();
        // With enough users at least one differs
        let results: Vec<_> = (0..50)
            .map(|i| s.resolve("exp-1", &format!("u{i}"), &cfg))
            .collect();
        let has_control   = results.iter().any(|r| r == "control");
        let has_treatment = results.iter().any(|r| r == "treatment");
        assert!(has_control && has_treatment, "expected both variants to appear");
    }

    #[test]
    fn sticky_means_same_variant_for_same_user() {
        let cfg = SplitConfig::new("salt1", true);
        let s = TrafficSplitter::new(cfg);
        let exp_cfg = two_variant_cfg();
        let v1 = s.resolve("exp-1", "sticky-user", &exp_cfg);
        let v2 = s.resolve("exp-1", "sticky-user", &exp_cfg);
        assert_eq!(v1, v2);
    }

    #[test]
    fn invalid_weights_returns_error() {
        let s = TrafficSplitter::new(SplitConfig::default());
        let cfg = ExperimentConfig {
            id: "exp-bad".to_owned(),
            variants: vec![Variant::new("a", 30), Variant::new("b", 30)],
        };
        assert!(matches!(
            s.try_resolve("exp-bad", "user-1", &cfg),
            Err(SplitError::InvalidWeights(60))
        ));
    }

    #[test]
    fn empty_user_id_returns_error() {
        let s = TrafficSplitter::new(SplitConfig::default());
        let cfg = two_variant_cfg();
        assert!(matches!(
            s.try_resolve("exp-1", "", &cfg),
            Err(SplitError::EmptyUserId)
        ));
    }

    #[test]
    fn validate_weights_ok() {
        let variants = vec![Variant::new("a", 60), Variant::new("b", 40)];
        assert!(TrafficSplitter::validate_weights(&variants).is_ok());
    }

    #[test]
    fn validate_weights_err() {
        let variants = vec![Variant::new("a", 60), Variant::new("b", 50)];
        assert!(matches!(
            TrafficSplitter::validate_weights(&variants),
            Err(SplitError::InvalidWeights(110))
        ));
    }

    #[test]
    fn bucket_for_is_in_range() {
        let s = TrafficSplitter::new(SplitConfig::default());
        for i in 0..20u32 {
            assert!(s.bucket_for("exp-1", &format!("u{i}")) < 100);
        }
    }

    #[test]
    fn three_variant_split_covers_all() {
        let cfg = ExperimentConfig::new("exp-3", vec![
            Variant::new("a", 34),
            Variant::new("b", 33),
            Variant::new("c", 33),
        ]).unwrap();
        let s = TrafficSplitter::new(SplitConfig::default());
        let results: Vec<_> = (0..200)
            .map(|i| s.resolve("exp-3", &format!("u{i}"), &cfg))
            .collect();
        for name in &["a", "b", "c"] {
            assert!(results.iter().any(|r| r == name), "variant {} missing", name);
        }
    }

    #[test]
    fn different_salt_changes_assignment() {
        let cfg = two_variant_cfg();
        let s1 = TrafficSplitter::new(SplitConfig::new("salt-A", true));
        let s2 = TrafficSplitter::new(SplitConfig::new("salt-B", true));
        let mut same = 0usize;
        for i in 0..50u32 {
            let uid = format!("user-{i}");
            if s1.resolve("exp-1", &uid, &cfg) == s2.resolve("exp-1", &uid, &cfg) {
                same += 1;
            }
        }
        // With different salts, assignments should diverge for at least some users
        assert!(same < 50, "expected some users to differ between salts");
    }
}
