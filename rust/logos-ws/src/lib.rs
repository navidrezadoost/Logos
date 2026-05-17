// logos-ws — Phase 3 placeholder
//
// WebSocket fan-out primitives, successor to P2.4 (horizontal WS scaling).
// Will expose a Tokio-based connection manager that replaces the Redis PubSub
// bounce for in-process fan-out when all replicas share the Rust binary.
//
// Expected API (to be implemented):
//   pub struct WsHub { … }
//   impl WsHub {
//       pub fn subscribe(&self, file_id: Uuid, page_id: Option<Uuid>) -> Receiver<Message>;
//       pub fn publish(&self, topic: Topic, msg: Message);
//   }

/// Placeholder — not yet implemented.
pub fn placeholder() {}
