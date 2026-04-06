//! # logos-agent-rollback
//!
//! Agent Version Rollback & A/B Testing for the Logos platform.
//!
//! ## Modules
//! - [`store`] — immutable versioned snapshots of agent configurations
//! - [`rollback`] — rollback engine with audit log
//! - [`ab`] — traffic-splitting experiments between two versions
//! - [`diff`] — structured diff between any two snapshots
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_rollback::store::{VersionStore, AgentSnapshot};
//! use logos_agent_rollback::rollback::RollbackEngine;
//!
//! // 1. Store two snapshots
//! let mut store = VersionStore::new();
//! store.save(AgentSnapshot::new("bot", 1, "v1.0", 1000)).unwrap();
//! store.save(AgentSnapshot::new("bot", 2, "v2.0", 2000)).unwrap();
//! store.set_active("bot", 2).unwrap();
//!
//! // 2. Roll back to v1
//! let mut engine = RollbackEngine::new();
//! let rec = engine.rollback(&mut store, "bot", 1, "regression", 3000).unwrap();
//! assert_eq!(rec.to_version, 1);
//! assert_eq!(store.active("bot").unwrap().version, 1);
//! ```

pub mod ab;
pub mod diff;
pub mod rollback;
pub mod store;
