//! # logos-plugins — Plugin System for Logos Design Engine
//!
//! Provides a sandboxed plugin runtime that lets third-party code
//! extend Logos without compromising engine stability or security.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────┐
//! │         PluginManager        │  lifecycle, registry
//! │   ┌──────────┬──────────┐   │
//! │   │ Manifest │ Sandbox  │   │  metadata + execution
//! │   └──────────┴──────────┘   │
//! │         PluginHost           │  document bridge
//! │       PermissionGuard        │  access control
//! └──────────────────────────────┘
//!              │
//!              ▼
//!     logos_core::Document        core engine
//! ```
//!
//! ## Modules
//!
//! - [`runtime`] — Sandboxed execution, resource limits, expression evaluator
//! - [`engine`] — Real JavaScript engine (boa_engine ES2023)
//! - [`manifest`] — Plugin metadata, hooks, commands
//! - [`permissions`] — Capability-based security, domain/path scoping
//! - [`host`] — Document bridge, host functions
//! - [`manager`] — Plugin lifecycle, loading, registry

pub mod engine;
pub mod host;
pub mod manifest;
pub mod manager;
pub mod permissions;
pub mod runtime;

// Re-export key types for convenience
pub use engine::{EventBus, JsEngine, UiBridge};
pub use host::PluginHost;
pub use manager::{PluginInstance, PluginManager, PluginRuntime, PluginState};
pub use manifest::{PluginCommand, PluginHook, PluginManifest, SemVer};
pub use permissions::{PermissionGuard, PermissionKind, PermissionSet};
pub use runtime::{ExecutionStats, HostFn, PluginValue, ResourceLimits, RuntimeError, Sandbox};
