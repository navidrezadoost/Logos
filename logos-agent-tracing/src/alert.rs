//! Alerting — conditions, evaluator, and webhook notifier.

use thiserror::Error;

/// Errors from notifier operations.
#[derive(Debug, Error, PartialEq)]
pub enum NotifierError {
    #[error("webhook URL is empty")]
    EmptyUrl,
    #[error("alert name is empty")]
    EmptyAlertName,
    #[error("notification failed: {0}")]
    SendFailed(String),
}

/// Severity level of a fired alert.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl AlertSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            AlertSeverity::Info     => "INFO",
            AlertSeverity::Warning  => "WARNING",
            AlertSeverity::Critical => "CRITICAL",
        }
    }
}

/// Condition type that triggers an alert.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionKind {
    /// Fires when error_rate > threshold.
    ErrorRateExceeds(f64),
    /// Fires when p99_latency_ms > threshold.
    LatencyP99Exceeds(f64),
    /// Fires when avg_latency_ms > threshold.
    AvgLatencyExceeds(f64),
    /// Fires when span_count in window < threshold (low traffic).
    ThroughputBelow(f64),
    /// Fires when absolute value compared to threshold.
    CustomThreshold { metric: String, threshold: f64 },
}

/// A named alert condition with severity and a cooldown.
#[derive(Debug, Clone)]
pub struct AlertCondition {
    pub name:       String,
    pub kind:       ConditionKind,
    pub severity:   AlertSeverity,
    /// Minimum seconds between repeated firings (0 = always fire).
    pub cooldown_secs: u64,
}

impl AlertCondition {
    pub fn new(name: impl Into<String>, kind: ConditionKind, severity: AlertSeverity) -> Self {
        Self { name: name.into(), kind, severity, cooldown_secs: 0 }
    }

    pub fn with_cooldown(mut self, secs: u64) -> Self {
        self.cooldown_secs = secs;
        self
    }

    /// Convenience: error rate exceeds threshold (WARNING).
    pub fn error_rate_exceeds(threshold: f64) -> Self {
        Self::new(
            format!("error_rate > {:.0}%", threshold * 100.0),
            ConditionKind::ErrorRateExceeds(threshold),
            AlertSeverity::Warning,
        )
    }

    /// Convenience: p99 latency exceeds threshold in ms (CRITICAL).
    pub fn p99_latency_exceeds(threshold_ms: f64) -> Self {
        Self::new(
            format!("p99 > {threshold_ms}ms"),
            ConditionKind::LatencyP99Exceeds(threshold_ms),
            AlertSeverity::Critical,
        )
    }
}

/// A fired alert instance.
#[derive(Debug, Clone)]
pub struct AlertFired {
    pub condition_name: String,
    pub severity:       AlertSeverity,
    pub observed_value: f64,
    pub threshold:      f64,
    pub message:        String,
}

impl AlertFired {
    pub fn is_critical(&self) -> bool {
        self.severity == AlertSeverity::Critical
    }
}

/// Evaluates alert conditions against observed metric values.
#[derive(Debug, Default)]
pub struct AlertEvaluator {
    /// Last fire timestamp per condition name (ms since epoch, simulated).
    last_fired_ms: std::collections::HashMap<String, f64>,
}

impl AlertEvaluator {
    pub fn new() -> Self { Self::default() }

    /// Evaluate a condition against `observed`, respecting cooldown.
    /// `now_ms` is the current time for cooldown bookkeeping.
    pub fn evaluate_at(
        &mut self,
        condition: &AlertCondition,
        observed: f64,
        now_ms: f64,
    ) -> Option<AlertFired> {
        if !Self::condition_fires(&condition.kind, observed) {
            return None;
        }
        // Cooldown check
        if condition.cooldown_secs > 0 {
            if let Some(&last) = self.last_fired_ms.get(&condition.name) {
                let elapsed_secs = (now_ms - last) / 1000.0;
                if elapsed_secs < condition.cooldown_secs as f64 {
                    return None;
                }
            }
        }
        self.last_fired_ms.insert(condition.name.clone(), now_ms);

        let threshold = Self::threshold(&condition.kind);
        Some(AlertFired {
            condition_name: condition.name.clone(),
            severity:       condition.severity.clone(),
            observed_value: observed,
            threshold,
            message: format!(
                "[{}] {} — observed {:.4} exceeds threshold {:.4}",
                condition.severity.label(),
                condition.name,
                observed,
                threshold,
            ),
        })
    }

    /// Evaluate without cooldown tracking (stateless).
    pub fn evaluate(&self, condition: &AlertCondition, observed: f64) -> bool {
        Self::condition_fires(&condition.kind, observed)
    }

    fn condition_fires(kind: &ConditionKind, observed: f64) -> bool {
        match kind {
            ConditionKind::ErrorRateExceeds(t)     => observed > *t,
            ConditionKind::LatencyP99Exceeds(t)    => observed > *t,
            ConditionKind::AvgLatencyExceeds(t)    => observed > *t,
            ConditionKind::ThroughputBelow(t)      => observed < *t,
            ConditionKind::CustomThreshold { threshold, .. } => observed > *threshold,
        }
    }

    fn threshold(kind: &ConditionKind) -> f64 {
        match kind {
            ConditionKind::ErrorRateExceeds(t)     => *t,
            ConditionKind::LatencyP99Exceeds(t)    => *t,
            ConditionKind::AvgLatencyExceeds(t)    => *t,
            ConditionKind::ThroughputBelow(t)      => *t,
            ConditionKind::CustomThreshold { threshold, .. } => *threshold,
        }
    }

    /// Reset cooldown state.
    pub fn reset(&mut self) { self.last_fired_ms.clear(); }
}

/// Webhook-style notifier: collects fired alerts (simulated dispatch).
#[derive(Debug)]
pub struct WebhookNotifier {
    pub url: String,
    sent: Vec<AlertFired>,
}

impl WebhookNotifier {
    pub fn new(url: impl Into<String>) -> Result<Self, NotifierError> {
        let url = url.into();
        if url.is_empty() { return Err(NotifierError::EmptyUrl); }
        Ok(Self { url, sent: Vec::new() })
    }

    /// Simulate sending an alert (stores it internally).
    pub fn notify(&mut self, alert: AlertFired) -> Result<(), NotifierError> {
        if alert.condition_name.is_empty() {
            return Err(NotifierError::EmptyAlertName);
        }
        self.sent.push(alert);
        Ok(())
    }

    /// Number of alerts dispatched.
    pub fn sent_count(&self) -> usize { self.sent.len() }

    /// All sent alerts.
    pub fn sent_alerts(&self) -> &[AlertFired] { &self.sent }

    /// Critical alerts sent.
    pub fn critical_count(&self) -> usize {
        self.sent.iter().filter(|a| a.is_critical()).count()
    }

    pub fn reset(&mut self) { self.sent.clear(); }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_rate_condition_fires_above_threshold() {
        let cond = AlertCondition::error_rate_exceeds(0.1);
        let eval = AlertEvaluator::new();
        assert!(eval.evaluate(&cond, 0.2));
    }

    #[test]
    fn error_rate_condition_no_fire_at_threshold() {
        let cond = AlertCondition::error_rate_exceeds(0.1);
        let eval = AlertEvaluator::new();
        assert!(!eval.evaluate(&cond, 0.1));
    }

    #[test]
    fn p99_latency_fires_above_threshold() {
        let cond = AlertCondition::p99_latency_exceeds(200.0);
        let eval = AlertEvaluator::new();
        assert!(eval.evaluate(&cond, 250.0));
    }

    #[test]
    fn throughput_below_fires_under_threshold() {
        let cond = AlertCondition::new(
            "low-traffic", ConditionKind::ThroughputBelow(100.0), AlertSeverity::Warning,
        );
        let eval = AlertEvaluator::new();
        assert!(eval.evaluate(&cond, 50.0));
    }

    #[test]
    fn alert_fired_message_non_empty() {
        let cond = AlertCondition::error_rate_exceeds(0.05);
        let mut eval = AlertEvaluator::new();
        let fired = eval.evaluate_at(&cond, 0.2, 1000.0).unwrap();
        assert!(!fired.message.is_empty());
    }

    #[test]
    fn cooldown_suppresses_second_fire() {
        let cond = AlertCondition::error_rate_exceeds(0.05).with_cooldown(60);
        let mut eval = AlertEvaluator::new();
        eval.evaluate_at(&cond, 0.2, 1000.0).unwrap(); // fires
        let second = eval.evaluate_at(&cond, 0.2, 5000.0); // 4 s later — still in cooldown
        assert!(second.is_none());
    }

    #[test]
    fn cooldown_allows_fire_after_expiry() {
        let cond = AlertCondition::error_rate_exceeds(0.05).with_cooldown(60);
        let mut eval = AlertEvaluator::new();
        eval.evaluate_at(&cond, 0.2, 0.0).unwrap();
        // 61 seconds later
        let second = eval.evaluate_at(&cond, 0.2, 61_000.0);
        assert!(second.is_some());
    }

    #[test]
    fn severity_ordering() {
        assert!(AlertSeverity::Critical > AlertSeverity::Warning);
        assert!(AlertSeverity::Warning  > AlertSeverity::Info);
    }

    #[test]
    fn webhook_notifier_empty_url_errors() {
        assert!(matches!(WebhookNotifier::new(""), Err(NotifierError::EmptyUrl)));
    }

    #[test]
    fn webhook_notifier_sends_and_counts() {
        let mut n = WebhookNotifier::new("https://hooks.example.com/alert").unwrap();
        let cond = AlertCondition::p99_latency_exceeds(100.0);
        let mut eval = AlertEvaluator::new();
        let fired = eval.evaluate_at(&cond, 200.0, 0.0).unwrap();
        n.notify(fired).unwrap();
        assert_eq!(n.sent_count(), 1);
    }

    #[test]
    fn webhook_critical_count() {
        let mut n = WebhookNotifier::new("https://hooks.example.com/alert").unwrap();
        let cond = AlertCondition::p99_latency_exceeds(100.0);
        let mut eval = AlertEvaluator::new();
        let fired = eval.evaluate_at(&cond, 200.0, 0.0).unwrap();
        assert!(fired.is_critical());
        n.notify(fired).unwrap();
        assert_eq!(n.critical_count(), 1);
    }
}
