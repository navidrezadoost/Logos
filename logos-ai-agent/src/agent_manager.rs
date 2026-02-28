//! Agent Manager — store user API tokens, manage agent sessions, rate limiting
//!
//! Users provide API keys (OpenAI, Anthropic, etc.) in Logos settings.
//! This module hashes+stores them securely, manages session lifecycle,
//! and enforces per-session rate limits.

use std::collections::HashMap;
use uuid::Uuid;
use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};

// ── Provider ─────────────────────────────────────────────────────────────────

/// Supported external LLM providers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentProvider {
    OpenAI,
    Anthropic,
    Cohere,
    Mistral,
    Custom { name: String, endpoint: String },
}

impl AgentProvider {
    /// Human-readable name.
    pub fn display_name(&self) -> &str {
        match self {
            AgentProvider::OpenAI => "OpenAI",
            AgentProvider::Anthropic => "Anthropic (Claude)",
            AgentProvider::Cohere => "Cohere",
            AgentProvider::Mistral => "Mistral AI",
            AgentProvider::Custom { name, .. } => name.as_str(),
        }
    }

    /// Default model name for the provider.
    pub fn default_model(&self) -> &str {
        match self {
            AgentProvider::OpenAI => "gpt-4o",
            AgentProvider::Anthropic => "claude-3-5-sonnet-20241022",
            AgentProvider::Cohere => "command-r-plus",
            AgentProvider::Mistral => "mistral-large-latest",
            AgentProvider::Custom { .. } => "custom",
        }
    }

    /// Whether the provider is considered production-ready for Logos.
    pub fn is_supported(&self) -> bool {
        !matches!(self, AgentProvider::Custom { .. })
    }
}

// ── Session status ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Key registered, not yet trained.
    Registered,
    /// Currently running through training curriculum.
    Training,
    /// Training complete, running evaluation test suite.
    Testing,
    /// Evaluation complete — level assigned.
    Certified,
    /// Session suspended (rate limit, revoked key, etc.).
    Suspended(String),
}

impl AgentStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, AgentStatus::Registered | AgentStatus::Training | AgentStatus::Testing | AgentStatus::Certified)
    }
}

// ── AgentSession ──────────────────────────────────────────────────────────────

/// One registered agent session, owned by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// Unique session id.
    pub id: String,
    /// AI provider.
    pub provider: AgentProvider,
    /// SHA-256 hash of the API key (never stored in plaintext).
    pub key_hash: String,
    /// Last 4 chars of the API key for display.
    pub key_hint: String,
    /// Reference model to use for this session.
    pub model: String,
    /// Current lifecycle status.
    pub status: AgentStatus,
    /// Unix timestamp of session creation.
    pub created_at: u64,
    /// Unix timestamp of last activity.
    pub last_active: u64,
    /// Total requests this session has made.
    pub request_count: u64,
    /// User-defined display name for this session.
    pub label: String,
}

impl AgentSession {
    /// Create a new session, hashing the api key.
    pub fn new(
        provider: AgentProvider,
        api_key: &str,
        label: impl Into<String>,
        now_secs: u64,
    ) -> Self {
        let model = provider.default_model().to_string();
        let key_hash = hash_api_key(api_key);
        let key_hint = api_key.chars().rev().take(4).collect::<String>()
            .chars().rev().collect();
        AgentSession {
            id: Uuid::new_v4().to_string(),
            provider,
            key_hash,
            key_hint,
            model,
            status: AgentStatus::Registered,
            created_at: now_secs,
            last_active: now_secs,
            request_count: 0,
            label: label.into(),
        }
    }

    /// Touch the session (update `last_active`, increment count).
    pub fn record_request(&mut self, now_secs: u64) {
        self.last_active = now_secs;
        self.request_count += 1;
    }

    /// Verify an API key against this session's stored hash.
    pub fn verify_key(&self, api_key: &str) -> bool {
        hash_api_key(api_key) == self.key_hash
    }

    /// Age in seconds since creation.
    pub fn age_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.created_at)
    }
}

fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── Rate limiter ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RateLimitError {
    pub session_id: String,
    pub retry_after_secs: u64,
}

/// Simple token-bucket rate limiter (per session).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiter {
    /// Max tokens in the bucket.
    capacity: u32,
    /// Current tokens available.
    tokens: f64,
    /// Tokens replenished per second.
    refill_rate: f64,
    /// Unix timestamp of last refill check.
    last_check: u64,
}

impl RateLimiter {
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        RateLimiter {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_check: 0,
        }
    }

    /// Standard limits: 60 req/min burst 20.
    pub fn standard() -> Self {
        Self::new(20, 1.0) // 1 token/sec = 60/min; burst 20
    }

    /// Relaxed limits for testing: 200 req/min.
    pub fn testing() -> Self {
        Self::new(200, 3.33)
    }

    /// Try to consume one token. Returns Ok on success.
    pub fn try_acquire(&mut self, now_secs: u64) -> Result<(), u64> {
        // Refill (always, even on first call — elapsed = 0 when last_check == now_secs)
        let elapsed = now_secs.saturating_sub(self.last_check) as f64;
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity as f64);
        }
        self.last_check = now_secs;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            // How many seconds until 1 token is available
            let wait = ((1.0 - self.tokens) / self.refill_rate).ceil() as u64;
            Err(wait)
        }
    }

    /// Available tokens (floored).
    pub fn available(&self) -> u32 {
        self.tokens.floor() as u32
    }
}

// ── Session store ─────────────────────────────────────────────────────────────

/// In-memory session storage (replaceable with DB backend).
#[derive(Debug, Default)]
pub struct SessionStore {
    sessions: HashMap<String, AgentSession>,
    limiters: HashMap<String, RateLimiter>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, session: AgentSession) {
        let limiter = RateLimiter::standard();
        self.limiters.insert(session.id.clone(), limiter);
        self.sessions.insert(session.id.clone(), session);
    }

    pub fn get(&self, id: &str) -> Option<&AgentSession> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentSession> {
        self.sessions.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> Option<AgentSession> {
        self.limiters.remove(id);
        self.sessions.remove(id)
    }

    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    pub fn all(&self) -> impl Iterator<Item = &AgentSession> {
        self.sessions.values()
    }

    pub fn try_rate_limit(&mut self, id: &str, now: u64) -> Result<(), u64> {
        if let Some(limiter) = self.limiters.get_mut(id) {
            limiter.try_acquire(now)
        } else {
            Ok(()) // No limiter = no limit (shouldn't happen)
        }
    }
}

// ── Manager config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentManagerConfig {
    /// Max sessions per user.
    pub max_sessions: usize,
    /// Session idle timeout in seconds (default: 7 days).
    pub idle_timeout_secs: u64,
    /// Minimum API key length.
    pub min_key_length: usize,
}

impl Default for AgentManagerConfig {
    fn default() -> Self {
        AgentManagerConfig {
            max_sessions: 5,
            idle_timeout_secs: 7 * 24 * 3600,
            min_key_length: 20,
        }
    }
}

// ── Agent Manager ─────────────────────────────────────────────────────────────

/// Top-level manager: register agents, manage sessions, enforce limits.
#[derive(Debug)]
pub struct AgentManager {
    store: SessionStore,
    config: AgentManagerConfig,
}

impl AgentManager {
    pub fn new(config: AgentManagerConfig) -> Self {
        AgentManager {
            store: SessionStore::new(),
            config,
        }
    }

    /// Register a new agent from a user-provided API key.
    pub fn register(
        &mut self,
        provider: AgentProvider,
        api_key: &str,
        label: impl Into<String>,
        now_secs: u64,
    ) -> Result<String, crate::AgentError> {
        // Validate key length
        if api_key.len() < self.config.min_key_length {
            return Err(crate::AgentError::InvalidApiKey);
        }

        // Check session cap
        let active: usize = self.store.all()
            .filter(|s| s.status.is_active())
            .count();
        if active >= self.config.max_sessions {
            return Err(crate::AgentError::RateLimitExceeded(
                "max sessions reached".into()
            ));
        }

        let session = AgentSession::new(provider, api_key, label, now_secs);
        let id = session.id.clone();
        self.store.insert(session);
        Ok(id)
    }

    /// Revoke a session.
    pub fn revoke(&mut self, id: &str) -> Option<AgentSession> {
        self.store.remove(id)
    }

    /// Get session by id.
    pub fn get_session(&self, id: &str) -> Option<&AgentSession> {
        self.store.get(id)
    }

    /// Advance the session status.
    pub fn set_status(&mut self, id: &str, status: AgentStatus) -> bool {
        if let Some(s) = self.store.get_mut(id) {
            s.status = status;
            true
        } else {
            false
        }
    }

    /// Record a request, checking rate limits.
    pub fn record_request(&mut self, id: &str, now: u64) -> Result<(), crate::AgentError> {
        // Rate limit check
        self.store.try_rate_limit(id, now)
            .map_err(|wait| crate::AgentError::RateLimitExceeded(
                format!("retry after {}s", wait)
            ))?;

        if let Some(s) = self.store.get_mut(id) {
            s.record_request(now);
            Ok(())
        } else {
            Err(crate::AgentError::SessionNotFound(id.to_string()))
        }
    }

    /// Prune idle sessions older than the configured idle timeout.
    pub fn prune_idle(&mut self, now: u64) -> usize {
        let timeout = self.config.idle_timeout_secs;
        let expired: Vec<String> = self.store.all()
            .filter(|s| now.saturating_sub(s.last_active) > timeout)
            .map(|s| s.id.clone())
            .collect();
        let count = expired.len();
        for id in expired {
            self.store.remove(&id);
        }
        count
    }

    /// All active sessions.
    pub fn active_sessions(&self) -> Vec<&AgentSession> {
        self.store.all()
            .filter(|s| s.status.is_active())
            .collect()
    }

    /// Total session count.
    pub fn session_count(&self) -> usize {
        self.store.count()
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new(AgentManagerConfig::default())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_key(n: usize) -> String {
        "sk-test".to_string() + &"x".repeat(n)
    }

    #[test]
    fn register_session_ok() {
        let mut mgr = AgentManager::default();
        let id = mgr.register(
            AgentProvider::OpenAI,
            &make_key(30),
            "My GPT-4",
            1000,
        ).unwrap();
        assert!(!id.is_empty());
        let s = mgr.get_session(&id).unwrap();
        assert_eq!(s.status, AgentStatus::Registered);
        assert_eq!(s.provider, AgentProvider::OpenAI);
    }

    #[test]
    fn reject_short_api_key() {
        let mut mgr = AgentManager::default();
        let res = mgr.register(AgentProvider::Anthropic, "short", "x", 0);
        assert!(matches!(res, Err(crate::AgentError::InvalidApiKey)));
    }

    #[test]
    fn revoke_session() {
        let mut mgr = AgentManager::default();
        let id = mgr.register(AgentProvider::OpenAI, &make_key(30), "x", 0).unwrap();
        let revoked = mgr.revoke(&id);
        assert!(revoked.is_some());
        assert!(mgr.get_session(&id).is_none());
    }

    #[test]
    fn max_sessions_enforced() {
        let config = AgentManagerConfig { max_sessions: 2, ..Default::default() };
        let mut mgr = AgentManager::new(config);
        mgr.register(AgentProvider::OpenAI, &make_key(30), "a", 0).unwrap();
        mgr.register(AgentProvider::OpenAI, &make_key(30), "b", 0).unwrap();
        let third = mgr.register(AgentProvider::OpenAI, &make_key(30), "c", 0);
        assert!(third.is_err());
    }

    #[test]
    fn status_transitions() {
        let mut mgr = AgentManager::default();
        let id = mgr.register(AgentProvider::Anthropic, &make_key(30), "claude", 0).unwrap();
        mgr.set_status(&id, AgentStatus::Training);
        assert_eq!(mgr.get_session(&id).unwrap().status, AgentStatus::Training);
        mgr.set_status(&id, AgentStatus::Testing);
        assert_eq!(mgr.get_session(&id).unwrap().status, AgentStatus::Testing);
        mgr.set_status(&id, AgentStatus::Certified);
        assert_eq!(mgr.get_session(&id).unwrap().status, AgentStatus::Certified);
    }

    #[test]
    fn rate_limiter_burst() {
        let mut limiter = RateLimiter::new(5, 1.0);
        for _ in 0..5 {
            assert!(limiter.try_acquire(0).is_ok());
        }
        // Bucket exhausted
        assert!(limiter.try_acquire(0).is_err());
        // After 1 second, 1 token refills
        assert!(limiter.try_acquire(1).is_ok());
    }

    #[test]
    fn rate_limiter_refill_over_time() {
        let mut limiter = RateLimiter::new(10, 2.0); // 2 tokens/sec
        for _ in 0..10 {
            limiter.try_acquire(0).unwrap();
        }
        assert!(limiter.try_acquire(0).is_err());
        // After 3 seconds → 6 tokens
        for _ in 0..6 {
            assert!(limiter.try_acquire(3).is_ok());
        }
        assert!(limiter.try_acquire(3).is_err());
    }

    #[test]
    fn key_hashing_doesnt_store_plaintext() {
        let session = AgentSession::new(
            AgentProvider::OpenAI,
            "sk-secret-key-1234",
            "test",
            0,
        );
        assert!(!session.key_hash.contains("secret"));
        assert_eq!(session.key_hint.len(), 4); // last 4 chars
    }

    #[test]
    fn verify_key_roundtrip() {
        let session = AgentSession::new(
            AgentProvider::OpenAI,
            "sk-super-secret-1234",
            "test",
            0,
        );
        assert!(session.verify_key("sk-super-secret-1234"));
        assert!(!session.verify_key("sk-wrong-key-0000"));
    }

    #[test]
    fn prune_idle_sessions() {
        let mut mgr = AgentManager::new(AgentManagerConfig {
            idle_timeout_secs: 100,
            ..Default::default()
        });
        mgr.register(AgentProvider::OpenAI, &make_key(30), "a", 0).unwrap();
        mgr.register(AgentProvider::Anthropic, &make_key(30), "b", 50).unwrap();
        // At t=200: session "a" is 200s old (>100 timeout), "b" is 150s old (>100 timeout)
        let pruned = mgr.prune_idle(200);
        assert_eq!(pruned, 2);
        assert_eq!(mgr.session_count(), 0);
    }

    #[test]
    fn record_request_updates_count() {
        let mut mgr = AgentManager::default();
        let id = mgr.register(AgentProvider::Cohere, &make_key(30), "x", 0).unwrap();
        for t in 0..5u64 {
            mgr.record_request(&id, t * 2).unwrap(); // every 2 seconds, below rate limit
        }
        assert_eq!(mgr.get_session(&id).unwrap().request_count, 5);
    }

    #[test]
    fn provider_display_names() {
        assert_eq!(AgentProvider::OpenAI.display_name(), "OpenAI");
        assert_eq!(AgentProvider::Anthropic.display_name(), "Anthropic (Claude)");
        let custom = AgentProvider::Custom {
            name: "LocalLLM".into(),
            endpoint: "http://localhost:11434".into(),
        };
        assert_eq!(custom.display_name(), "LocalLLM");
    }

    #[test]
    fn default_models() {
        assert_eq!(AgentProvider::OpenAI.default_model(), "gpt-4o");
        assert_eq!(AgentProvider::Anthropic.default_model(), "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn custom_provider_not_supported() {
        let custom = AgentProvider::Custom {
            name: "LocalLLM".into(),
            endpoint: "http://localhost:11434".into(),
        };
        assert!(!custom.is_supported());
        assert!(AgentProvider::OpenAI.is_supported());
    }

    #[test]
    fn session_age() {
        let s = AgentSession::new(AgentProvider::OpenAI, &make_key(30), "x", 1000);
        assert_eq!(s.age_secs(1060), 60);
        assert_eq!(s.age_secs(999), 0); // saturating
    }

    #[test]
    fn active_sessions_filter() {
        let mut mgr = AgentManager::default();
        let id1 = mgr.register(AgentProvider::OpenAI, &make_key(30), "a", 0).unwrap();
        let id2 = mgr.register(AgentProvider::Anthropic, &make_key(30), "b", 0).unwrap();
        mgr.set_status(&id2, AgentStatus::Suspended("revoked".into()));
        assert_eq!(mgr.active_sessions().len(), 1);
        assert_eq!(mgr.active_sessions()[0].id, id1);
    }
}
