//! Retry, timeout, and backoff policies for workflow steps.

use thiserror::Error;

/// Errors from policy operations.
#[derive(Debug, Error, PartialEq)]
pub enum PolicyError {
    #[error("max_attempts must be >= 1 (got {0})")]
    InvalidMaxAttempts(u32),
    #[error("base_delay_ms must be > 0 (got {0})")]
    InvalidBaseDelay(u64),
    #[error("timeout_ms must be > 0 (got {0})")]
    InvalidTimeout(u64),
    #[error("max_delay_ms must be >= base_delay_ms")]
    MaxDelayTooSmall,
}

/// Strategy for calculating delays between retries.
#[derive(Debug, Clone, PartialEq)]
pub enum BackoffKind {
    /// Fixed delay between every retry.
    Fixed,
    /// Delay doubles on each attempt: base * 2^attempt.
    Exponential,
    /// Exponential + bounded random jitter in [0, base_delay_ms).
    ExponentialJitter,
    /// Linearly increasing: base * attempt.
    Linear,
}

impl BackoffKind {
    pub fn label(&self) -> &'static str {
        match self {
            BackoffKind::Fixed              => "FIXED",
            BackoffKind::Exponential        => "EXPONENTIAL",
            BackoffKind::ExponentialJitter  => "EXPONENTIAL_JITTER",
            BackoffKind::Linear             => "LINEAR",
        }
    }
}

/// Backoff delay calculator.
#[derive(Debug, Clone)]
pub struct BackoffPolicy {
    pub kind:          BackoffKind,
    pub base_delay_ms: u64,
    pub max_delay_ms:  u64,
}

impl BackoffPolicy {
    pub fn fixed(delay_ms: u64) -> Self {
        Self { kind: BackoffKind::Fixed, base_delay_ms: delay_ms, max_delay_ms: delay_ms }
    }

    pub fn exponential(base_ms: u64, max_ms: u64) -> Self {
        Self { kind: BackoffKind::Exponential, base_delay_ms: base_ms, max_delay_ms: max_ms }
    }

    pub fn exponential_jitter(base_ms: u64, max_ms: u64) -> Self {
        Self { kind: BackoffKind::ExponentialJitter, base_delay_ms: base_ms, max_delay_ms: max_ms }
    }

    /// Compute deterministic delay for `attempt` (0-indexed).
    /// Uses no RNG — jitter adds a fixed fraction for testability.
    pub fn delay_ms(&self, attempt: u32) -> u64 {
        let d = match self.kind {
            BackoffKind::Fixed => self.base_delay_ms,
            BackoffKind::Exponential => {
                self.base_delay_ms.saturating_mul(1u64 << attempt.min(30))
            }
            BackoffKind::ExponentialJitter => {
                let exp = self.base_delay_ms.saturating_mul(1u64 << attempt.min(30));
                // Deterministic jitter: add 25% of base
                exp + self.base_delay_ms / 4
            }
            BackoffKind::Linear => {
                self.base_delay_ms.saturating_mul(attempt as u64 + 1)
            }
        };
        d.min(self.max_delay_ms)
    }

    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.base_delay_ms == 0 { return Err(PolicyError::InvalidBaseDelay(0)); }
        if self.max_delay_ms < self.base_delay_ms { return Err(PolicyError::MaxDelayTooSmall); }
        Ok(())
    }
}

/// Retry policy for a workflow step.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Total number of attempts (1 = no retry).
    pub max_attempts: u32,
    pub backoff:      BackoffPolicy,
    /// Only retry on transient errors (if false, retry on all failures).
    pub transient_only: bool,
}

impl RetryPolicy {
    /// No retries — execute exactly once.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            backoff: BackoffPolicy::fixed(0),
            transient_only: false,
        }
    }

    /// Fixed interval retries.
    pub fn fixed(max_attempts: u32, delay_ms: u64) -> Self {
        Self {
            max_attempts,
            backoff: BackoffPolicy::fixed(delay_ms),
            transient_only: false,
        }
    }

    /// Exponential backoff retries.
    pub fn exponential(max_attempts: u32, base_ms: u64) -> Self {
        Self {
            max_attempts,
            backoff: BackoffPolicy::exponential(base_ms, base_ms * 32),
            transient_only: false,
        }
    }

    /// Whether another attempt is allowed after `attempt` failures (0-indexed).
    pub fn should_retry(&self, attempts_so_far: u32) -> bool {
        attempts_so_far < self.max_attempts.saturating_sub(1)
    }

    /// Delay before the next attempt.
    pub fn delay_before_attempt(&self, attempt: u32) -> u64 {
        if attempt == 0 { 0 } else { self.backoff.delay_ms(attempt - 1) }
    }

    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.max_attempts == 0 {
            return Err(PolicyError::InvalidMaxAttempts(0));
        }
        Ok(())
    }
}

impl Default for RetryPolicy {
    fn default() -> Self { Self::none() }
}

/// Timeout policy for a single step.
#[derive(Debug, Clone)]
pub struct TimeoutPolicy {
    /// Maximum wall-clock time in ms allowed for the step.
    pub timeout_ms:     u64,
    /// Action on timeout: abort or fall through to next step.
    pub fail_on_timeout: bool,
    /// Optional fallback agent id on timeout.
    pub fallback_agent: Option<String>,
}

impl TimeoutPolicy {
    pub fn new(timeout_ms: u64) -> Self {
        Self { timeout_ms, fail_on_timeout: true, fallback_agent: None }
    }

    pub fn lenient(timeout_ms: u64) -> Self {
        Self { timeout_ms, fail_on_timeout: false, fallback_agent: None }
    }

    pub fn with_fallback(mut self, agent_id: impl Into<String>) -> Self {
        self.fallback_agent = Some(agent_id.into());
        self
    }

    /// Returns true if `elapsed_ms` has exceeded the timeout.
    pub fn is_exceeded(&self, elapsed_ms: u64) -> bool {
        elapsed_ms > self.timeout_ms
    }

    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.timeout_ms == 0 {
            return Err(PolicyError::InvalidTimeout(0));
        }
        Ok(())
    }
}

impl Default for TimeoutPolicy {
    fn default() -> Self { Self::new(30_000) } // 30 s
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_none_no_retry() {
        let p = RetryPolicy::none();
        assert!(!p.should_retry(0));
    }

    #[test]
    fn retry_fixed_allows_retries() {
        let p = RetryPolicy::fixed(3, 100);
        assert!(p.should_retry(0));
        assert!(p.should_retry(1));
        assert!(!p.should_retry(2));
    }

    #[test]
    fn retry_delay_first_attempt_is_zero() {
        let p = RetryPolicy::exponential(3, 100);
        assert_eq!(p.delay_before_attempt(0), 0);
    }

    #[test]
    fn retry_exponential_delay_doubles() {
        let p = RetryPolicy::exponential(5, 100);
        let d1 = p.delay_before_attempt(1); // attempt 0 of backoff → 100
        let d2 = p.delay_before_attempt(2); // attempt 1 of backoff → 200
        assert_eq!(d2, d1 * 2);
    }

    #[test]
    fn retry_zero_attempts_invalid() {
        let p = RetryPolicy { max_attempts: 0, backoff: BackoffPolicy::fixed(10), transient_only: false };
        assert!(matches!(p.validate(), Err(PolicyError::InvalidMaxAttempts(0))));
    }

    #[test]
    fn backoff_fixed_constant() {
        let b = BackoffPolicy::fixed(50);
        assert_eq!(b.delay_ms(0), 50);
        assert_eq!(b.delay_ms(5), 50);
    }

    #[test]
    fn backoff_exponential_capped_at_max() {
        let b = BackoffPolicy::exponential(100, 500);
        assert!(b.delay_ms(10) <= 500);
    }

    #[test]
    fn backoff_linear_scales() {
        let b = BackoffPolicy { kind: BackoffKind::Linear, base_delay_ms: 50, max_delay_ms: 10_000 };
        assert_eq!(b.delay_ms(0), 50);
        assert_eq!(b.delay_ms(1), 100);
        assert_eq!(b.delay_ms(2), 150);
    }

    #[test]
    fn timeout_is_exceeded() {
        let t = TimeoutPolicy::new(1000);
        assert!(t.is_exceeded(1001));
        assert!(!t.is_exceeded(999));
    }

    #[test]
    fn timeout_zero_invalid() {
        assert!(matches!(TimeoutPolicy::new(0).validate(), Err(PolicyError::InvalidTimeout(0))));
    }

    #[test]
    fn timeout_with_fallback() {
        let t = TimeoutPolicy::new(500).with_fallback("backup-agent");
        assert_eq!(t.fallback_agent.as_deref(), Some("backup-agent"));
    }

    #[test]
    fn backoff_kind_labels() {
        assert_eq!(BackoffKind::Exponential.label(), "EXPONENTIAL");
        assert_eq!(BackoffKind::Fixed.label(),        "FIXED");
    }
}
