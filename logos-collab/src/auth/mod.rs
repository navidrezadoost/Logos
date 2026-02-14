//! Authentication & authorization for the collaboration server.
//!
//! Architecture:
//! ```text
//! WebSocket Upgrade
//!       │
//!       ▼
//! ┌─────────────┐     verify_token()     ┌─────────────┐
//! │  Middleware  │ ──────<300ns────────►  │  TokenEngine │
//! │  (per-conn) │                        │  (HMAC-SHA256)│
//! └──────┬──────┘                        └──────────────┘
//!        │
//!        ▼
//! ┌─────────────┐     check()            ┌─────────────┐
//! │  Per-Message │ ──────<200ns────────►  │ RateLimiter  │
//! │  Handler    │                        │ (token bucket)│
//! └─────────────┘                        └──────────────┘
//! ```
//!
//! ## Performance Targets
//!
//! | Operation | Target | Reference |
//! |-----------|--------|-----------|
//! | Token issuance | <500ns | OWASP - JWT Best Practices |
//! | Token verification | <300ns | Computer Architecture §2.3 |
//! | Rate limit check | <200ns | DDIA, Chapter 11 |
//! | Memory per user | <128 bytes | — |
//!
//! ## Security References
//!
//! - OWASP Testing Guide v4 — Session Management
//! - RFC 7519 — JSON Web Tokens
//! - DDIA Chapter 11 — Stream Processing (rate limiting)

pub mod token;
pub mod ratelimit;
pub mod middleware;

pub use token::{TokenEngine, Claims, TokenError};
pub use ratelimit::{RateLimiter, RateLimitConfig, TokenBucket};
pub use middleware::{AuthMiddleware, AuthConfig, AuthError};
