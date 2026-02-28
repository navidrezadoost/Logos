//! Agent Dispatcher — routes UI requests to certified agent slots
//!
//! The dispatcher maintains a pool of `AgentSlot`s (active agent sessions),
//! selects the best one for each `DispatchRequest` based on routing policy,
//! and records latency/success metrics.

use serde::{Deserialize, Serialize};

// ── Routing policy ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingPolicy {
    /// First available slot regardless of level.
    BestAvailable,
    /// Prefer slots at a specific level string (e.g., "Senior").
    ByLevel(String),
    /// Route to a specific session ID.
    ToSessionId(String),
    /// Distribute evenly (by request count).
    RoundRobin,
    /// Highest-score agent wins.
    MostCertified,
}

// ── Agent slot ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSlot {
    pub session_id: String,
    pub level: String,
    pub provider: String,
    pub busy: bool,
    pub last_used_ts: u64,
    pub total_requests: u32,
    pub success_requests: u32,
    pub avg_latency_ms: f64,
}

impl AgentSlot {
    pub fn new(session_id: impl Into<String>, level: impl Into<String>, provider: impl Into<String>) -> Self {
        AgentSlot {
            session_id: session_id.into(),
            level: level.into(),
            provider: provider.into(),
            busy: false,
            last_used_ts: 0,
            total_requests: 0,
            success_requests: 0,
            avg_latency_ms: 0.0,
        }
    }

    pub fn is_available(&self) -> bool { !self.busy }

    pub fn success_rate(&self) -> f32 {
        if self.total_requests == 0 { return 1.0; }
        self.success_requests as f32 / self.total_requests as f32
    }

    pub fn record_outcome(&mut self, success: bool, latency_ms: u64, ts: u64) {
        self.total_requests += 1;
        if success { self.success_requests += 1; }
        // Exponential moving average for latency
        let alpha = 0.2;
        self.avg_latency_ms = (1.0 - alpha) * self.avg_latency_ms + alpha * latency_ms as f64;
        self.last_used_ts = ts;
        self.busy = false;
    }
}

// ── Dispatch request ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchRequest {
    pub request_id: String,
    pub user_message: String,
    pub context_json: Option<String>,
    pub routing: RoutingPolicy,
    pub timeout_ms: u64,
    pub priority: DispatchPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl DispatchRequest {
    pub fn new(user_message: impl Into<String>, routing: RoutingPolicy) -> Self {
        DispatchRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            user_message: user_message.into(),
            context_json: None,
            routing,
            timeout_ms: 30_000,
            priority: DispatchPriority::Normal,
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context_json = Some(ctx.into());
        self
    }

    pub fn with_priority(mut self, p: DispatchPriority) -> Self {
        self.priority = p;
        self
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }
}

// ── Dispatch response ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResponse {
    pub request_id: String,
    pub agent_session_id: String,
    pub response_text: String,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub token_count: u32,
}

impl DispatchResponse {
    pub fn success(request_id: impl Into<String>, agent_session_id: impl Into<String>, text: impl Into<String>, latency_ms: u64) -> Self {
        DispatchResponse {
            request_id: request_id.into(),
            agent_session_id: agent_session_id.into(),
            response_text: text.into(),
            latency_ms,
            success: true,
            error: None,
            token_count: 0,
        }
    }

    pub fn failed(request_id: impl Into<String>, error: impl Into<String>) -> Self {
        DispatchResponse {
            request_id: request_id.into(),
            agent_session_id: String::new(),
            response_text: String::new(),
            latency_ms: 0,
            success: false,
            error: Some(error.into()),
            token_count: 0,
        }
    }
}

// ── Dispatch status ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DispatchStatus {
    Queued,
    InFlight { agent_session_id: String },
    Completed { request_id: String },
    Failed { request_id: String, reason: String },
    TimedOut { request_id: String },
    Rejected { reason: String },
}

impl DispatchStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, DispatchStatus::Completed { .. } | DispatchStatus::Failed { .. }
            | DispatchStatus::TimedOut { .. } | DispatchStatus::Rejected { .. })
    }
}

// ── Dispatcher config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    pub max_concurrent: usize,
    pub default_timeout_ms: u64,
    pub max_retry_count: u8,
    pub queue_capacity: usize,
}

impl Default for DispatcherConfig {
    fn default() -> Self {
        DispatcherConfig {
            max_concurrent: 4,
            default_timeout_ms: 30_000,
            max_retry_count: 2,
            queue_capacity: 64,
        }
    }
}

// ── Dispatch metrics ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DispatchMetrics {
    pub total_dispatched: u32,
    pub total_success: u32,
    pub total_failed: u32,
    pub total_timed_out: u32,
    pub avg_latency_ms: f64,
    pub peak_concurrent: usize,
}

impl DispatchMetrics {
    pub fn record(&mut self, success: bool, latency_ms: u64) {
        self.total_dispatched += 1;
        if success {
            self.total_success += 1;
        } else {
            self.total_failed += 1;
        }
        let alpha = 0.1;
        self.avg_latency_ms = (1.0 - alpha) * self.avg_latency_ms + alpha * latency_ms as f64;
    }

    pub fn record_timeout(&mut self) { self.total_timed_out += 1; self.total_dispatched += 1; }

    pub fn success_rate(&self) -> f32 {
        if self.total_dispatched == 0 { return 1.0; }
        self.total_success as f32 / self.total_dispatched as f32
    }
}

// ── Agent dispatcher ──────────────────────────────────────────────────────────

/// Maintains a pool of agent slots and routes dispatch requests.
pub struct AgentDispatcher {
    pub slots: Vec<AgentSlot>,
    pub config: DispatcherConfig,
    pub metrics: DispatchMetrics,
    active_count: usize,
}

impl AgentDispatcher {
    pub fn new(config: DispatcherConfig) -> Self {
        AgentDispatcher { slots: Vec::new(), config, metrics: DispatchMetrics::default(), active_count: 0 }
    }

    /// Register a new agent slot.
    pub fn register(&mut self, slot: AgentSlot) {
        self.slots.push(slot);
    }

    /// Remove an agent slot by session ID.
    pub fn deregister(&mut self, session_id: &str) -> bool {
        if let Some(pos) = self.slots.iter().position(|s| s.session_id == session_id) {
            self.slots.remove(pos);
            true
        } else {
            false
        }
    }

    /// Select the best available slot for a request.
    pub fn select_slot(&mut self, routing: &RoutingPolicy) -> Option<&mut AgentSlot> {
        match routing {
            RoutingPolicy::BestAvailable => {
                self.slots.iter_mut().find(|s| s.is_available())
            }
            RoutingPolicy::ByLevel(level) => {
                self.slots.iter_mut()
                    .filter(|s| s.is_available())
                    .find(|s| s.level.to_lowercase() == level.to_lowercase())
            }
            RoutingPolicy::ToSessionId(id) => {
                self.slots.iter_mut().find(|s| &s.session_id == id && s.is_available())
            }
            RoutingPolicy::RoundRobin => {
                self.slots.iter_mut()
                    .filter(|s| s.is_available())
                    .min_by_key(|s| s.total_requests)
            }
            RoutingPolicy::MostCertified => {
                // Prefer slots with higher success rate and lower latency
                self.slots.iter_mut()
                    .filter(|s| s.is_available())
                    .max_by(|a, b| {
                        a.success_rate().partial_cmp(&b.success_rate()).unwrap_or(std::cmp::Ordering::Equal)
                    })
            }
        }
    }

    /// Simulate a synchronous dispatch (in production, would be async).
    pub fn dispatch_sync(&mut self, req: &DispatchRequest, ts: u64) -> DispatchResponse {
        if self.active_count >= self.config.max_concurrent {
            return DispatchResponse::failed(&req.request_id, "dispatcher: max concurrent limit reached");
        }

        let slot_id = match self.select_slot(&req.routing) {
            Some(slot) => {
                slot.busy = true;
                let id = slot.session_id.clone();
                id
            }
            None => {
                return DispatchResponse::failed(&req.request_id, "dispatcher: no available slot");
            }
        };

        self.active_count += 1;

        // Simulate agent processing (echo back for testing)
        let simulated_latency = 42u64;
        let response_text = format!("[Agent {}]: Processed: {}", &slot_id[..8.min(slot_id.len())], req.user_message);

        // Record outcome
        if let Some(slot) = self.slots.iter_mut().find(|s| s.session_id == slot_id) {
            slot.record_outcome(true, simulated_latency, ts);
        }
        self.active_count = self.active_count.saturating_sub(1);
        self.metrics.record(true, simulated_latency);

        DispatchResponse::success(&req.request_id, &slot_id, response_text, simulated_latency)
    }

    pub fn available_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_available()).count()
    }

    pub fn slot_count(&self) -> usize { self.slots.len() }
    pub fn busy_count(&self) -> usize { self.slots.iter().filter(|s| s.busy).count() }
    pub fn is_saturated(&self) -> bool { self.active_count >= self.config.max_concurrent }
}

impl Default for AgentDispatcher {
    fn default() -> Self { Self::new(DispatcherConfig::default()) }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatcher_with_slots() -> AgentDispatcher {
        let mut d = AgentDispatcher::default();
        d.register(AgentSlot::new("session-junior-1", "Junior", "builtin"));
        d.register(AgentSlot::new("session-mid-1", "Mid", "builtin"));
        d.register(AgentSlot::new("session-senior-1", "Senior", "builtin"));
        d
    }

    #[test]
    fn dispatcher_registers_slots() {
        let d = dispatcher_with_slots();
        assert_eq!(d.slot_count(), 3);
        assert_eq!(d.available_count(), 3);
    }

    #[test]
    fn deregister_removes_slot() {
        let mut d = dispatcher_with_slots();
        assert!(d.deregister("session-junior-1"));
        assert_eq!(d.slot_count(), 2);
        assert!(!d.deregister("nonexistent"));
    }

    #[test]
    fn dispatch_best_available() {
        let mut d = dispatcher_with_slots();
        let req = DispatchRequest::new("draw a red rectangle", RoutingPolicy::BestAvailable);
        let resp = d.dispatch_sync(&req, 1000);
        assert!(resp.success);
        assert!(!resp.response_text.is_empty());
        assert_eq!(d.metrics.total_success, 1);
    }

    #[test]
    fn dispatch_by_level() {
        let mut d = dispatcher_with_slots();
        let req = DispatchRequest::new("design review", RoutingPolicy::ByLevel("Senior".into()));
        let resp = d.dispatch_sync(&req, 1000);
        assert!(resp.success);
        assert!(resp.agent_session_id.contains("senior"));
    }

    #[test]
    fn dispatch_to_session_id() {
        let mut d = dispatcher_with_slots();
        let req = DispatchRequest::new("hi", RoutingPolicy::ToSessionId("session-mid-1".into()));
        let resp = d.dispatch_sync(&req, 1000);
        assert!(resp.success);
        assert_eq!(resp.agent_session_id, "session-mid-1");
    }

    #[test]
    fn dispatch_round_robin() {
        let mut d = dispatcher_with_slots();
        // First dispatch goes to slot with fewest requests
        let req = DispatchRequest::new("hello", RoutingPolicy::RoundRobin);
        let r1 = d.dispatch_sync(&req, 100);
        assert!(r1.success);
    }

    #[test]
    fn dispatch_fails_when_no_slots() {
        let mut d = AgentDispatcher::default();
        let req = DispatchRequest::new("hello", RoutingPolicy::BestAvailable);
        let resp = d.dispatch_sync(&req, 100);
        assert!(!resp.success);
        assert!(resp.error.is_some());
    }

    #[test]
    fn dispatch_fails_by_level_when_no_match() {
        let mut d = dispatcher_with_slots();
        let req = DispatchRequest::new("hello", RoutingPolicy::ByLevel("Principal".into()));
        let resp = d.dispatch_sync(&req, 100);
        assert!(!resp.success);
    }

    #[test]
    fn metrics_track_success() {
        let mut d = dispatcher_with_slots();
        for i in 0..5 {
            let req = DispatchRequest::new(format!("msg {}", i), RoutingPolicy::BestAvailable);
            d.dispatch_sync(&req, i as u64 * 100);
        }
        assert_eq!(d.metrics.total_dispatched, 5);
        assert!(d.metrics.success_rate() == 1.0);
    }

    #[test]
    fn slot_success_rate() {
        let mut slot = AgentSlot::new("s", "Junior", "builtin");
        slot.record_outcome(true, 50, 100);
        slot.record_outcome(true, 60, 200);
        slot.record_outcome(false, 0, 300);
        assert!((slot.success_rate() - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn dispatch_status_is_terminal() {
        assert!(DispatchStatus::Completed { request_id: "x".into() }.is_terminal());
        assert!(DispatchStatus::Failed { request_id: "x".into(), reason: "e".into() }.is_terminal());
        assert!(!DispatchStatus::Queued.is_terminal());
        assert!(!DispatchStatus::InFlight { agent_session_id: "s".into() }.is_terminal());
    }

    #[test]
    fn request_builder_chain() {
        let req = DispatchRequest::new("help me", RoutingPolicy::BestAvailable)
            .with_context(r#"{"page":"home"}"#)
            .with_timeout(5000)
            .with_priority(DispatchPriority::High);
        assert!(req.context_json.is_some());
        assert_eq!(req.timeout_ms, 5000);
        assert_eq!(req.priority, DispatchPriority::High);
    }

    #[test]
    fn metrics_avg_latency_updates() {
        let mut m = DispatchMetrics::default();
        m.record(true, 100);
        m.record(true, 200);
        assert!(m.avg_latency_ms > 0.0 && m.avg_latency_ms < 200.0);
    }
}
