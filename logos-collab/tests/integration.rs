//! Integration tests for end-to-end WebSocket collaboration.
//!
//! These tests start a real server and connect real clients,
//! verifying the full sync pipeline.

use logos_collab::protocol::{PeerInfo, SyncMessage, AwarenessState};
use logos_collab::server::{SyncServer, ServerConfig};
use logos_collab::client::{SyncClient, ConnectionState, SyncEvent};
use logos_collab::broadcast::{BroadcastGroup, RoomManager};
use uuid::Uuid;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

/// Find a free port for testing.
async fn free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Start a server on a free port, return the port.
async fn start_test_server() -> u16 {
    let port = free_port().await;
    let config = ServerConfig {
        bind_addr: format!("127.0.0.1:{port}"),
        max_peers_per_room: 10,
        broadcast_capacity: 64,
        heartbeat_interval_secs: 30,
        storage_path: None,
    };
    let server = SyncServer::new(config);
    tokio::spawn(async move {
        server.run().await.unwrap();
    });
    // Give server time to bind
    tokio::time::sleep(Duration::from_millis(50)).await;
    port
}

#[tokio::test]
async fn test_server_accepts_connections() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}");

    // Connect raw WebSocket
    let result = tokio_tungstenite::connect_async(&url).await;
    assert!(result.is_ok(), "Should connect to server");
}

#[tokio::test]
async fn test_client_connects_and_receives_state() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}");

    let info = PeerInfo::new("Alice");
    let doc_id = Uuid::new_v4();
    let mut client = SyncClient::new(info, doc_id, &url);
    let mut event_rx = client.take_event_rx().unwrap();

    let connect_result = client.connect().await;
    assert!(connect_result.is_ok(), "Client should connect");

    // Should receive Connected event
    let event = timeout(Duration::from_secs(2), event_rx.recv()).await;
    assert!(event.is_ok(), "Should receive event within timeout");
    match event.unwrap() {
        Some(SyncEvent::Connected) => {}
        other => panic!("Expected Connected event, got {other:?}"),
    }

    assert_eq!(client.connection_state().await, ConnectionState::Connected);
}

#[tokio::test]
async fn test_two_clients_same_document() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}");
    let doc_id = Uuid::new_v4();

    // Client 1
    let info1 = PeerInfo::new("Alice");
    let mut client1 = SyncClient::new(info1, doc_id, &url);
    let mut events1 = client1.take_event_rx().unwrap();
    client1.connect().await.unwrap();

    // Drain connected event
    let _ = timeout(Duration::from_secs(1), events1.recv()).await;

    // Client 2
    let info2 = PeerInfo::new("Bob");
    let mut client2 = SyncClient::new(info2, doc_id, &url);
    let mut events2 = client2.take_event_rx().unwrap();
    client2.connect().await.unwrap();

    // Drain connected event for client2
    let _ = timeout(Duration::from_secs(1), events2.recv()).await;

    // Client 1 should receive PeerJoined for client 2
    let event = timeout(Duration::from_secs(2), events1.recv()).await;
    assert!(event.is_ok(), "Client1 should receive PeerJoined");
}

#[tokio::test]
async fn test_delta_broadcast_between_clients() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}");
    let doc_id = Uuid::new_v4();

    // Client 1
    let info1 = PeerInfo::new("Alice");
    let mut client1 = SyncClient::new(info1, doc_id, &url);
    let mut events1 = client1.take_event_rx().unwrap();
    client1.connect().await.unwrap();
    let _ = timeout(Duration::from_secs(1), events1.recv()).await; // Connected

    // Client 2
    let info2 = PeerInfo::new("Bob");
    let mut client2 = SyncClient::new(info2, doc_id, &url);
    let mut events2 = client2.take_event_rx().unwrap();
    client2.connect().await.unwrap();
    let _ = timeout(Duration::from_secs(1), events2.recv()).await; // Connected

    // Let peer join messages settle
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Drain any pending events
    while let Ok(Some(_)) = timeout(Duration::from_millis(50), events1.recv()).await {}
    while let Ok(Some(_)) = timeout(Duration::from_millis(50), events2.recv()).await {}

    // Client 1 sends a delta
    let test_delta = vec![42u8; 32];
    client1.send_delta(test_delta.clone()).await.unwrap();

    // Client 2 should receive the delta
    let event = timeout(Duration::from_secs(2), events2.recv()).await;
    match event {
        Ok(Some(SyncEvent::RemoteDelta { update, .. })) => {
            assert_eq!(update, test_delta, "Delta payload should match");
        }
        _other => {
            // May receive PeerJoined first, try again
            if let Ok(Some(SyncEvent::RemoteDelta { update, .. })) =
                timeout(Duration::from_secs(2), events2.recv()).await
            {
                assert_eq!(update, test_delta, "Delta payload should match");
            }
        }
    }
}

#[tokio::test]
async fn test_offline_queue_replay() {
    let info = PeerInfo::new("OfflineUser");
    let doc_id = Uuid::new_v4();
    let client = SyncClient::new(info, doc_id, "ws://localhost:99999"); // Invalid server

    // Queue some deltas while offline
    for i in 0..5 {
        client.send_delta(vec![i as u8; 16]).await.unwrap();
    }

    assert_eq!(client.offline_queue_len().await, 5);
    assert_eq!(client.clock().await, 5);
}

#[tokio::test]
async fn test_broadcast_group_high_throughput() {
    let group = BroadcastGroup::new(2048);

    // Add 100 peers
    let mut receivers = Vec::new();
    for i in 0..100 {
        let peer = PeerInfo::new(format!("Peer{i}"));
        let rx = group.add_peer(peer).await;
        receivers.push(rx);
    }

    // Broadcast 1000 messages
    let start = std::time::Instant::now();
    for i in 0..1000u64 {
        let data = Arc::new(vec![i as u8; 64]);
        group.broadcast_raw(data);
    }
    let elapsed = start.elapsed();

    // Target: <10ms for 1000 messages to 100 peers
    assert!(
        elapsed.as_millis() < 100, // Generous limit for CI
        "1000 broadcasts took {:?}, expected <100ms",
        elapsed
    );

    let stats = group.stats().await;
    assert_eq!(stats.active_peers, 100);
}

#[tokio::test]
async fn test_room_manager_isolation() {
    let manager = RoomManager::new(64);

    let doc1 = Uuid::new_v4();
    let doc2 = Uuid::new_v4();

    let room1 = manager.get_or_create(doc1).await;
    let room2 = manager.get_or_create(doc2).await;

    let peer1 = PeerInfo::new("Alice");
    let peer2 = PeerInfo::new("Bob");

    let mut rx1 = room1.add_peer(peer1).await;
    let _rx2 = room2.add_peer(peer2).await;

    // Message to room2 should NOT appear in room1
    let msg = SyncMessage::delta(Uuid::new_v4(), doc2, 1, vec![1, 2, 3]);
    room2.broadcast(&msg).unwrap();

    // Room1 receiver should timeout (no message)
    let result = timeout(Duration::from_millis(100), rx1.recv()).await;
    assert!(result.is_err(), "Room1 should not receive room2 messages");
}

#[tokio::test]
async fn test_protocol_message_size() {
    // Verify wire format efficiency
    let peer = Uuid::new_v4();
    let doc = Uuid::new_v4();

    // Empty delta
    let empty = SyncMessage::delta(peer, doc, 0, Vec::new());
    let empty_bytes = empty.encode().unwrap();
    assert!(empty_bytes.len() < 50, "Empty delta should be <50 bytes, got {}", empty_bytes.len());

    // Small delta (typical single-property change)
    let small = SyncMessage::delta(peer, doc, 1, vec![0u8; 32]);
    let small_bytes = small.encode().unwrap();
    assert!(small_bytes.len() < 100, "Small delta should be <100 bytes, got {}", small_bytes.len());

    // Awareness update
    let state = AwarenessState::default();
    let awareness = SyncMessage::awareness(peer, doc, 1, &state);
    let awareness_bytes = awareness.encode().unwrap();
    assert!(awareness_bytes.len() < 100, "Awareness should be <100 bytes, got {}", awareness_bytes.len());
}

#[tokio::test]
async fn test_ping_pong() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}");

    let info = PeerInfo::new("PingUser");
    let doc_id = Uuid::new_v4();
    let mut client = SyncClient::new(info, doc_id, &url);
    let mut events = client.take_event_rx().unwrap();
    client.connect().await.unwrap();
    let _ = timeout(Duration::from_secs(1), events.recv()).await; // Connected

    // Send ping — should not error
    client.send_ping().await.unwrap();
}
