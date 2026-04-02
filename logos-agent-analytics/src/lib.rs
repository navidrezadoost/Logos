//! `logos-agent-analytics` — Agent Analytics Dashboard for Logos.
//!
//! # Modules
//!
//! * [`metrics`]    — Raw event recording: invocations, latency, tokens, outcomes
//! * [`aggregator`] — Time-windowed aggregation and per-version roll-ups
//! * [`feedback`]   — User ratings and comments attached to agent sessions
//! * [`dashboard`]  — Unified view: top performers, version comparisons, alerts
//!
//! # Quick start
//!
//! ```rust
//! use logos_agent_analytics::{
//!     MetricsCollector, InvocationEvent, OutcomeKind,
//!     FeedbackStore, UserFeedback,
//!     Dashboard,
//! };
//!
//! let mut col = MetricsCollector::new();
//! col.record(InvocationEvent::new("agent-a", "1.0.0", OutcomeKind::Success, 120, 800));
//!
//! let mut fb = FeedbackStore::new();
//! fb.submit(UserFeedback::new("agent-a", "1.0.0", "sess-1", 5));
//!
//! let dash = Dashboard::build(&col, &fb);
//! println!("{}", dash.summary_text());
//! ```

pub mod metrics;
pub mod aggregator;
pub mod feedback;
pub mod dashboard;

// ── Flat re-exports ───────────────────────────────────────────────────────────

pub use metrics::{InvocationEvent, MetricsCollector, OutcomeKind, MetricsError};
pub use aggregator::{Aggregator, AgentVersionStats, TimeWindow};
pub use feedback::{FeedbackStore, UserFeedback, FeedbackSummary, FeedbackError};
pub use dashboard::{Dashboard, DashboardAlert, AlertKind, VersionComparison};
