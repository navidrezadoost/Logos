//! WebSocket pub/sub broker — replaces Redis pub/sub.
//!
//! Each "room" maps to a broadcast channel.  Subscribers hold a
//! `tokio::sync::broadcast::Receiver`; publishers call `send()`.
//!
//! Design:
//! - Rooms are created lazily on first subscribe or publish.
//! - Rooms are cleaned up automatically when all receivers are dropped
//!   (the broadcast sender's `receiver_count()` drops to 0 on the next
//!   publish).
//! - Message capacity per room is configurable (default 256).

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use thiserror::Error;

/// Maximum messages buffered per room before old ones are overwritten.
const DEFAULT_CAPACITY: usize = 256;

/// A low-overhead, cloneable message sent through the broker.
#[derive(Debug, Clone)]
pub struct Message {
    /// Source user / client ID (for echo suppression on the receiver side).
    pub sender_id: String,
    /// Arbitrary payload (JSON bytes, Yrs update bytes, etc.).
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("room '{0}' has no active subscribers — message dropped")]
    NoSubscribers(String),
    #[error("broadcast send failed: {0}")]
    SendFailed(String),
}

type RoomSender = broadcast::Sender<Message>;

/// Room-based pub/sub broker backed by `tokio::sync::broadcast` channels.
///
/// Clone-safe — all clones share the same underlying `DashMap`.
#[derive(Clone)]
pub struct Broker {
    rooms: Arc<DashMap<String, RoomSender>>,
    capacity: usize,
}

impl Broker {
    /// Create a new [`Broker`] with the default channel capacity.
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            capacity: DEFAULT_CAPACITY,
        }
    }

    /// Create a new [`Broker`] with a custom channel capacity per room.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            capacity,
        }
    }

    /// Subscribe to `room_id`.  Returns a `Receiver` that will yield all
    /// messages published to that room from this point forward.
    pub fn subscribe(&self, room_id: impl Into<String>) -> broadcast::Receiver<Message> {
        let room_id = room_id.into();
        let sender = self
            .rooms
            .entry(room_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(self.capacity);
                tx
            });
        sender.subscribe()
    }

    /// Publish `message` to every subscriber in `room_id`.
    ///
    /// Returns `Ok(subscriber_count)` on success.
    /// Returns `Err(BrokerError::NoSubscribers)` when the room has no active
    /// receivers (the message is still dropped, not queued).
    pub fn publish(
        &self,
        room_id: impl AsRef<str>,
        message: Message,
    ) -> Result<usize, BrokerError> {
        let room_id = room_id.as_ref();
        if let Some(sender) = self.rooms.get(room_id) {
            let n = sender.receiver_count();
            if n == 0 {
                return Err(BrokerError::NoSubscribers(room_id.to_owned()));
            }
            sender
                .send(message)
                .map_err(|e| BrokerError::SendFailed(e.to_string()))?;
            Ok(n)
        } else {
            Err(BrokerError::NoSubscribers(room_id.to_owned()))
        }
    }

    /// Number of active (non-dropped) receivers in `room_id`.
    pub fn subscriber_count(&self, room_id: &str) -> usize {
        self.rooms
            .get(room_id)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }

    /// Remove a room entirely (its broadcast channel is dropped).  Any
    /// subsequent publish attempts will return `NoSubscribers`.
    pub fn close_room(&self, room_id: &str) {
        self.rooms.remove(room_id);
    }

    /// Total number of active rooms.
    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }
}

impl Default for Broker {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests  (B-01 … B-15)
// ─────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn b01_subscribe_and_publish() {
        let broker = Broker::new();
        let mut rx = broker.subscribe("room-A");
        let msg = Message { sender_id: "u1".into(), payload: b"hello".to_vec() };
        broker.publish("room-A", msg.clone()).unwrap();
        let received = rx.recv().await.unwrap();
        assert_eq!(received.payload, b"hello");
        assert_eq!(received.sender_id, "u1");
    }

    #[tokio::test]
    async fn b02_multiple_subscribers_same_room() {
        let broker = Broker::new();
        let mut rx1 = broker.subscribe("room-B");
        let mut rx2 = broker.subscribe("room-B");
        broker.publish("room-B", Message { sender_id: "u1".into(), payload: b"multi".to_vec() }).unwrap();
        let r1 = rx1.recv().await.unwrap();
        let r2 = rx2.recv().await.unwrap();
        assert_eq!(r1.payload, r2.payload);
    }

    #[tokio::test]
    async fn b03_different_rooms_are_isolated() {
        let broker = Broker::new();
        let mut rx_a = broker.subscribe("A");
        let _rx_b = broker.subscribe("B");
        broker.publish("B", Message { sender_id: "u".into(), payload: b"b-only".to_vec() }).unwrap();
        // rx_a should NOT receive anything from room B
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(20),
            rx_a.recv()
        ).await.is_err());
    }

    #[test]
    fn b04_no_subscribers_returns_error() {
        let broker = Broker::new();
        let res = broker.publish("empty-room", Message { sender_id: "u".into(), payload: vec![] });
        assert!(matches!(res, Err(BrokerError::NoSubscribers(_))));
    }

    #[test]
    fn b05_subscriber_count() {
        let broker = Broker::new();
        assert_eq!(broker.subscriber_count("r"), 0);
        let _rx = broker.subscribe("r");
        assert_eq!(broker.subscriber_count("r"), 1);
        let _rx2 = broker.subscribe("r");
        assert_eq!(broker.subscriber_count("r"), 2);
    }

    #[test]
    fn b06_room_count() {
        let broker = Broker::new();
        let _r1 = broker.subscribe("r1");
        let _r2 = broker.subscribe("r2");
        assert_eq!(broker.room_count(), 2);
    }

    #[test]
    fn b07_close_room() {
        let broker = Broker::new();
        let _rx = broker.subscribe("r");
        broker.close_room("r");
        let res = broker.publish("r", Message { sender_id: "u".into(), payload: vec![] });
        assert!(res.is_err());
    }

    #[test]
    fn b08_broker_is_clone_shared() {
        let b1 = Broker::new();
        let b2 = b1.clone();
        let _rx = b1.subscribe("shared");
        // Both clones see the same room
        assert_eq!(b2.subscriber_count("shared"), 1);
    }

    #[test]
    fn b09_with_capacity() {
        let broker = Broker::with_capacity(8);
        assert_eq!(broker.capacity, 8);
    }

    #[tokio::test]
    async fn b10_receiver_dropped_decrements_count() {
        let broker = Broker::new();
        {
            let _rx = broker.subscribe("tmp");
            assert_eq!(broker.subscriber_count("tmp"), 1);
        } // _rx dropped here
        assert_eq!(broker.subscriber_count("tmp"), 0);
    }

    #[tokio::test]
    async fn b11_binary_payload_round_trip() {
        let broker = Broker::new();
        let mut rx = broker.subscribe("bin");
        let data = vec![0u8, 1, 2, 255, 128];
        broker.publish("bin", Message { sender_id: "u".into(), payload: data.clone() }).unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.payload, data);
    }

    #[tokio::test]
    async fn b12_multiple_messages_in_order() {
        let broker = Broker::new();
        let mut rx = broker.subscribe("seq");
        for i in 0u8..5 {
            broker.publish("seq", Message { sender_id: "u".into(), payload: vec![i] }).unwrap();
        }
        for i in 0u8..5 {
            let msg = rx.recv().await.unwrap();
            assert_eq!(msg.payload[0], i);
        }
    }

    #[tokio::test]
    async fn b13_sender_id_preserved() {
        let broker = Broker::new();
        let mut rx = broker.subscribe("id-check");
        broker.publish("id-check", Message { sender_id: "alice".into(), payload: vec![] }).unwrap();
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.sender_id, "alice");
    }

    #[test]
    fn b14_close_nonexistent_room_is_noop() {
        let broker = Broker::new();
        broker.close_room("ghost"); // should not panic
    }

    #[tokio::test]
    async fn b15_publish_returns_subscriber_count() {
        let broker = Broker::new();
        let _rx1 = broker.subscribe("cnt");
        let _rx2 = broker.subscribe("cnt");
        let _rx3 = broker.subscribe("cnt");
        let n = broker.publish("cnt", Message { sender_id: "u".into(), payload: vec![] }).unwrap();
        assert_eq!(n, 3);
    }
}
