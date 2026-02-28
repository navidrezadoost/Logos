//! logos-rl-ux — Phase 15.2: RL-UX in Production
//!
//! Production deployment layer for the reinforcement learning UX system.
//! Provides a persistent Q-table, multi-factor reward modeling, telemetry
//! collection, A/B testing, and a policy engine for serving suggestions.

pub mod q_table;
pub mod data_collector;
pub mod ab_testing;
pub mod reward_model;
pub mod policy_engine;

// ── Re-exports ────────────────────────────────────────────────────────────────

// Q-table
pub use q_table::{
    QTable, StateKey, QEntry, ReplayBuffer, Experience, DecaySchedule, QTableCheckpoint,
};

// Data collector
pub use data_collector::{
    DataCollector, CollectorConfig, InteractionEvent, SessionStats, DataBatch,
};

// A/B testing
pub use ab_testing::{
    Experiment, ExperimentConfig, ExperimentRegistry, ExperimentStatus,
    ExperimentVariant, TrafficSplit, VariantMetrics, StatTest,
};

// Reward model
pub use reward_model::{
    RewardModel, RewardConfig, RewardSignal, RewardSource, RewardHistory,
    InteractionSnapshot,
};

// Policy engine
pub use policy_engine::{
    PolicyEngine, PolicyConfig, PolicyVariant, PredictionRequest,
    PredictionResult, Suggestion, Feedback, PolicyMetrics, HeuristicBaseline,
};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RlUxError {
    #[error("q-table error: {0}")]
    QTable(String),

    #[error("data collection error: {0}")]
    Collection(String),

    #[error("experiment error: {0}")]
    Experiment(String),

    #[error("policy error: {0}")]
    Policy(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
