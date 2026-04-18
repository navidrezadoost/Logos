//! Automated plugin certification — sandbox scoring and badge assignment.
//!
//! A plugin goes through a sandbox run; `SandboxResult` captures the outcome.
//! `CertificationRepo::certify()` converts the result into a `CertificationScore`
//! which carries a `BadgeLevel` that is visible on the marketplace listing.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DbError;

// ── Badge levels ─────────────────────────────────────────────────────────────

/// Quality badge assigned to a certified plugin.
///
/// The level is derived from the numeric score:
/// - `None`   — not yet certified or score < 40
/// - `Junior` — 40–59
/// - `Mid`    — 60–79
/// - `Senior` — 80–94
/// - `Expert` — 95–100
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BadgeLevel {
    None,
    Junior,
    Mid,
    Senior,
    Expert,
}

impl BadgeLevel {
    /// Derive the badge level from a raw score in [0, 100].
    pub fn from_score(score: u32) -> Self {
        match score {
            95..=100 => BadgeLevel::Expert,
            80..=94  => BadgeLevel::Senior,
            60..=79  => BadgeLevel::Mid,
            40..=59  => BadgeLevel::Junior,
            _        => BadgeLevel::None,
        }
    }

    /// Human-readable label shown on the marketplace.
    pub fn label(&self) -> &'static str {
        match self {
            BadgeLevel::None   => "Unverified",
            BadgeLevel::Junior => "Junior",
            BadgeLevel::Mid    => "Mid",
            BadgeLevel::Senior => "Senior",
            BadgeLevel::Expert => "Expert",
        }
    }
}

impl std::fmt::Display for BadgeLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ── Sandbox result ────────────────────────────────────────────────────────────

/// Raw outcome produced by the certification sandbox runner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SandboxResult {
    /// Unit tests the plugin's own test suite ran.
    pub passed_tests: u32,
    /// Total unit tests discovered.
    pub total_tests: u32,
    /// Plugin did not call any disallowed host APIs.
    pub no_forbidden_apis: bool,
    /// All memory operations were provably safe (no unsafe blocks in WASM).
    pub memory_safe: bool,
    /// Plugin produced identical output for identical input in repeated runs.
    pub deterministic: bool,
    /// Optional free-text notes from the sandbox run.
    pub notes: String,
}

impl SandboxResult {
    /// Create a perfect result (useful for tests).
    pub fn perfect(total_tests: u32) -> Self {
        Self {
            passed_tests: total_tests,
            total_tests,
            no_forbidden_apis: true,
            memory_safe: true,
            deterministic: true,
            notes: String::new(),
        }
    }

    /// Fraction of tests that passed, in [0.0, 1.0].
    pub fn pass_rate(&self) -> f64 {
        if self.total_tests == 0 {
            return 0.0;
        }
        self.passed_tests as f64 / self.total_tests as f64
    }
}

// ── Scoring algorithm ─────────────────────────────────────────────────────────

/// Compute a [0, 100] score from a `SandboxResult`.
///
/// Breakdown:
/// - Up to 70 points from test-pass rate.
/// - 10 points for no forbidden APIs.
/// - 10 points for memory safety.
/// - 10 points for determinism.
pub fn compute_score(result: &SandboxResult) -> u32 {
    let test_points = (result.pass_rate() * 70.0).round() as u32;
    let api_points       = if result.no_forbidden_apis { 10 } else { 0 };
    let memory_points    = if result.memory_safe       { 10 } else { 0 };
    let determ_points    = if result.deterministic     { 10 } else { 0 };
    (test_points + api_points + memory_points + determ_points).min(100)
}

// ── Certification record ──────────────────────────────────────────────────────

/// Stored certification for a single plugin version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertificationScore {
    /// Plugin ID this certification applies to.
    pub plugin_id: Uuid,
    /// Raw numeric score in [0, 100].
    pub score: u32,
    /// Derived quality badge.
    pub badge: BadgeLevel,
    /// The sandbox run that produced this score.
    pub sandbox: SandboxResult,
    /// Unix-ms timestamp of certification.
    pub certified_at: u64,
}

impl CertificationScore {
    /// `true` if the plugin passed at minimum threshold (score ≥ 40).
    pub fn is_passing(&self) -> bool {
        self.badge != BadgeLevel::None
    }
}

// ── Certification repository ──────────────────────────────────────────────────

/// In-memory store of plugin certifications.
#[derive(Default, Debug)]
pub struct CertificationRepo {
    entries: std::collections::HashMap<Uuid, CertificationScore>,
}

impl CertificationRepo {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the scoring algorithm on `result` and persist the outcome.
    pub fn certify(
        &mut self,
        plugin_id: Uuid,
        sandbox: SandboxResult,
        certified_at: u64,
    ) -> &CertificationScore {
        let score = compute_score(&sandbox);
        let badge = BadgeLevel::from_score(score);
        let entry = CertificationScore { plugin_id, score, badge, sandbox, certified_at };
        self.entries.insert(plugin_id, entry);
        self.entries.get(&plugin_id).unwrap()
    }

    /// Retrieve the certification for a plugin, if it has been certified.
    pub fn get(&self, plugin_id: &Uuid) -> Option<&CertificationScore> {
        self.entries.get(plugin_id)
    }

    /// Convenience: return the badge level for a plugin, or `BadgeLevel::None`.
    pub fn badge_for(&self, plugin_id: &Uuid) -> BadgeLevel {
        self.entries
            .get(plugin_id)
            .map(|c| c.badge)
            .unwrap_or(BadgeLevel::None)
    }

    /// Revoke the certification for a plugin (e.g. after a security report).
    pub fn revoke(&mut self, plugin_id: &Uuid) -> Result<(), DbError> {
        self.entries
            .remove(plugin_id)
            .map(|_| ())
            .ok_or(DbError::NotFound(format!("certification for {plugin_id}")))
    }

    /// All certified plugins that hold at least `min_badge`.
    pub fn list_at_least(&self, min_badge: BadgeLevel) -> Vec<&CertificationScore> {
        let mut out: Vec<_> = self.entries.values()
            .filter(|c| c.badge >= min_badge)
            .collect();
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pid() -> Uuid { Uuid::new_v4() }

    // BadgeLevel::from_score ---------------------------------------------------

    #[test]
    fn cert001_badge_expert() {
        assert_eq!(BadgeLevel::from_score(100), BadgeLevel::Expert);
        assert_eq!(BadgeLevel::from_score(95),  BadgeLevel::Expert);
    }

    #[test]
    fn cert002_badge_senior() {
        assert_eq!(BadgeLevel::from_score(94), BadgeLevel::Senior);
        assert_eq!(BadgeLevel::from_score(80), BadgeLevel::Senior);
    }

    #[test]
    fn cert003_badge_mid() {
        assert_eq!(BadgeLevel::from_score(79), BadgeLevel::Mid);
        assert_eq!(BadgeLevel::from_score(60), BadgeLevel::Mid);
    }

    #[test]
    fn cert004_badge_junior() {
        assert_eq!(BadgeLevel::from_score(59), BadgeLevel::Junior);
        assert_eq!(BadgeLevel::from_score(40), BadgeLevel::Junior);
    }

    #[test]
    fn cert005_badge_none() {
        assert_eq!(BadgeLevel::from_score(39), BadgeLevel::None);
        assert_eq!(BadgeLevel::from_score(0),  BadgeLevel::None);
    }

    #[test]
    fn cert006_badge_ordering() {
        assert!(BadgeLevel::Expert > BadgeLevel::Senior);
        assert!(BadgeLevel::Senior > BadgeLevel::Mid);
        assert!(BadgeLevel::Mid > BadgeLevel::Junior);
        assert!(BadgeLevel::Junior > BadgeLevel::None);
    }

    #[test]
    fn cert007_badge_label() {
        assert_eq!(BadgeLevel::Expert.label(), "Expert");
        assert_eq!(BadgeLevel::None.label(), "Unverified");
    }

    // compute_score ------------------------------------------------------------

    #[test]
    fn cert008_perfect_score() {
        let r = SandboxResult::perfect(100);
        assert_eq!(compute_score(&r), 100);
    }

    #[test]
    fn cert009_score_no_tests_only_bonuses() {
        let r = SandboxResult {
            passed_tests: 0, total_tests: 0,
            no_forbidden_apis: true, memory_safe: true, deterministic: true,
            notes: String::new(),
        };
        // 0 test points + 30 bonus = 30
        assert_eq!(compute_score(&r), 30);
    }

    #[test]
    fn cert010_score_half_tests_all_bonuses() {
        let r = SandboxResult {
            passed_tests: 50, total_tests: 100,
            no_forbidden_apis: true, memory_safe: true, deterministic: true,
            notes: String::new(),
        };
        // 35 + 30 = 65
        assert_eq!(compute_score(&r), 65);
    }

    #[test]
    fn cert011_score_all_tests_no_bonuses() {
        let r = SandboxResult {
            passed_tests: 100, total_tests: 100,
            no_forbidden_apis: false, memory_safe: false, deterministic: false,
            notes: String::new(),
        };
        assert_eq!(compute_score(&r), 70);
    }

    // SandboxResult helpers ----------------------------------------------------

    #[test]
    fn cert012_pass_rate_perfect() {
        let r = SandboxResult::perfect(50);
        assert!((r.pass_rate() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cert013_pass_rate_zero_total() {
        let r = SandboxResult { total_tests: 0, passed_tests: 0, ..SandboxResult::perfect(0) };
        assert!((r.pass_rate() - 0.0).abs() < 1e-9);
    }

    // CertificationRepo --------------------------------------------------------

    #[test]
    fn cert014_repo_starts_empty() {
        let repo = CertificationRepo::new();
        assert!(repo.is_empty());
    }

    #[test]
    fn cert015_certify_stores_result() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        repo.certify(id, SandboxResult::perfect(10), 1000);
        assert_eq!(repo.len(), 1);
        assert!(repo.get(&id).is_some());
    }

    #[test]
    fn cert016_get_nonexistent_is_none() {
        let repo = CertificationRepo::new();
        assert!(repo.get(&pid()).is_none());
    }

    #[test]
    fn cert017_badge_for_certified_plugin() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        repo.certify(id, SandboxResult::perfect(100), 0);
        assert_eq!(repo.badge_for(&id), BadgeLevel::Expert);
    }

    #[test]
    fn cert018_badge_for_uncertified_is_none() {
        let repo = CertificationRepo::new();
        assert_eq!(repo.badge_for(&pid()), BadgeLevel::None);
    }

    #[test]
    fn cert019_is_passing_true() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        let score = repo.certify(id, SandboxResult::perfect(10), 0).clone();
        assert!(score.is_passing());
    }

    #[test]
    fn cert020_is_passing_false_when_score_low() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        let result = SandboxResult {
            passed_tests: 0, total_tests: 100,
            no_forbidden_apis: false, memory_safe: false, deterministic: false,
            notes: String::new(),
        };
        let score = repo.certify(id, result, 0).clone();
        assert!(!score.is_passing());
    }

    #[test]
    fn cert021_revoke_existing() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        repo.certify(id, SandboxResult::perfect(5), 0);
        assert!(repo.revoke(&id).is_ok());
        assert!(repo.get(&id).is_none());
    }

    #[test]
    fn cert022_revoke_nonexistent_is_error() {
        let mut repo = CertificationRepo::new();
        assert!(repo.revoke(&pid()).is_err());
    }

    #[test]
    fn cert023_list_at_least_senior() {
        let mut repo = CertificationRepo::new();
        let expert_id = pid();
        let mid_id    = pid();

        repo.certify(expert_id, SandboxResult::perfect(100), 0);
        repo.certify(mid_id, SandboxResult {
            passed_tests: 50, total_tests: 100,
            no_forbidden_apis: true, memory_safe: true, deterministic: false,
            notes: String::new(),
        }, 0);

        let seniors = repo.list_at_least(BadgeLevel::Senior);
        assert_eq!(seniors.len(), 1);
        assert_eq!(seniors[0].plugin_id, expert_id);
    }

    #[test]
    fn cert024_list_at_least_all_pass() {
        let mut repo = CertificationRepo::new();
        for _ in 0..4 {
            repo.certify(pid(), SandboxResult::perfect(10), 0);
        }
        let list = repo.list_at_least(BadgeLevel::None);
        assert_eq!(list.len(), 4);
    }

    #[test]
    fn cert025_list_sorted_by_score_desc() {
        let mut repo = CertificationRepo::new();
        let low_id  = pid();
        let high_id = pid();
        repo.certify(low_id, SandboxResult {
            passed_tests: 0, total_tests: 100,
            no_forbidden_apis: true, memory_safe: true, deterministic: true,
            notes: String::new(),
        }, 0); // score = 30
        repo.certify(high_id, SandboxResult::perfect(100), 0); // score = 100
        let list = repo.list_at_least(BadgeLevel::None);
        // highest score first
        assert_eq!(list[0].plugin_id, high_id);
    }

    #[test]
    fn cert026_re_certify_replaces_previous() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        repo.certify(id, SandboxResult {
            passed_tests: 0, total_tests: 10,
            no_forbidden_apis: false, memory_safe: false, deterministic: false,
            notes: String::new(),
        }, 0);
        repo.certify(id, SandboxResult::perfect(10), 1000);
        assert_eq!(repo.get(&id).unwrap().badge, BadgeLevel::Expert);
        assert_eq!(repo.len(), 1); // still one entry
    }

    #[test]
    fn cert027_certified_at_stored() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        repo.certify(id, SandboxResult::perfect(10), 42);
        assert_eq!(repo.get(&id).unwrap().certified_at, 42);
    }

    #[test]
    fn cert028_score_capped_at_100() {
        // Artificial case: pass_rate rounds up slightly; ensure capped.
        let r = SandboxResult {
            passed_tests: 100, total_tests: 100,
            no_forbidden_apis: true, memory_safe: true, deterministic: true,
            notes: String::new(),
        };
        assert!(compute_score(&r) <= 100);
    }

    #[test]
    fn cert029_notes_preserved() {
        let mut repo = CertificationRepo::new();
        let id = pid();
        let mut result = SandboxResult::perfect(5);
        result.notes = "all clear".to_string();
        repo.certify(id, result, 0);
        assert_eq!(repo.get(&id).unwrap().sandbox.notes, "all clear");
    }

    #[test]
    fn cert030_badge_display() {
        assert_eq!(format!("{}", BadgeLevel::Senior), "Senior");
        assert_eq!(format!("{}", BadgeLevel::None),   "Unverified");
    }
}
