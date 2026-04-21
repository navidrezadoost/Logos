// logos-collab/src/stress/report.rs
//
//! Human-readable and machine-readable stress-test report generation.
//!
//! A [`Report`] is produced from [`StressMetrics`] plus the configuration
//! thresholds that define pass/fail criteria.

use super::metrics::StressMetrics;

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Pass/fail thresholds applied against [`StressMetrics`].
#[derive(Debug, Clone)]
pub struct Thresholds {
    /// Minimum throughput in ops/sec that the run must achieve.
    pub min_ops_per_sec: f64,
    /// Maximum acceptable p99 latency in microseconds.
    pub max_p99_us: u64,
    /// Maximum acceptable error rate (0.0 – 1.0).
    pub max_error_rate: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_ops_per_sec: 1_000.0,
            max_p99_us:      100_000, // 100 ms
            max_error_rate:  0.0,
        }
    }
}

impl Thresholds {
    /// Conservative thresholds for unit-test scenarios (fewer users, no actual
    /// network).
    pub fn relaxed() -> Self {
        Self {
            min_ops_per_sec: 100.0,
            max_p99_us:      500_000, // 500 ms — tasks are not rate-limited
            max_error_rate:  0.0,
        }
    }
}

// ── Verdict ───────────────────────────────────────────────────────────────────

/// The outcome of a single threshold check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail(String),
}

impl Verdict {
    pub fn is_pass(&self) -> bool { matches!(self, Verdict::Pass) }
}

// ── Report ────────────────────────────────────────────────────────────────────

/// The complete stress-test report.
#[derive(Debug, Clone)]
pub struct Report {
    pub total_ops:    u64,
    pub error_count:  u64,
    pub error_rate:   f64,
    pub ops_per_sec:  f64,
    pub p50_us:       Option<u64>,
    pub p95_us:       Option<u64>,
    pub p99_us:       Option<u64>,
    pub max_us:       Option<u64>,
    pub thresholds:   Thresholds,
    pub verdicts:     Vec<Verdict>,
}

impl Report {
    /// Build a [`Report`] from metrics and thresholds.
    pub fn build(metrics: &StressMetrics, thresholds: Thresholds) -> Self {
        let ops_per_sec = metrics.throughput.ops_per_sec();
        let error_rate  = metrics.error_rate();
        let p99_us      = metrics.latency.p99();

        let mut verdicts = Vec::new();

        // Throughput check
        if ops_per_sec < thresholds.min_ops_per_sec {
            verdicts.push(Verdict::Fail(format!(
                "throughput {ops_per_sec:.0} ops/s < {min:.0} ops/s required",
                min = thresholds.min_ops_per_sec
            )));
        } else {
            verdicts.push(Verdict::Pass);
        }

        // p99 latency check
        if let Some(p99) = p99_us {
            if p99 > thresholds.max_p99_us {
                verdicts.push(Verdict::Fail(format!(
                    "p99 latency {p99}µs > {}µs allowed",
                    thresholds.max_p99_us
                )));
            } else {
                verdicts.push(Verdict::Pass);
            }
        }

        // Error-rate check
        if error_rate > thresholds.max_error_rate {
            verdicts.push(Verdict::Fail(format!(
                "error rate {:.2}% > {:.2}% allowed",
                error_rate * 100.0,
                thresholds.max_error_rate * 100.0
            )));
        } else {
            verdicts.push(Verdict::Pass);
        }

        Self {
            total_ops:   metrics.total_ops,
            error_count: metrics.error_count,
            error_rate,
            ops_per_sec,
            p50_us:      metrics.latency.p50(),
            p95_us:      metrics.latency.p95(),
            p99_us,
            max_us:      metrics.latency.max(),
            thresholds,
            verdicts,
        }
    }

    /// `true` if every verdict is [`Verdict::Pass`].
    pub fn passed(&self) -> bool {
        self.verdicts.iter().all(|v| v.is_pass())
    }

    /// Number of failed verdicts.
    pub fn failure_count(&self) -> usize {
        self.verdicts.iter().filter(|v| !v.is_pass()).count()
    }

    /// Render a human-readable summary table (plain text).
    pub fn render_text(&self) -> String {
        let pass_fail = if self.passed() { "PASS ✓" } else { "FAIL ✗" };
        let mut out = format!(
            "┌─────────────────────────── Stress Report ─────────────────────────────┐\n\
             │  Result      : {pass_fail:<55}│\n\
             │  Total ops   : {:<55}│\n\
             │  Errors      : {:<55}│\n\
             │  Error rate  : {:<55}│\n\
             │  Throughput  : {:<55}│\n",
            self.total_ops,
            self.error_count,
            format!("{:.2}%", self.error_rate * 100.0),
            format!("{:.0} ops/s", self.ops_per_sec),
        );

        let fmt_us = |v: Option<u64>| v.map_or("—".into(), |u| format!("{u} µs"));
        out += &format!(
            "│  p50 latency : {:<55}│\n\
             │  p95 latency : {:<55}│\n\
             │  p99 latency : {:<55}│\n\
             │  max latency : {:<55}│\n",
            fmt_us(self.p50_us),
            fmt_us(self.p95_us),
            fmt_us(self.p99_us),
            fmt_us(self.max_us),
        );

        for (i, v) in self.verdicts.iter().enumerate() {
            let line = match v {
                Verdict::Pass       => format!("  Check #{:<3}: PASS", i + 1),
                Verdict::Fail(msg) => format!("  Check #{:<3}: FAIL — {msg}", i + 1),
            };
            out += &format!("│{line:<71}│\n");
        }
        out += "└───────────────────────────────────────────────────────────────────────┘\n";
        out
    }

    /// Render a compact JSON record (single-line, no pretty-print).
    pub fn render_json(&self) -> String {
        let verdicts: Vec<String> = self.verdicts.iter().map(|v| match v {
            Verdict::Pass      => "pass".into(),
            Verdict::Fail(msg) => format!("fail:{msg}"),
        }).collect();

        format!(
            r#"{{"total_ops":{total},"errors":{errors},"error_rate":{er:.4},"ops_per_sec":{ops:.2},"p50_us":{p50},"p95_us":{p95},"p99_us":{p99},"max_us":{max},"passed":{passed},"verdicts":{verdicts_json}}}"#,
            total   = self.total_ops,
            errors  = self.error_count,
            er      = self.error_rate,
            ops     = self.ops_per_sec,
            p50     = self.p50_us.unwrap_or(0),
            p95     = self.p95_us.unwrap_or(0),
            p99     = self.p99_us.unwrap_or(0),
            max     = self.max_us.unwrap_or(0),
            passed  = self.passed(),
            verdicts_json = format!("[{}]", verdicts.iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(",")),
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stress::metrics::StressMetrics;

    fn passing_metrics() -> StressMetrics {
        let mut m = StressMetrics::new(0);
        for i in 0..1_000u64 {
            m.record_ok(50 + i % 50, i); // ~1 000 ops in ~1 000 ms
        }
        m
    }

    // R-01: Report::build produces a passing report for good metrics.
    #[test]
    fn r_01_passing_report() {
        let m = passing_metrics();
        let r = Report::build(&m, Thresholds::relaxed());
        assert!(r.passed(), "Expected PASS, failures: {:?}", r.verdicts);
    }

    // R-02: Zero error count yields error_rate() == 0.0.
    #[test]
    fn r_02_zero_errors() {
        let m = passing_metrics();
        let r = Report::build(&m, Thresholds::relaxed());
        assert!((r.error_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(r.error_count, 0);
    }

    // R-03: Any error causes error-rate check to fail.
    #[test]
    fn r_03_any_error_fails() {
        let mut m = passing_metrics();
        m.record_error();
        let r = Report::build(&m, Thresholds::relaxed());
        assert!(!r.passed(), "One error should fail the report");
        assert!(r.failure_count() >= 1);
    }

    // R-04: render_text contains PASS when all checks pass.
    #[test]
    fn r_04_render_text_pass() {
        let m = passing_metrics();
        let r = Report::build(&m, Thresholds::relaxed());
        let text = r.render_text();
        assert!(text.contains("PASS"), "render_text should contain PASS");
    }

    // R-05: render_json is valid-looking JSON containing "passed".
    #[test]
    fn r_05_render_json_contains_passed() {
        let m = passing_metrics();
        let r = Report::build(&m, Thresholds::relaxed());
        let json = r.render_json();
        assert!(json.contains("\"passed\":true"), "JSON should say passed:true\n{json}");
        assert!(json.starts_with('{') && json.ends_with('}'));
    }

    // R-06: Verdict::Pass is_pass() returns true; Fail returns false.
    #[test]
    fn r_06_verdict_helpers() {
        assert!(Verdict::Pass.is_pass());
        assert!(!Verdict::Fail("oops".into()).is_pass());
    }

    // R-07: Thresholds::default targets 1000 ops/s and 0% errors.
    #[test]
    fn r_07_default_thresholds() {
        let t = Thresholds::default();
        assert!((t.min_ops_per_sec - 1_000.0).abs() < f64::EPSILON);
        assert!((t.max_error_rate - 0.0).abs() < f64::EPSILON);
    }

    // R-08: A slow run (below min_ops_per_sec) fails the throughput check.
    #[test]
    fn r_08_slow_throughput_fails() {
        let mut m = StressMetrics::new(0);
        // 10 ops in 10 000 ms → 1 ops/s (below 100 relaxed threshold)
        for i in 0..10u64 {
            m.record_ok(500, i * 1_000);
        }
        let r = Report::build(&m, Thresholds::relaxed());
        assert!(!r.passed(), "Should fail due to low throughput");
    }
}
