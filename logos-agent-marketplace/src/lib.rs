//! logos-agent-marketplace — Phase 15.3: Agent Marketplace
//!
//! Discovery, certification, ratings, and install management for Logos agents.
//! Turns agents from a feature into a self-sustaining ecosystem.

pub mod manifest;
pub mod registry;
pub mod ratings;
pub mod install;
pub mod certification;
pub mod submission;

// ── Re-exports ────────────────────────────────────────────────────────────────

// Manifest
pub use manifest::{
    AgentCategory, AgentManifest, AgentVersion, CompatibilityMatrix, PricingModel,
};

// Registry
pub use registry::{
    MarketplaceRegistry, PublisherProfile, SearchQuery, SearchResult, SortOrder,
};

// Ratings
pub use ratings::{
    ModerationStatus, Rating, RatingSummary, Review, ReviewStore,
};

// Install
pub use install::{
    DependencyResolver, InstallRegistry, InstallRequest, InstallResult,
    InstalledAgent, InstallStatus,
};

// Certification
pub use certification::{
    CertificationLevel, CertificationRegistry, CertificationRequest,
    CertificationResult, Certifier, CheckResult, CheckSeverity,
};

// Submission portal
pub use submission::{Submission, SubmissionStatus, SubmissionStore, SubmissionError};

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MarketplaceError {
    #[error("manifest error: {0}")]
    Manifest(String),

    #[error("certification failed: {0}")]
    Certification(String),

    #[error("install error: {0}")]
    Install(String),

    #[error("review moderation error: {0}")]
    Moderation(String),

    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}
