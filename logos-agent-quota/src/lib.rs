//! # logos-agent-quota
//!
//! Per-tenant invocation caps, token budgets, and token-bucket rate limiting
//! for Logos AI Agents.
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_quota::{
//!     TenantQuota, QuotaManager,
//!     TokenBucket, BucketConfig,
//!     UsageTracker,
//!     RateLimiter,
//! };
//!
//! // Configure a quota for a tenant
//! let quota = TenantQuota::new("acme-corp", 1_000, 50_000);
//! let mut manager = QuotaManager::new();
//! manager.register(quota);
//!
//! // Track usage
//! let mut tracker = UsageTracker::new();
//! tracker.record("acme-corp", "agent-x", 1, 200);
//! let usage = tracker.usage_for("acme-corp");
//! assert_eq!(usage.invocations, 1);
//!
//! // Token-bucket rate limiter: 10 req/s, burst 20
//! let cfg = BucketConfig::new(10.0, 20);
//! let mut bucket = TokenBucket::new(cfg);
//! assert!(bucket.try_acquire(1).is_ok());
//!
//! // Composite rate limiter
//! let mut limiter = RateLimiter::new();
//! limiter.add_tenant("acme-corp", BucketConfig::new(5.0, 10));
//! assert!(limiter.check("acme-corp", 1).is_ok());
//! ```

pub mod quota;
pub mod bucket;
pub mod tracker;
pub mod limiter;

pub use quota::{TenantQuota, QuotaManager, QuotaPolicy, QuotaError, InvocationQuota};
pub use bucket::{TokenBucket, BucketConfig, BucketError};
pub use tracker::{UsageTracker, TenantUsage, UsageSnapshot};
pub use limiter::{RateLimiter, LimiterError, ThrottleAction};
