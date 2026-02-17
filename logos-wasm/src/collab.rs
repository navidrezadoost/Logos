//! # Collaboration module — WebSocket sync for WASM
//!
//! Provides a lightweight WebSocket-based sync client for the browser
//! environment. Uses `web_sys::WebSocket` directly (no tokio dependency)
//! to connect to the Logos collaboration server.
//!
//! This module does NOT depend on `logos-collab` (which requires tokio +
//! rocksdb). Instead, it implements the same binary protocol for message
//! exchange so the server doesn't need to know whether the client is
//! native or browser-based.
//!
//! ## Architecture
//!
//! ```text
//! WasmSyncClient (browser)
//!   ├─ web_sys::WebSocket       ← browser WebSocket API
//!   ├─ JsClosure (onmessage)    ← receives binary SyncMessages
//!   ├─ JsClosure (onerror)      ← reconnect with backoff
//!   └─ offline_queue: Vec<u8>   ← queued deltas for replay
//! ```

use wasm_bindgen::prelude::*;
use uuid::Uuid;

/// Connection state for the WASM sync client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[wasm_bindgen]
pub enum WasmConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Configuration for the WASM sync client.
#[derive(Debug, Clone)]
pub struct WasmSyncConfig {
    /// WebSocket server URL (ws:// or wss://).
    pub server_url: String,
    /// Document ID.
    pub doc_id: Uuid,
    /// User display name.
    pub user_name: String,
    /// Maximum offline queue size.
    pub max_queue_size: usize,
    /// Base reconnect delay in milliseconds.
    pub reconnect_base_ms: u64,
    /// Maximum reconnect delay in milliseconds.
    pub reconnect_max_ms: u64,
}

impl Default for WasmSyncConfig {
    fn default() -> Self {
        Self {
            server_url: "ws://localhost:9090".to_string(),
            doc_id: Uuid::new_v4(),
            user_name: "Anonymous".to_string(),
            max_queue_size: 10_000,
            reconnect_base_ms: 1000,
            reconnect_max_ms: 30_000,
        }
    }
}

/// Queued delta for offline replay.
#[derive(Debug, Clone)]
pub struct QueuedDelta {
    pub clock: u64,
    pub payload: Vec<u8>,
}

/// WASM sync client state (non-WebSocket parts — compiles everywhere).
///
/// The actual WebSocket connection is established via `web_sys::WebSocket`
/// and is only available on `wasm32`. This struct holds the protocol
/// state that can be tested on any platform.
pub struct WasmSyncState {
    /// Our peer ID.
    pub peer_id: Uuid,
    /// Configuration.
    pub config: WasmSyncConfig,
    /// Current connection state.
    pub state: WasmConnectionState,
    /// Lamport clock.
    pub clock: u64,
    /// Offline queue.
    pub offline_queue: Vec<QueuedDelta>,
    /// Reconnect attempt counter (for exponential backoff).
    pub reconnect_attempts: u32,
}

impl WasmSyncState {
    /// Create a new sync state.
    pub fn new(config: WasmSyncConfig) -> Self {
        Self {
            peer_id: Uuid::new_v4(),
            config,
            state: WasmConnectionState::Disconnected,
            clock: 0,
            offline_queue: Vec::new(),
            reconnect_attempts: 0,
        }
    }

    /// Increment the Lamport clock and return the new value.
    pub fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Queue a delta for later replay.
    ///
    /// Returns `false` if the queue is full.
    pub fn queue_delta(&mut self, payload: Vec<u8>) -> bool {
        if self.offline_queue.len() >= self.config.max_queue_size {
            return false;
        }
        let clock = self.tick();
        self.offline_queue.push(QueuedDelta { clock, payload });
        true
    }

    /// Drain all queued deltas for replay on reconnection.
    pub fn drain_queue(&mut self) -> Vec<QueuedDelta> {
        std::mem::take(&mut self.offline_queue)
    }

    /// Compute the next reconnect delay with exponential backoff + jitter.
    ///
    /// Formula: min(base × 2^attempts, max) × (0.75 + random(0..0.5))
    /// This gives ±25% jitter around the computed delay.
    pub fn next_reconnect_delay_ms(&self) -> u64 {
        let base = self.config.reconnect_base_ms;
        let max = self.config.reconnect_max_ms;
        let delay = base.saturating_mul(1u64 << self.reconnect_attempts.min(20));
        let capped = delay.min(max);
        // Deterministic jitter based on peer_id + attempts (no rand dependency).
        let jitter_seed = self.peer_id.as_u128() as u64 ^ (self.reconnect_attempts as u64 * 7919);
        let jitter_frac = (jitter_seed % 500) as f64 / 1000.0; // 0.0..0.5
        let jittered = (capped as f64) * (0.75 + jitter_frac);
        jittered as u64
    }

    /// Record a successful connection.
    pub fn on_connected(&mut self) {
        self.state = WasmConnectionState::Connected;
        self.reconnect_attempts = 0;
    }

    /// Record a connection failure and prepare for retry.
    pub fn on_disconnected(&mut self) {
        self.state = WasmConnectionState::Disconnected;
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
    }

    /// Record that we are attempting to reconnect.
    pub fn on_reconnecting(&mut self) {
        self.state = WasmConnectionState::Reconnecting;
    }

    /// Get the queue size.
    pub fn queue_len(&self) -> usize {
        self.offline_queue.len()
    }

    /// Total bytes queued.
    pub fn queue_bytes(&self) -> usize {
        self.offline_queue.iter().map(|d| d.payload.len()).sum()
    }
}

// ===================================================================
// Tests
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sync_state() -> WasmSyncState {
        WasmSyncState::new(WasmSyncConfig::default())
    }

    #[test]
    fn test_creation() {
        let state = make_sync_state();
        assert_eq!(state.state, WasmConnectionState::Disconnected);
        assert_eq!(state.clock, 0);
        assert!(state.offline_queue.is_empty());
        assert_eq!(state.reconnect_attempts, 0);
    }

    #[test]
    fn test_tick() {
        let mut state = make_sync_state();
        assert_eq!(state.tick(), 1);
        assert_eq!(state.tick(), 2);
        assert_eq!(state.tick(), 3);
    }

    #[test]
    fn test_queue_delta() {
        let mut state = make_sync_state();
        assert!(state.queue_delta(vec![1, 2, 3]));
        assert!(state.queue_delta(vec![4, 5]));
        assert_eq!(state.queue_len(), 2);
        assert_eq!(state.queue_bytes(), 5);
    }

    #[test]
    fn test_queue_capacity() {
        let mut state = WasmSyncState::new(WasmSyncConfig {
            max_queue_size: 3,
            ..Default::default()
        });
        assert!(state.queue_delta(vec![1]));
        assert!(state.queue_delta(vec![2]));
        assert!(state.queue_delta(vec![3]));
        assert!(!state.queue_delta(vec![4])); // Full.
        assert_eq!(state.queue_len(), 3);
    }

    #[test]
    fn test_drain_queue() {
        let mut state = make_sync_state();
        state.queue_delta(vec![1, 2]);
        state.queue_delta(vec![3, 4]);
        let drained = state.drain_queue();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].clock, 1);
        assert_eq!(drained[1].clock, 2);
        assert!(state.offline_queue.is_empty());
    }

    #[test]
    fn test_reconnect_delay_exponential() {
        let mut state = make_sync_state();

        let d0 = state.next_reconnect_delay_ms();
        assert!(d0 >= 750 && d0 <= 1250, "first delay should be ~1000ms, got {d0}");

        state.reconnect_attempts = 1;
        let d1 = state.next_reconnect_delay_ms();
        assert!(d1 >= 1500 && d1 <= 2500, "second delay should be ~2000ms, got {d1}");

        state.reconnect_attempts = 2;
        let d2 = state.next_reconnect_delay_ms();
        assert!(d2 >= 3000 && d2 <= 5000, "third delay should be ~4000ms, got {d2}");
    }

    #[test]
    fn test_reconnect_delay_capped() {
        let mut state = WasmSyncState::new(WasmSyncConfig {
            reconnect_base_ms: 1000,
            reconnect_max_ms: 30_000,
            ..Default::default()
        });
        state.reconnect_attempts = 30; // 2^30 * 1000 >> 30_000.
        let d = state.next_reconnect_delay_ms();
        // Should be capped at max * jitter.
        assert!(d <= 37_500, "delay should be capped, got {d}");
    }

    #[test]
    fn test_connection_state_transitions() {
        let mut state = make_sync_state();
        assert_eq!(state.state, WasmConnectionState::Disconnected);

        state.on_reconnecting();
        assert_eq!(state.state, WasmConnectionState::Reconnecting);

        state.on_connected();
        assert_eq!(state.state, WasmConnectionState::Connected);
        assert_eq!(state.reconnect_attempts, 0);

        state.on_disconnected();
        assert_eq!(state.state, WasmConnectionState::Disconnected);
        assert_eq!(state.reconnect_attempts, 1);
    }

    #[test]
    fn test_reconnect_attempts_saturate() {
        let mut state = make_sync_state();
        for _ in 0..100 {
            state.on_disconnected();
        }
        // Should not overflow.
        assert!(state.reconnect_attempts <= 100);
    }

    #[test]
    fn test_config_defaults() {
        let config = WasmSyncConfig::default();
        assert_eq!(config.server_url, "ws://localhost:9090");
        assert_eq!(config.user_name, "Anonymous");
        assert_eq!(config.max_queue_size, 10_000);
        assert_eq!(config.reconnect_base_ms, 1000);
        assert_eq!(config.reconnect_max_ms, 30_000);
    }

    #[test]
    fn test_on_connected_resets_attempts() {
        let mut state = make_sync_state();
        state.on_disconnected();
        state.on_disconnected();
        state.on_disconnected();
        assert_eq!(state.reconnect_attempts, 3);

        state.on_connected();
        assert_eq!(state.reconnect_attempts, 0);
    }
}
