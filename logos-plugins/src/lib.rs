//! # logos-plugins — Plugin System for Logos Design Engine
//!
//! Provides a sandboxed plugin runtime that lets third-party code
//! extend Logos without compromising engine stability or security.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────┐
//! │         PluginManager            │  lifecycle, registry
//! │   ┌──────────┬──────────┐       │
//! │   │ Manifest │ Sandbox  │       │  metadata + execution
//! │   └──────────┴──────────┘       │
//! │         PluginHost               │  document bridge
//! │       PermissionGuard            │  access control
//! │  ┌──────────────────────────┐   │
//! │  │   Signing + Packaging    │   │  Ed25519, .logos-plugin
//! │  │   PluginRegistry         │   │  install/upgrade/search
//! │  └──────────────────────────┘   │
//! └──────────────────────────────────┘
//!              │
//!              ▼
//!     logos_core::Document            core engine
//! ```
//!
//! ## Modules
//!
//! - [`runtime`] — Sandboxed execution, resource limits, expression evaluator
//! - [`engine`] — Real JavaScript engine (boa_engine ES2023)
//! - [`manifest`] — Plugin metadata, hooks, commands, marketplace metadata
//! - [`permissions`] — Capability-based security, domain/path scoping
//! - [`host`] — Document bridge, host functions
//! - [`manager`] — Plugin lifecycle, loading, registry
//! - [`signing`] — Ed25519 digital signatures, SHA-256 content hashing
//! - [`packaging`] — .logos-plugin binary format, compression, icons
//! - [`registry`] — Local plugin registry, install/upgrade/search
//! - [`hot_reload`] — File watching, module reloading, debounce
//! - [`crash_recovery`] — Crash reports, recovery policies, restart decisions
//! - [`sandbox_monitor`] — Memory/execution tracking, health scores
//! - [`update_scheduler`] — Update checking, scheduling, staged rollout
//! - [`storage`] — Per-plugin key-value storage with quotas
//! - [`discovery`] — Trending plugins, category browsing, recommendations
//! - [`sdk`] — Plugin project scaffolding and templates

pub mod crash_recovery;
pub mod discovery;
pub mod engine;
pub mod examples;
pub mod host;
pub mod hot_reload;
#[cfg(test)]
mod integration_tests;
pub mod js_migration;
pub mod manifest;
pub mod manager;
pub mod marketplace;
pub mod marketplace_http;
pub mod packaging;
pub mod permission_prompt;
pub mod permissions;
pub mod registry;
pub mod runtime;
pub mod sandbox_monitor;
pub mod sdk;
pub mod signing;
pub mod storage;
pub mod update_scheduler;

// Re-export key types for convenience
pub use engine::{EventBus, JsEngine, UiBridge, WasmRuntime};
pub use host::PluginHost;
pub use js_migration::{
    JsWasmBridge, WasmPayload, CompilationStats, DeprecationStatus,
    MigrationAnalysis, MigrationSummary,
    boa_deprecation_status, analyze_migration, analyze_all_migrations,
    summarize_migrations,
};
pub use marketplace::{MarketplaceClient, PackageBuilder, TrustedPublishers};
pub use marketplace_http::{ApiEndpoint, ApiError, ApiResponse, DownloadProgress, DownloadState, InstallTransaction, MarketplaceHttpClient, PluginUpdate, RateLimiter, RetryPolicy, TransactionState};
pub use manager::{PluginInstance, PluginManager, PluginRuntime, PluginState};
pub use manifest::{ManifestError, PluginCategory, PluginCommand, PluginHook, PluginManifest, SemVer, TomlManifest};
pub use packaging::{IconSize, PackageFlags, PluginPackage};
pub use permissions::{PermissionGuard, PermissionKind, PermissionSet};
pub use permission_prompt::{InstallApproval, PermissionDecision, PermissionPromptItem, PermissionPromptSession, RiskLevel, SavedPermissionPreferences};
pub use registry::{InstalledPlugin, PluginFilter, PluginRegistry, RegistrySource};
pub use runtime::{ExecutionStats, HostFn, PluginValue, ResourceLimits, RuntimeError, Sandbox};
pub use signing::{CertificateChain, ContentHash, PluginKeyPair, PluginPublicKey, PluginSignature, SignatureVerifier, SigningContext, TrustCertificate, VerificationPolicy, VerificationResult};

// Phase 12 re-exports
pub use hot_reload::{FileWatcher, HotReloadManager, WatcherConfig, ReloadResult, ReloadEvent, ChangeKind, StatePreservation};
pub use crash_recovery::{CrashRecoveryManager, CrashReport, CrashKind, RecoveryPolicy, RecoveryStrategy, RestartDecision};
pub use sandbox_monitor::{SandboxDashboard, ResourceBudget, HealthScore, MemoryTracker, ExecutionMonitor};
pub use update_scheduler::{UpdateScheduler, UpdatePolicy, PendingUpdate, UpdatePriority, UpdateAction, UpdateFrequency, AutoUpdateMode};
pub use storage::{StorageManager, StorageQuota, StorageError};
pub use discovery::{TrendingTracker, TrendingEntry, CategoryBrowser, PluginRecommender, Recommendation, RecommendationReason};
pub use sdk::{PluginScaffold, ScaffoldConfig, TemplateKind, GeneratedFile};
