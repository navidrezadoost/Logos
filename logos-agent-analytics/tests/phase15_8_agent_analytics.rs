//! Phase 15.8 — Agent Analytics Dashboard integration tests.
//!
//! 50 tests covering: metrics (10), aggregator (12), feedback (10),
//! dashboard (12), and end-to-end (6).

use logos_agent_analytics::{
    // metrics
    InvocationEvent, MetricsCollector, OutcomeKind, MetricsError,
    // aggregator
    Aggregator, AgentVersionStats, TimeWindow,
    // feedback
    FeedbackStore, UserFeedback, FeedbackSummary, FeedbackError,
    // dashboard
    Dashboard, DashboardAlert, AlertKind, VersionComparison,
};

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

fn success_evt(agent: &str, ver: &str, lat: u64, tok: u32) -> InvocationEvent {
    InvocationEvent::new(agent, ver, OutcomeKind::Success, lat, tok)
}
fn failure_evt(agent: &str, ver: &str, lat: u64, tok: u32) -> InvocationEvent {
    InvocationEvent::new(agent, ver, OutcomeKind::Failure, lat, tok)
}
fn timeout_evt(agent: &str, ver: &str) -> InvocationEvent {
    InvocationEvent::new(agent, ver, OutcomeKind::Timeout, 5000, 0)
}
fn fb(agent: &str, ver: &str, sess: &str, r: u8) -> UserFeedback {
    UserFeedback::new(agent, ver, sess, r)
}

fn simple_collector() -> MetricsCollector {
    let mut col = MetricsCollector::new();
    col.record(success_evt("agent-a", "1.0.0", 150, 400));
    col.record(success_evt("agent-a", "1.0.0", 250, 600));
    col.record(failure_evt("agent-a", "1.0.0", 300, 200));
    col
}

fn simple_store() -> FeedbackStore {
    let mut s = FeedbackStore::new();
    s.submit(fb("agent-a", "1.0.0", "s1", 5));
    s.submit(fb("agent-a", "1.0.0", "s2", 4));
    s
}

// ════════════════════════════════════════════════════════════════════════════
// §1 Metrics (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn metrics_empty_collector_has_zero_count() {
    assert_eq!(MetricsCollector::new().total_count(), 0);
}

#[test]
fn metrics_record_increments_count() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("a", "1.0.0", 100, 0));
    assert_eq!(col.total_count(), 1);
}

#[test]
fn metrics_record_many_increments_count() {
    let mut col = MetricsCollector::new();
    let evts: Vec<_> = (0..5).map(|_| success_evt("a", "1.0.0", 100, 0)).collect();
    col.record_many(evts);
    assert_eq!(col.total_count(), 5);
}

#[test]
fn metrics_events_for_version_filters() {
    let col = simple_collector();
    assert_eq!(col.events_for_version("agent-a", "1.0.0").len(), 3);
    assert_eq!(col.events_for_version("agent-a", "9.9.9").len(), 0);
}

#[test]
fn metrics_events_for_agent_returns_all_versions() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("x", "1.0.0", 100, 0));
    col.record(success_evt("x", "2.0.0", 100, 0));
    assert_eq!(col.events_for_agent("x").len(), 2);
}

#[test]
fn metrics_success_rate_mixed() {
    let col = simple_collector(); // 2 success, 1 failure
    let rate = col.success_rate("agent-a", "1.0.0");
    assert!((rate - 66.666_67).abs() < 0.01, "rate={rate}");
}

#[test]
fn metrics_avg_latency_ms_correct() {
    let col = simple_collector(); // 150, 250, 300 → avg=233.33
    assert!((col.avg_latency_ms("agent-a", "1.0.0") - 233.333).abs() < 0.1);
}

#[test]
fn metrics_peak_latency_ms() {
    let col = simple_collector();
    assert_eq!(col.peak_latency_ms("agent-a", "1.0.0"), 300);
}

#[test]
fn metrics_total_tokens() {
    let col = simple_collector(); // 400+600+200=1200
    assert_eq!(col.total_tokens("agent-a", "1.0.0"), 1200);
}

#[test]
fn metrics_p95_latency_monotone() {
    let mut col = MetricsCollector::new();
    for i in 1u64..=20 {
        col.record(success_evt("a", "1.0.0", i * 10, 0));
    }
    let p95 = col.p95_latency_ms("a", "1.0.0");
    // p95 of [10,20,...,200] ≈ 190
    assert!(p95 >= 180 && p95 <= 200, "p95={p95}");
}

// ════════════════════════════════════════════════════════════════════════════
// §2 Aggregator (12 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn agg_empty_collector_returns_empty_vec() {
    let col = MetricsCollector::new();
    assert!(Aggregator::compute(&col, &TimeWindow::All).is_empty());
}

#[test]
fn agg_single_version_stats_correct() {
    let col = simple_collector();
    let stats = Aggregator::compute(&col, &TimeWindow::All);
    assert_eq!(stats.len(), 1);
    let s = &stats[0];
    assert_eq!(s.call_count, 3);
    assert_eq!(s.success_count, 2);
    assert_eq!(s.failure_count, 1);
    assert_eq!(s.total_tokens, 1200);
}

#[test]
fn agg_multiple_versions_separate_rows() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("a", "1.0.0", 100, 0));
    col.record(success_evt("a", "2.0.0", 200, 0));
    let stats = Aggregator::compute(&col, &TimeWindow::All);
    assert_eq!(stats.len(), 2);
}

#[test]
fn agg_success_rate_100_percent() {
    let mut col = MetricsCollector::new();
    for _ in 0..5 { col.record(success_evt("a", "1.0.0", 100, 0)); }
    let s = &Aggregator::compute(&col, &TimeWindow::All)[0];
    assert!((s.success_rate() - 100.0).abs() < 1e-3);
}

#[test]
fn agg_time_window_last_n_reduces_count() {
    let mut col = MetricsCollector::new();
    for _ in 0..10 { col.record(success_evt("a", "1.0.0", 100, 0)); }
    let stats = Aggregator::compute(&col, &TimeWindow::LastN(4));
    assert_eq!(stats[0].call_count, 4);
}

#[test]
fn agg_time_window_since_ts() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("b", "1.0.0", 100, 0).with_ts(1000));
    col.record(success_evt("b", "1.0.0", 100, 0).with_ts(2000));
    col.record(success_evt("b", "1.0.0", 100, 0).with_ts(3000));
    let stats = Aggregator::compute(&col, &TimeWindow::SinceTs(2000));
    assert_eq!(stats[0].call_count, 2);
}

#[test]
fn agg_for_agent_narrows_to_one_agent() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("foo", "1.0.0", 100, 0));
    col.record(success_evt("bar", "1.0.0", 100, 0));
    let res = Aggregator::for_agent(&col, "foo", &TimeWindow::All);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].agent_id, "foo");
}

#[test]
fn agg_best_version_picks_highest_success() {
    let mut col = MetricsCollector::new();
    // v1: 50%
    col.record(success_evt("ag", "1.0.0", 100, 0));
    col.record(failure_evt("ag", "1.0.0", 100, 0));
    // v2: 100%
    col.record(success_evt("ag", "2.0.0", 100, 0));
    let best = Aggregator::best_version(&col, "ag", &TimeWindow::All).unwrap();
    assert_eq!(best.version, "2.0.0");
}

#[test]
fn agg_best_version_none_if_no_events() {
    let col = MetricsCollector::new();
    assert!(Aggregator::best_version(&col, "ghost", &TimeWindow::All).is_none());
}

#[test]
fn agg_is_better_than_true_when_higher_success() {
    let good = AgentVersionStats { success_count: 10, call_count: 10, ..Default::default() };
    let bad  = AgentVersionStats { success_count: 5,  call_count: 10, ..Default::default() };
    assert!(good.is_better_than(&bad));
}

#[test]
fn agg_avg_tokens_correct() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("a", "1.0.0", 0, 200));
    col.record(success_evt("a", "1.0.0", 0, 400));
    let s = &Aggregator::compute(&col, &TimeWindow::All)[0];
    assert!((s.avg_tokens - 300.0).abs() < 1e-6);
}

#[test]
fn agg_timeout_count_tracked() {
    let mut col = MetricsCollector::new();
    col.record(timeout_evt("a", "1.0.0"));
    col.record(success_evt("a", "1.0.0", 100, 0));
    let s = &Aggregator::compute(&col, &TimeWindow::All)[0];
    assert_eq!(s.timeout_count, 1);
}

// ════════════════════════════════════════════════════════════════════════════
// §3 Feedback (10 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn fb_empty_store_total_zero() {
    assert_eq!(FeedbackStore::new().total_count(), 0);
}

#[test]
fn fb_submit_increments_total() {
    let mut s = FeedbackStore::new();
    s.submit(fb("a", "1.0.0", "s1", 5));
    assert_eq!(s.total_count(), 1);
}

#[test]
fn fb_avg_rating_all_five() {
    let mut s = FeedbackStore::new();
    for i in 0..3 { s.submit(fb("a", "1.0.0", &format!("s{i}"), 5)); }
    assert!((s.avg_rating("a", "1.0.0").unwrap() - 5.0).abs() < 1e-3);
}

#[test]
fn fb_avg_rating_none_when_no_feedback() {
    assert!(FeedbackStore::new().avg_rating("ghost", "1.0.0").is_none());
}

#[test]
fn fb_summary_distribution_correct() {
    let mut s = FeedbackStore::new();
    s.submit(fb("a", "1.0.0", "s1", 1));
    s.submit(fb("a", "1.0.0", "s2", 3));
    s.submit(fb("a", "1.0.0", "s3", 5));
    let sum = s.summary_for("a", "1.0.0");
    assert_eq!(sum.rating_distribution[0], 1); // 1-star
    assert_eq!(sum.rating_distribution[2], 1); // 3-star
    assert_eq!(sum.rating_distribution[4], 1); // 5-star
}

#[test]
fn fb_positive_pct_all_five_star() {
    let mut s = FeedbackStore::new();
    for i in 0..4 { s.submit(fb("a", "1.0.0", &format!("s{i}"), 5)); }
    let sum = s.summary_for("a", "1.0.0");
    assert!((sum.positive_pct - 100.0).abs() < 1e-3);
}

#[test]
fn fb_submit_checked_valid_rating_ok() {
    let mut s = FeedbackStore::new();
    assert!(s.submit_checked(fb("a", "1.0.0", "s1", 4)).is_ok());
}

#[test]
fn fb_submit_checked_invalid_rating_err() {
    let mut s = FeedbackStore::new();
    assert_eq!(s.submit_checked(fb("a", "1.0.0", "s1", 0)), Err(FeedbackError::InvalidRating(0)));
    assert_eq!(s.submit_checked(fb("a", "1.0.0", "s2", 6)), Err(FeedbackError::InvalidRating(6)));
}

#[test]
fn fb_feedbacks_for_filter_works() {
    let s = simple_store(); // 2 feedbacks for agent-a/1.0.0
    assert_eq!(s.feedbacks_for("agent-a", "1.0.0").len(), 2);
    assert_eq!(s.feedbacks_for("agent-a", "9.9.9").len(), 0);
}

#[test]
fn fb_covered_versions_deduplicated() {
    let mut s = FeedbackStore::new();
    s.submit(fb("a", "1.0.0", "s1", 5));
    s.submit(fb("a", "1.0.0", "s2", 4));
    s.submit(fb("b", "1.0.0", "s3", 3));
    assert_eq!(s.covered_versions().len(), 2);
}

// ════════════════════════════════════════════════════════════════════════════
// §4 Dashboard (12 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn dash_no_data_summary_text() {
    let dash = Dashboard::build(&MetricsCollector::new(), &FeedbackStore::new());
    assert_eq!(dash.summary_text(), "Dashboard: no data");
}

#[test]
fn dash_summary_text_contains_agent_id() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    col.record(success_evt("visible-agent", "1.0.0", 100, 0));
    s.submit(fb("visible-agent", "1.0.0", "s1", 5));
    let dash = Dashboard::build(&col, &s);
    assert!(dash.summary_text().contains("visible-agent"));
}

#[test]
fn dash_no_alert_for_healthy_agent() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..5 { col.record(success_evt("healthy", "1.0.0", 50, 0)); }
    s.submit(fb("healthy", "1.0.0", "s1", 5));
    let dash = Dashboard::build(&col, &s);
    let labels: Vec<_> = dash.alerts().iter().map(|a| a.kind.label()).collect();
    assert!(!labels.contains(&"low-success-rate"), "unexpected success-rate alert");
    assert!(!labels.contains(&"high-latency"), "unexpected latency alert");
    assert!(!labels.contains(&"low-rating"), "unexpected rating alert");
}

#[test]
fn dash_low_success_rate_alert_triggered() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    col.record(failure_evt("fail-agent", "1.0.0", 100, 0));
    s.submit(fb("fail-agent", "1.0.0", "s1", 4));
    let dash = Dashboard::build(&col, &s);
    assert!(dash.alerts().iter().any(|a| a.kind.label() == "low-success-rate"));
}

#[test]
fn dash_high_latency_alert_triggered() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    col.record(success_evt("slow", "1.0.0", 999_999, 0));
    s.submit(fb("slow", "1.0.0", "s1", 4));
    let dash = Dashboard::build(&col, &s);
    assert!(dash.alerts().iter().any(|a| a.kind.label() == "high-latency"));
}

#[test]
fn dash_no_feedback_alert_triggered() {
    let mut col = MetricsCollector::new();
    col.record(success_evt("agent-no-fb", "1.0.0", 100, 0));
    let dash = Dashboard::build(&col, &FeedbackStore::new());
    assert!(dash.alerts().iter().any(|a| a.kind.label() == "no-feedback"));
}

#[test]
fn dash_low_rating_alert_triggered() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..3 { col.record(success_evt("agent-lr", "1.0.0", 50, 0)); }
    s.submit(fb("agent-lr", "1.0.0", "s1", 1));
    s.submit(fb("agent-lr", "1.0.0", "s2", 1));
    let dash = Dashboard::build(&col, &s);
    assert!(dash.alerts().iter().any(|a| a.kind.label() == "low-rating"));
}

#[test]
fn dash_top_agents_ordered_by_success_rate() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..5 { col.record(success_evt("best",  "1.0.0", 100, 0)); }
    col.record(success_evt("worst", "1.0.0", 100, 0));
    col.record(failure_evt("worst", "1.0.0", 100, 0));
    for a in ["best", "worst"] { s.submit(fb(a, "1.0.0", "s", 5)); }
    let dash = Dashboard::build(&col, &s);
    let top = dash.top_agents(2);
    assert_eq!(top[0].agent_id, "best");
}

#[test]
fn dash_compare_versions_improvement() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    col.record(success_evt("ag", "1.0.0", 100, 0));
    col.record(failure_evt("ag", "1.0.0", 100, 0)); // 50 %
    col.record(success_evt("ag", "2.0.0", 100, 0)); // 100 %
    for v in ["1.0.0", "2.0.0"] { s.submit(fb("ag", v, "s", 4)); }
    let dash = Dashboard::build(&col, &s);
    let cmp  = dash.compare_versions("ag", "1.0.0", "2.0.0").unwrap();
    assert!(cmp.is_improvement());
    assert!(cmp.success_rate_delta() > 0.0);
}

#[test]
fn dash_compare_versions_missing_returns_none() {
    let dash = Dashboard::build(&MetricsCollector::new(), &FeedbackStore::new());
    assert!(dash.compare_versions("ghost", "1.0.0", "2.0.0").is_none());
}

#[test]
fn dash_has_alerts_false_when_healthy() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..5 { col.record(success_evt("ok", "1.0.0", 50, 0)); }
    s.submit(fb("ok", "1.0.0", "s1", 5));
    let dash = Dashboard::build(&col, &s);
    // only the non-alert path — success rate is 100 %, latency is 50 ms, rating 5
    let has_bad = dash.alerts().iter().any(|a| {
        matches!(a.kind, AlertKind::LowSuccessRate { .. } | AlertKind::HighLatency { .. } | AlertKind::LowRating { .. })
    });
    assert!(!has_bad);
}

#[test]
fn dash_to_json_valid_and_contains_stats() {
    let col = simple_collector();
    let s   = simple_store();
    let dash = Dashboard::build(&col, &s);
    let json = dash.to_json().unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["stats"].is_array());
}

// ════════════════════════════════════════════════════════════════════════════
// §5 End-to-end (6 tests)
// ════════════════════════════════════════════════════════════════════════════

#[test]
fn e2e_full_pipeline_smoke_test() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for i in 0u64..8 {
        col.record(success_evt("e2e-agent", "1.0.0", 100 + i * 10, 200));
    }
    col.record(failure_evt("e2e-agent", "1.0.0", 300, 100));
    s.submit(fb("e2e-agent", "1.0.0", "sess-a", 5));
    s.submit(fb("e2e-agent", "1.0.0", "sess-b", 4));
    let dash = Dashboard::build(&col, &s);
    let text = dash.summary_text();
    assert!(text.contains("e2e-agent"));
    assert!(!dash.alerts().iter().any(|a| a.kind.label() == "no-feedback"));
}

#[test]
fn e2e_version_rollback_scenario() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    // v1: good
    for _ in 0..10 { col.record(success_evt("svc", "1.0.0", 80, 100)); }
    // v2: bad (regression introduced)
    for _ in 0.. 3 { col.record(success_evt("svc", "2.0.0", 200, 150)); }
    for _ in 0.. 7 { col.record(failure_evt("svc", "2.0.0", 200, 150)); }
    for v in ["1.0.0", "2.0.0"] { s.submit(fb("svc", v, "s", 4)); }
    let dash = Dashboard::build(&col, &s);
    let cmp  = dash.compare_versions("svc", "1.0.0", "2.0.0").unwrap();
    assert!(!cmp.is_improvement(), "v2 should be a regression");
    assert!(cmp.success_rate_delta() < 0.0);
}

#[test]
fn e2e_multi_agent_dashboard() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    let agents = ["ag-1", "ag-2", "ag-3"];
    for agent in agents {
        for _ in 0..5 { col.record(success_evt(agent, "1.0.0", 100, 50)); }
        s.submit(fb(agent, "1.0.0", "s", 5));
    }
    let dash = Dashboard::build(&col, &s);
    assert_eq!(dash.all_stats().len(), 3);
    assert_eq!(dash.top_agents(3).len(), 3);
}

#[test]
fn e2e_no_alerts_all_green() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..20 { col.record(success_evt("green", "1.0.0", 50, 10)); }
    for i in 0..4 { s.submit(fb("green", "1.0.0", &format!("s{i}"), 5)); }
    let dash = Dashboard::build(&col, &s);
    let bad_alerts: Vec<_> = dash.alerts().iter()
        .filter(|a| matches!(a.kind, AlertKind::LowSuccessRate { .. } | AlertKind::HighLatency { .. } | AlertKind::LowRating { .. }))
        .collect();
    assert!(bad_alerts.is_empty());
}

#[test]
fn e2e_aggregator_best_version_wins() {
    let mut col = MetricsCollector::new();
    for _ in 0..3 { col.record(success_evt("svc", "1.0.0", 100, 0)); }
    for _ in 0..3 { col.record(failure_evt("svc", "1.0.0", 100, 0)); }
    for _ in 0..9 { col.record(success_evt("svc", "2.0.0", 100, 0)); }
    col.record(failure_evt("svc", "2.0.0", 100, 0));
    let best = Aggregator::best_version(&col, "svc", &TimeWindow::All).unwrap();
    assert_eq!(best.version, "2.0.0");
}

#[test]
fn e2e_feedback_drives_low_rating_alert() {
    let mut col = MetricsCollector::new();
    let mut s   = FeedbackStore::new();
    for _ in 0..5 { col.record(success_evt("angry-users", "1.0.0", 50, 0)); }
    // all 1-star reviews → avg rating 1.0
    for i in 0..5 { s.submit(fb("angry-users", "1.0.0", &format!("u{i}"), 1)); }
    let dash = Dashboard::build(&col, &s);
    assert!(dash.alerts().iter().any(|a| a.kind.label() == "low-rating"));
}
