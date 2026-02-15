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

pub mod engine;
pub mod host;
pub mod manifest;
pub mod manager;
pub mod marketplace;
pub mod packaging;
pub mod permissions;
pub mod registry;
pub mod runtime;
pub mod signing;

// Re-export key types for convenience
pub use engine::{EventBus, JsEngine, UiBridge};
pub use host::PluginHost;
pub use marketplace::{MarketplaceClient, PackageBuilder, TrustedPublishers};
pub use manager::{PluginInstance, PluginManager, PluginRuntime, PluginState};
pub use manifest::{PluginCategory, PluginCommand, PluginHook, PluginManifest, SemVer};
pub use packaging::{IconSize, PackageFlags, PluginPackage};
pub use permissions::{PermissionGuard, PermissionKind, PermissionSet};
pub use registry::{InstalledPlugin, PluginFilter, PluginRegistry, RegistrySource};
pub use runtime::{ExecutionStats, HostFn, PluginValue, ResourceLimits, RuntimeError, Sandbox};
pub use signing::{ContentHash, PluginKeyPair, PluginPublicKey, PluginSignature, SigningContext};
