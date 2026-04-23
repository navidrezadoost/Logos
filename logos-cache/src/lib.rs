//! logos-cache — embedded in-memory cache, session store, and WebSocket broker.
//!
//! Replaces Redis / Valkey entirely. No external daemon required.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                      logos-cache                        │
//! │                                                         │
//! │   ┌──────────────┐   ┌──────────────┐  ┌───────────┐  │
//! │   │ SessionStore │   │  CacheStore  │  │  Broker   │  │
//! │   │  (moka TTL)  │   │ (moka::Cache)│  │ (dashmap) │  │
//! │   └──────────────┘   └──────────────┘  └───────────┘  │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! All three components share the same Tokio runtime as the rest of
//! `logos-server` with zero cross-process overhead.

pub mod broker;
pub mod cache;
pub mod session;

pub use broker::{Broker, BrokerError, Message as BrokerMessage};
pub use cache::{CacheStore, CacheError};
pub use session::{SessionStore, SessionError, UserSession};
