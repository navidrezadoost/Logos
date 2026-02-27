//! Unified identity, authentication, and permission layer for Logos.
//!
//! This crate provides the canonical `User`, `Role`, `Permission`, and
//! `Session` types used across all Logos subsystems. It replaces the
//! previously fragmented auth landscape:
//!
//! | Before | After |
//! |--------|-------|
//! | `logos-comments::UserRole` | `logos_identity::Role` |
//! | `logos-sync::SessionUserRole` | `logos_identity::Role` |
//! | `logos-collab::auth::Claims` | `logos_identity::TokenClaims` |
//! | `logos-marketplace-auth::PublisherIdentity` | linked via `UserId` |
//! | `logos-plugins::PermissionGuard` | maps to `Permission` |
//!
//! ## Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │                logos-identity                  │
//! │                                               │
//! │  User ◄── UserId ──► Role ──► PermissionSet  │
//! │    │                  │              │         │
//! │    ▼                  ▼              ▼         │
//! │  UserStore      AccessControlList  Permission │
//! │    │                  │                        │
//! │    ▼                  ▼                        │
//! │  Session ◄── SessionStore ──► AuditLog        │
//! │    │                                          │
//! │    ▼                                          │
//! │  TokenClaims ◄── TokenProvider                │
//! │    │                                          │
//! │    ▼                                          │
//! │  OAuthProvider ──► Credential                 │
//! │                                               │
//! │  IdentityManager (orchestrates all above)     │
//! └───────────────────────────────────────────────┘
//! ```
//!
//! ## Minimal Dependencies
//!
//! Only `serde`, `uuid`, `thiserror`, `serde_json` — no crypto, no HTTP,
//! no async runtime. Crypto and transport are provided by consumer crates
//! (e.g., `logos-collab` implements `TokenProvider` with HMAC-SHA256).

pub mod error;
pub mod user;
pub mod role;
pub mod permission;
pub mod credential;
pub mod session;
pub mod token;
pub mod store;
pub mod session_store;
pub mod oauth;
pub mod acl;
pub mod audit;
pub mod manager;

// Re-exports — the public API surface
pub use error::IdentityError;
pub use user::{UserId, User, UserProfile, AuthProvider, AccountStatus};
pub use role::Role;
pub use permission::{Permission, PermissionSet};
pub use credential::{Credential, PasswordCredential, OAuthCredential, ApiKeyCredential, HashAlgorithm};
pub use session::{SessionId, Session};
pub use token::{TokenClaims, TokenKind, TokenProvider};
pub use store::{UserStore, InMemoryUserStore};
pub use session_store::{SessionStore, InMemorySessionStore};
pub use oauth::{OAuthConfig, OAuthState, OAuthTokenResponse, OAuthUserInfo, OAuthProvider};
pub use acl::{AccessControlList, AccessControlEntry};
pub use audit::{AuditEntry, AuditAction, ResourceType, AuditFilter, AuditLog, InMemoryAuditLog};
pub use manager::IdentityManager;
pub use manager::IdentityConfig;
