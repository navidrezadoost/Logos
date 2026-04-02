/// Phase 15.12 — Agent Observability & Tracing (logos-agent-tracing)
/// Integration test suite: 60 tests total
///
/// §1  Span & Tracer            (13 tests)
/// §2  Span Collector           (12 tests)
/// §3  Latency Histogram        (13 tests)
/// §4  Alerting                 (12 tests)
/// §5  End-to-end               (10 tests)

use logos_agent_tracing::{
    Tracer, SpanKind, SpanContext, SpanId, TraceId, SpanStatus, SpanEvent, TracerError,
    SpanCollector, TraceQuery, CollectorError,
    LatencyHistogram, ErrorRateTracker, BucketBounds,
    AlertCondition, AlertEvaluator, AlertSeverity, WebhookNotifier, NotifierError,
    ConditionKind,
};

// ── §1  Span & Tracer ─────────────────────────────────────────────────────────

#[test]
fn span_tracer_start_creates_span() {
    let mut t = Tracer::new("agent-svc");
    t.start_span("invoke", SpanKind::Internal);
    assert_eq!(t.span_count(), 1);
}

#[test]
fn span_tracer_finish_marks_span_done() {
    let mut t = Tracer::new_at("svc", 0.0);
    let id = t.start_span("op", SpanKind::Client).span_id.clone();
    t.finish_span_at(id, 100.0, true).unwrap();
    assert_eq!(t.open_span_count(), 0);
}

#[test]
fn span_duration_calculated_correctly() {
    let mut t = Tracer::new_at("svc", 50.0);
    let id = t.start_span("x", SpanKind::Internal).span_id.clone();
    t.finish_span_at(id, 150.0, true).unwrap();
    assert!((t.spans[0].duration_ms().unwrap() - 100.0).abs() < 1e-9);
}

#[test]
fn span_status_ok_when_success() {
    let mut t = Tracer::new("svc");
    let id = t.start_span("op", SpanKind::Internal).span_id.clone();
    t.finish_span(id, true).unwrap();
    assert_eq!(t.spans[0].status, SpanStatus::Ok);
}

#[test]
fn span_status_error_when_failure() {
    let mut t = Tracer::new("svc");
    let id = t.start_span("op", SpanKind::Internal).span_id.clone();
    t.finish_span(id, false).unwrap();
    assert!(t.spans[0].status.is_error());
}

#[test]
fn span_finish_unknown_errors() {
    let mut t = Tracer::new("svc");
    assert!(matches!(
        t.finish_span(SpanId::new("nope"), true),
        Err(TracerError::SpanNotFound(_))
    ));
}

#[test]
fn span_double_finish_errors() {
    let mut t = Tracer::new("svc");
    let id = t.start_span("op", SpanKind::Internal).span_id.clone();
    t.finish_span(id.clone(), true).unwrap();
    assert!(matches!(
        t.finish_span(id, true),
        Err(TracerError::AlreadyFinished(_))
    ));
}

#[test]
fn span_drain_returns_finished_only() {
    let mut t = Tracer::new("svc");
    let id = t.start_span("a", SpanKind::Internal).span_id.clone();
    t.start_span("b", SpanKind::Internal);
    t.finish_span(id, true).unwrap();
    let drained = t.drain();
    assert_eq!(drained.len(), 1);
    assert_eq!(t.open_span_count(), 1);
}

#[test]
fn span_child_has_correct_parent() {
    let mut t = Tracer::new("svc");
    let parent_id = t.start_span("parent", SpanKind::Server).span_id.clone();
    let trace_id  = t.spans[0].context.trace_id.clone();
    let child     = t.start_child_span("child", SpanKind::Internal, parent_id.clone(), trace_id);
    assert_eq!(child.context.parent_span_id.as_ref(), Some(&parent_id));
}

#[test]
fn span_context_root_has_no_parent() {
    let ctx = SpanContext::new_root(TraceId::new("t"), SpanId::new("s"));
    assert!(ctx.is_root());
}

#[test]
fn span_event_attached() {
    let mut t = Tracer::new("svc");
    let id = t.start_span("op", SpanKind::Internal).span_id.clone();
    t.spans[0].add_event(SpanEvent::new("cache.miss", 5.0));
    t.finish_span(id, true).unwrap();
    assert_eq!(t.spans[0].events.len(), 1);
}

#[test]
fn span_attribute_stored() {
    let mut t = Tracer::new("svc");
    t.start_span("op", SpanKind::Internal);
    t.spans[0].set_attribute("agent.id", "agent-42");
    assert_eq!(t.spans[0].attributes[0].1, "agent-42");
}

#[test]
fn span_kind_labels_correct() {
    assert_eq!(SpanKind::Server.label(),   "SERVER");
    assert_eq!(SpanKind::Consumer.label(), "CONSUMER");
    assert_eq!(SpanKind::Producer.label(), "PRODUCER");
}

// ── §2  Span Collector ────────────────────────────────────────────────────────

fn make_span(svc: &str, op: &str, start: f64, end: f64, ok: bool) -> logos_agent_tracing::Span {
    let mut t = Tracer::new_at(svc, start);
    let id = t.start_span(op, SpanKind::Internal).span_id.clone();
    t.finish_span_at(id, end, ok).unwrap();
    t.drain().remove(0)
}

#[test]
fn collector_records_span() {
    let mut c = SpanCollector::new();
    c.record(make_span("svc", "op", 0.0, 10.0, true));
    assert_eq!(c.span_count(), 1);
}

#[test]
fn collector_trace_lookup_ok() {
    let mut c = SpanCollector::new();
    let s = make_span("svc", "op", 0.0, 10.0, true);
    let tid = s.context.trace_id.0.clone();
    c.record(s);
    assert!(c.trace(&tid).is_ok());
}

#[test]
fn collector_trace_lookup_missing() {
    let c = SpanCollector::new();
    assert!(matches!(c.trace("x"), Err(CollectorError::TraceNotFound(_))));
}

#[test]
fn collector_query_by_service() {
    let mut c = SpanCollector::new();
    c.record(make_span("svc-a", "op", 0.0, 10.0, true));
    c.record(make_span("svc-b", "op", 0.0, 10.0, true));
    let results = c.query(&TraceQuery::new().service("svc-a"));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].service_name, "svc-a");
}

#[test]
fn collector_query_by_min_duration() {
    let mut c = SpanCollector::new();
    c.record(make_span("svc", "fast", 0.0, 5.0,   true));
    c.record(make_span("svc", "slow", 0.0, 500.0, true));
    let results = c.query(&TraceQuery::new().min_duration(100.0));
    assert_eq!(results.len(), 1);
}

#[test]
fn collector_query_errors_only() {
    let mut c = SpanCollector::new();
    c.record(make_span("svc", "ok",  0.0, 10.0, true));
    c.record(make_span("svc", "err", 0.0, 10.0, false));
    let results = c.query(&TraceQuery::new().errors_only());
    assert_eq!(results.len(), 1);
    assert!(results[0].status.is_error());
}

#[test]
fn collector_query_by_operation() {
    let mut c = SpanCollector::new();
    c.record(make_span("svc", "invoke",   0.0, 10.0, true));
    c.record(make_span("svc", "callback", 0.0, 10.0, true));
    let results = c.query(&TraceQuery::new().operation("invoke"));
    assert_eq!(results.len(), 1);
}

#[test]
fn collector_global_error_rate() {
    let mut c = SpanCollector::new();
    for _ in 0..8 { c.record(make_span("s", "ok",  0.0, 1.0, true)); }
    for _ in 0..2 { c.record(make_span("s", "err", 0.0, 1.0, false)); }
    assert!((c.global_error_rate() - 0.2).abs() < 1e-9);
}

#[test]
fn collector_avg_duration() {
    let mut c = SpanCollector::new();
    c.record(make_span("s", "a", 0.0, 100.0, true));
    c.record(make_span("s", "b", 0.0, 300.0, true));
    assert!((c.avg_duration_ms() - 200.0).abs() < 1e-9);
}

#[test]
fn collector_slowest_trace() {
    let mut c = SpanCollector::new();
    c.record(make_span("s", "fast", 0.0,  10.0, true));
    c.record(make_span("s", "slow", 0.0, 999.0, true));
    let slowest = c.slowest_trace().unwrap();
    assert_eq!(slowest.spans[0].name, "slow");
}

#[test]
fn collector_reset_clears() {
    let mut c = SpanCollector::new();
    c.record(make_span("s", "op", 0.0, 1.0, true));
    c.reset();
    assert_eq!(c.span_count(), 0);
}

#[test]
fn collector_trace_count() {
    let mut c = SpanCollector::new();
    c.record(make_span("a", "op", 0.0, 1.0, true));
    c.record(make_span("b", "op", 0.0, 1.0, true));
    // Each span has a unique trace id → 2 traces
    assert_eq!(c.trace_count(), 2);
}

// ── §3  Latency Histogram ─────────────────────────────────────────────────────

#[test]
fn histogram_count() {
    let mut h = LatencyHistogram::new();
    h.record_all(vec![1.0, 2.0, 3.0]);
    assert_eq!(h.count(), 3);
}

#[test]
fn histogram_mean() {
    let mut h = LatencyHistogram::new();
    h.record_all(vec![10.0, 20.0, 30.0]);
    assert!((h.mean_ms() - 20.0).abs() < 1e-9);
}

#[test]
fn histogram_p50_median() {
    let mut h = LatencyHistogram::new();
    h.record_all((1..=9).map(|i| i as f64));
    assert!((h.p50_ms() - 5.0).abs() < 1.0);
}

#[test]
fn histogram_p99_near_max() {
    let mut h = LatencyHistogram::new();
    for i in 1..=100 { h.record_ms(i as f64); }
    assert!(h.p99_ms() >= 99.0);
}

#[test]
fn histogram_min_max() {
    let mut h = LatencyHistogram::new();
    h.record_all(vec![3.0, 7.0, 15.0]);
    assert!((h.min_ms() - 3.0).abs() < 1e-9);
    assert!((h.max_ms() - 15.0).abs() < 1e-9);
}

#[test]
fn histogram_empty_zero() {
    let h = LatencyHistogram::new();
    assert_eq!(h.p50_ms(), 0.0);
    assert_eq!(h.mean_ms(), 0.0);
    assert_eq!(h.count(), 0);
}

#[test]
fn histogram_reset() {
    let mut h = LatencyHistogram::new();
    h.record_ms(42.0);
    h.reset();
    assert_eq!(h.count(), 0);
}

#[test]
fn histogram_bucket_counts_cumulative() {
    let mut h = LatencyHistogram::new();
    h.record_all(vec![1.0, 10.0, 100.0]);
    let counts = h.bucket_counts();
    let mut prev = 0u64;
    for (_, c) in counts {
        assert!(c >= prev);
        prev = c;
    }
}

#[test]
fn histogram_custom_buckets() {
    let mut h = LatencyHistogram::with_buckets(BucketBounds::custom(vec![50.0, 200.0, 1000.0]));
    h.record_all(vec![10.0, 100.0, 500.0]);
    let counts = h.bucket_counts();
    assert_eq!(counts.len(), 3);
}

#[test]
fn histogram_snapshot_consistent() {
    let mut h = LatencyHistogram::new();
    h.record_all(vec![10.0, 20.0, 30.0, 40.0, 50.0]);
    let s = h.snapshot();
    assert_eq!(s.count, 5);
    assert!((s.sum_ms - 150.0).abs() < 1e-9);
    assert!(s.p99_ms >= s.p50_ms);
}

#[test]
fn error_rate_tracker_correct() {
    let mut t = ErrorRateTracker::new(10);
    for _ in 0..6 { t.record(false); }
    for _ in 0..4 { t.record(true); }
    assert!((t.rate() - 0.4).abs() < 1e-9);
}

#[test]
fn error_rate_tracker_window_evicts() {
    let mut t = ErrorRateTracker::new(3);
    t.record(true);
    t.record(true);
    t.record(true);
    t.record(false); // evicts oldest true
    assert!((t.rate() - 2.0 / 3.0).abs() < 1e-9);
}

#[test]
fn error_rate_tracker_empty_zero() {
    let t = ErrorRateTracker::new(5);
    assert_eq!(t.rate(), 0.0);
}

// ── §4  Alerting ──────────────────────────────────────────────────────────────

#[test]
fn alert_error_rate_fires_above_threshold() {
    let cond = AlertCondition::error_rate_exceeds(0.10);
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, 0.15));
}

#[test]
fn alert_error_rate_no_fire_at_threshold() {
    let cond = AlertCondition::error_rate_exceeds(0.10);
    let eval = AlertEvaluator::new();
    assert!(!eval.evaluate(&cond, 0.10));
}

#[test]
fn alert_p99_latency_fires() {
    let cond = AlertCondition::p99_latency_exceeds(500.0);
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, 600.0));
}

#[test]
fn alert_throughput_below_fires() {
    let cond = AlertCondition::new(
        "low-rps", ConditionKind::ThroughputBelow(50.0), AlertSeverity::Warning,
    );
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, 30.0));
}

#[test]
fn alert_custom_threshold_fires() {
    let cond = AlertCondition::new(
        "custom",
        ConditionKind::CustomThreshold { metric: "cpu".to_owned(), threshold: 0.8 },
        AlertSeverity::Critical,
    );
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, 0.9));
}

#[test]
fn alert_fired_has_message() {
    let cond = AlertCondition::error_rate_exceeds(0.05);
    let mut eval = AlertEvaluator::new();
    let fired = eval.evaluate_at(&cond, 0.3, 0.0).unwrap();
    assert!(!fired.message.is_empty());
}

#[test]
fn alert_cooldown_suppresses_repeat() {
    let cond = AlertCondition::error_rate_exceeds(0.05).with_cooldown(120);
    let mut eval = AlertEvaluator::new();
    eval.evaluate_at(&cond, 0.2, 0.0).unwrap();
    assert!(eval.evaluate_at(&cond, 0.2, 60_000.0).is_none());
}

#[test]
fn alert_cooldown_fires_after_expiry() {
    let cond = AlertCondition::error_rate_exceeds(0.05).with_cooldown(30);
    let mut eval = AlertEvaluator::new();
    eval.evaluate_at(&cond, 0.2, 0.0).unwrap();
    assert!(eval.evaluate_at(&cond, 0.2, 31_000.0).is_some());
}

#[test]
fn alert_severity_ordering() {
    assert!(AlertSeverity::Critical > AlertSeverity::Warning);
    assert!(AlertSeverity::Warning  > AlertSeverity::Info);
}

#[test]
fn alert_severity_labels() {
    assert_eq!(AlertSeverity::Critical.label(), "CRITICAL");
    assert_eq!(AlertSeverity::Warning.label(),  "WARNING");
    assert_eq!(AlertSeverity::Info.label(),     "INFO");
}

#[test]
fn webhook_notifier_empty_url_error() {
    assert!(matches!(WebhookNotifier::new(""), Err(NotifierError::EmptyUrl)));
}

#[test]
fn webhook_notifier_counts_sent() {
    let mut n = WebhookNotifier::new("https://hooks.example.com/a").unwrap();
    let cond = AlertCondition::p99_latency_exceeds(100.0);
    let mut eval = AlertEvaluator::new();
    n.notify(eval.evaluate_at(&cond, 200.0, 0.0).unwrap()).unwrap();
    n.notify(eval.evaluate_at(&cond, 300.0, 61_000.0).unwrap()).unwrap();
    assert_eq!(n.sent_count(), 2);
}

// ── §5  End-to-end ────────────────────────────────────────────────────────────

#[test]
fn e2e_full_trace_collect_histogram() {
    // Simulate 10 agent invocations, collect spans, compute p99
    let mut tracer = Tracer::new_at("agent-invoke", 0.0);
    let mut latencies = vec![5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 80.0, 100.0, 500.0];

    for (i, &dur) in latencies.iter().enumerate() {
        let id = tracer.start_span(&format!("invoke-{i}"), SpanKind::Internal).span_id.clone();
        tracer.finish_span_at(id, dur, true).unwrap();
    }
    let spans = tracer.drain();

    let mut collector = SpanCollector::new();
    let mut histogram = LatencyHistogram::new();
    for s in spans {
        if let Some(d) = s.duration_ms() { histogram.record_ms(d); }
        collector.record(s);
    }

    assert_eq!(collector.span_count(), 10);
    assert!(histogram.p99_ms() >= 100.0);
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
}

#[test]
fn e2e_error_rate_triggers_alert() {
    let mut tracer = Tracer::new_at("svc", 0.0);
    let mut tracker = ErrorRateTracker::new(20);

    for i in 0..20 {
        let id = tracer.start_span("op", SpanKind::Internal).span_id.clone();
        let ok = i % 4 != 0; // 25% error
        tracer.finish_span(id, ok).unwrap();
        tracker.record(!ok);
    }

    let cond = AlertCondition::error_rate_exceeds(0.20);
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, tracker.rate()));
}

#[test]
fn e2e_slow_span_triggers_latency_alert() {
    let mut t = Tracer::new_at("svc", 0.0);
    let mut hist = LatencyHistogram::new();
    let durations = vec![10.0, 15.0, 20.0, 12.0, 800.0]; // last is slow

    for (i, &dur) in durations.iter().enumerate() {
        let id = t.start_span(&format!("op-{i}"), SpanKind::Internal).span_id.clone();
        t.finish_span_at(id, dur, true).unwrap();
    }
    for s in t.drain() {
        if let Some(d) = s.duration_ms() { hist.record_ms(d); }
    }

    let cond = AlertCondition::p99_latency_exceeds(100.0);
    let eval = AlertEvaluator::new();
    assert!(eval.evaluate(&cond, hist.p99_ms()));
}

#[test]
fn e2e_webhook_fires_for_critical_alert() {
    let cond = AlertCondition::p99_latency_exceeds(200.0);
    let mut eval = AlertEvaluator::new();
    let mut notifier = WebhookNotifier::new("https://hooks.example.com/test").unwrap();

    let fired = eval.evaluate_at(&cond, 350.0, 0.0).unwrap();
    assert!(fired.is_critical());
    notifier.notify(fired).unwrap();
    assert_eq!(notifier.critical_count(), 1);
}

#[test]
fn e2e_child_spans_collected_under_same_trace() {
    let mut t = Tracer::new_at("gateway", 0.0);
    let parent_id  = t.start_span("http.request", SpanKind::Server).span_id.clone();
    let trace_id   = t.spans[0].context.trace_id.clone();
    let _child_id  = t.start_child_span("db.query", SpanKind::Client, parent_id.clone(), trace_id.clone()).span_id.clone();
    let _child2_id = t.start_child_span("cache.get", SpanKind::Internal, parent_id.clone(), trace_id.clone()).span_id.clone();

    // Finish all
    let all_ids: Vec<_> = t.spans.iter().map(|s| s.span_id.clone()).collect();
    for id in all_ids {
        t.finish_span_at(id, 50.0, true).unwrap();
    }

    let spans = t.drain();
    let mut c = SpanCollector::new();
    c.record_all(spans);

    // All 3 spans share the same trace id
    let tid = trace_id.0;
    let store = c.trace(&tid).unwrap();
    assert_eq!(store.span_count(), 3);
}

#[test]
fn e2e_no_alert_when_healthy() {
    let mut hist = LatencyHistogram::new();
    for _ in 0..100 { hist.record_ms(20.0); }

    let mut tracker = ErrorRateTracker::new(100);
    for _ in 0..100 { tracker.record(false); }

    let lat_cond = AlertCondition::p99_latency_exceeds(200.0);
    let err_cond = AlertCondition::error_rate_exceeds(0.05);
    let eval = AlertEvaluator::new();

    assert!(!eval.evaluate(&lat_cond, hist.p99_ms()));
    assert!(!eval.evaluate(&err_cond, tracker.rate()));
}

#[test]
fn e2e_histogram_p99_drives_sample_size_decision() {
    let mut h = LatencyHistogram::new();
    // Simulate 1000 requests: 940×30ms, 10×100ms, 50×800ms
    // With 1000 samples p99 is at sorted index 989 → falls in 800ms bucket
    for _ in 0..940 { h.record_ms(30.0); }
    for _ in 0..10  { h.record_ms(100.0); }
    for _ in 0..50  { h.record_ms(800.0); }

    let p99 = h.p99_ms();
    let cond = AlertCondition::p99_latency_exceeds(500.0);
    let eval = AlertEvaluator::new();
    // p99 = 800ms → exceeds 500ms threshold
    assert!(eval.evaluate(&cond, p99), "p99 was {p99}");
}

#[test]
fn e2e_collector_query_pipeline() {
    let mut c = SpanCollector::new();
    for i in 0..5  { c.record(make_span("worker",  &format!("job-{i}"), 0.0,  50.0, true)); }
    for i in 0..3  { c.record(make_span("gateway", &format!("req-{i}"), 0.0,  20.0, true)); }
    for _  in 0..2 { c.record(make_span("worker",  "job-err",           0.0,  10.0, false)); }

    assert_eq!(c.query(&TraceQuery::new().service("worker")).len(), 7);
    assert_eq!(c.query(&TraceQuery::new().errors_only()).len(), 2);
    assert_eq!(c.query(&TraceQuery::new().service("gateway")).len(), 3);
    assert!((c.global_error_rate() - 0.2).abs() < 0.01);
}

#[test]
fn e2e_alert_resets_and_re_evaluates() {
    let cond = AlertCondition::error_rate_exceeds(0.1).with_cooldown(300);
    let mut eval = AlertEvaluator::new();

    let f1 = eval.evaluate_at(&cond, 0.5, 0.0);
    assert!(f1.is_some());

    eval.reset(); // clear cooldown state

    let f2 = eval.evaluate_at(&cond, 0.5, 1.0); // immediately after reset
    assert!(f2.is_some(), "expected alert to fire again after reset");
}

#[test]
fn e2e_tracer_service_label_propagates() {
    let mut t = Tracer::new("quota-enforcer");
    let id = t.start_span("check", SpanKind::Internal).span_id.clone();
    t.finish_span(id, true).unwrap();
    let spans = t.drain();
    assert_eq!(spans[0].service_name, "quota-enforcer");
}
