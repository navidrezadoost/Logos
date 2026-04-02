//! `logos-agent-sandbox` — Agent Testing Sandbox for Logos Agents
//!
//! Provides a complete pre-publish testing environment:
//!
//! * [`sandbox`]       — Isolated runtime with mock file system, canvas state, clipboard
//! * [`simulator`]     — Simulated user interactions: click, type, drag, scroll
//! * [`profiler`]      — Performance metrics: latency, token count, memory snapshots
//! * [`certification`] — Reuse of Phase 14 certification suite (50 questions) in sandbox
//! * [`reporter`]      — Test result reports: pass/fail, failure reasons, JSON export
//! * [`integration`]   — CLI runner + marketplace publish gate

pub mod sandbox;
pub mod simulator;
pub mod profiler;
pub mod certification;
pub mod reporter;
pub mod integration;

// ── Flat re-exports ───────────────────────────────────────────────────────────

pub use sandbox::{
    CanvasLayer, CanvasState, Clipboard, MockFileSystem, ResourceLimits,
    SandboxEnv, SandboxError, SandboxResult,
};
pub use simulator::{
    DragEvent, InteractionEvent, InteractionSimulator, KeyEvent, PointerEvent,
    ScrollEvent, SimulatorConfig,
};
pub use profiler::{
    MemorySnapshot, PerformanceProfiler, ProfilerConfig, RunMetrics, TokenStats,
};
pub use certification::{
    CertQuestion, CertQuestionResult, CertificationRunner, CertificationSummary,
    SandboxCertConfig,
};
pub use reporter::{
    FailureReason, ReportFormat, SandboxReport, SandboxTestResult, TestStatus,
};
pub use integration::{
    MarketplaceGate, PublishDecision, SandboxCliRunner, SandboxRunConfig,
};
