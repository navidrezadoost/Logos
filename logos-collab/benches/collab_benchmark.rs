use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use logos_collab::protocol::{AwarenessState, PeerInfo, SyncMessage};
use logos_collab::broadcast::BroadcastGroup;
use logos_collab::presence::{
    AwarenessMessage, CursorColor, CursorRenderData, PresenceRoom, Vec2,
    build_cursor_instances,
};
use logos_collab::storage::{
    DocumentStore, StoreConfig, CompressedDelta, WriteAheadLog, WalConfig,
};
use logos_collab::auth::{TokenEngine, Claims, RateLimiter, RateLimitConfig};
use logos_collab::auth::multilimit::{MultiLevelLimiter, MultiLimitConfig, AtomicGlobalLimiter};
use logos_collab::auth::backpressure::{BackpressureChannel, DropStrategy, AdaptiveLimiter, AtomicDropCounter};
use uuid::Uuid;
use std::sync::Arc;

fn bench_delta_encode(c: &mut Criterion) {
    let peer = Uuid::new_v4();
    let doc = Uuid::new_v4();
    let delta = vec![0u8; 64]; // Typical small delta

    c.bench_function("delta_encode_64B", |b| {
        b.iter(|| {
            let msg = SyncMessage::delta(
                black_box(peer),
                black_box(doc),
                black_box(1),
                black_box(delta.clone()),
            );
            black_box(msg.encode().unwrap());
        })
    });
}

fn bench_delta_decode(c: &mut Criterion) {
    let peer = Uuid::new_v4();
    let doc = Uuid::new_v4();
    let msg = SyncMessage::delta(peer, doc, 1, vec![0u8; 64]);
    let encoded = msg.encode().unwrap();

    c.bench_function("delta_decode_64B", |b| {
        b.iter(|| {
            black_box(SyncMessage::decode(black_box(&encoded)).unwrap());
        })
    });
}

fn bench_delta_roundtrip(c: &mut Criterion) {
    let peer = Uuid::new_v4();
    let doc = Uuid::new_v4();

    c.bench_function("delta_roundtrip_64B", |b| {
        b.iter(|| {
            let msg = SyncMessage::delta(peer, doc, 1, vec![0u8; 64]);
            let encoded = msg.encode().unwrap();
            black_box(SyncMessage::decode(&encoded).unwrap());
        })
    });
}

fn bench_awareness_encode(c: &mut Criterion) {
    let peer = Uuid::new_v4();
    let doc = Uuid::new_v4();
    let state = AwarenessState {
        cursor_x: 100.0,
        cursor_y: 200.0,
        selection: vec![Uuid::new_v4()],
        editing: None,
    };

    c.bench_function("awareness_encode", |b| {
        b.iter(|| {
            let msg = SyncMessage::awareness(
                black_box(peer),
                black_box(doc),
                black_box(1),
                black_box(&state),
            );
            black_box(msg.encode().unwrap());
        })
    });
}

fn bench_peer_info_creation(c: &mut Criterion) {
    c.bench_function("peer_info_new", |b| {
        b.iter(|| {
            black_box(PeerInfo::new(black_box("TestUser")));
        })
    });
}

fn bench_broadcast_raw(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("broadcast_raw_100_peers", |b| {
        b.iter(|| {
            rt.block_on(async {
                let group = BroadcastGroup::new(1024);

                // Add 100 peers
                let mut receivers = Vec::new();
                for i in 0..100 {
                    let peer = PeerInfo::new(format!("Peer{i}"));
                    let rx = group.add_peer(peer).await;
                    receivers.push(rx);
                }

                // Broadcast 1 message
                let data = Arc::new(vec![0u8; 64]);
                let count = group.broadcast_raw(black_box(data));
                black_box(count);
            });
        })
    });
}

fn bench_broadcast_1000_messages(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("broadcast_1000_msgs_100_peers", |b| {
        b.iter(|| {
            rt.block_on(async {
                let group = BroadcastGroup::new(2048);

                let mut receivers = Vec::new();
                for i in 0..100 {
                    let peer = PeerInfo::new(format!("Peer{i}"));
                    let rx = group.add_peer(peer).await;
                    receivers.push(rx);
                }

                // Broadcast 1000 messages
                for i in 0..1000u64 {
                    let data = Arc::new(vec![i as u8; 64]);
                    group.broadcast_raw(black_box(data));
                }
            });
        })
    });
}

fn bench_offline_queue(c: &mut Criterion) {
    use logos_collab::OfflineQueue;

    c.bench_function("offline_queue_1000_ops", |b| {
        b.iter(|| {
            let mut queue = OfflineQueue::new(10_000);
            for i in 0..1000u64 {
                queue.enqueue(i, vec![0u8; 64]);
            }
            let drained = queue.drain();
            black_box(drained);
        })
    });
}

// ─── Presence benchmarks ────────────────────────────────────────

fn bench_cursor_encode(c: &mut Criterion) {
    let msg = AwarenessMessage::Cursor {
        user_id: Uuid::new_v4(),
        position: Vec2::new(150.0, 250.0),
        timestamp: 42,
    };

    c.bench_function("cursor_msg_encode", |b| {
        b.iter(|| {
            black_box(black_box(&msg).encode().unwrap());
        })
    });
}

fn bench_cursor_decode(c: &mut Criterion) {
    let msg = AwarenessMessage::Cursor {
        user_id: Uuid::new_v4(),
        position: Vec2::new(150.0, 250.0),
        timestamp: 42,
    };
    let encoded = msg.encode().unwrap();

    c.bench_function("cursor_msg_decode", |b| {
        b.iter(|| {
            black_box(AwarenessMessage::decode(black_box(&encoded)).unwrap());
        })
    });
}

fn bench_cursor_color_from_uuid(c: &mut Criterion) {
    let id = Uuid::new_v4();

    c.bench_function("cursor_color_from_uuid", |b| {
        b.iter(|| {
            black_box(CursorColor::from_uuid(black_box(id)));
        })
    });
}

fn bench_presence_room_handle_cursor(c: &mut Criterion) {
    let local_id = Uuid::new_v4();
    let remote_id = Uuid::new_v4();

    c.bench_function("presence_room_handle_cursor", |b| {
        b.iter_custom(|iters| {
            let mut room = PresenceRoom::new(local_id);
            let join = AwarenessMessage::Join {
                user_id: remote_id,
                user_name: "Remote".into(),
                user_color: CursorColor::default(),
                device_info: None,
            };
            room.handle_message(&join);

            let start = std::time::Instant::now();
            for i in 0..iters {
                let cursor = AwarenessMessage::Cursor {
                    user_id: remote_id,
                    position: Vec2::new(i as f32, i as f32 * 0.5),
                    timestamp: i,
                };
                room.handle_message(&cursor);
            }
            start.elapsed()
        })
    });
}

fn bench_build_1000_cursor_instances(c: &mut Criterion) {
    // Prepare 1000 cursor render data entries.
    let cursors: Vec<CursorRenderData> = (0..1000)
        .map(|i| CursorRenderData {
            position: Vec2::new(i as f32 * 1.5, i as f32 * 0.8),
            color: CursorColor::from_uuid(Uuid::new_v4()),
            user_name: format!("User_{i}"),
            selection: vec![],
            user_id: Uuid::new_v4(),
        })
        .collect();

    c.bench_function("build_1000_cursor_instances", |b| {
        b.iter(|| {
            let instances = build_cursor_instances(black_box(&cursors));
            black_box(instances);
        })
    });
}

fn bench_active_cursors_1000(c: &mut Criterion) {
    c.bench_function("active_cursors_1000_peers", |b| {
        b.iter_custom(|iters| {
            let local_id = Uuid::new_v4();
            let mut room = PresenceRoom::new(local_id);

            // Add 1000 remote peers with cursor positions.
            for i in 0..1000 {
                let remote_id = Uuid::new_v4();
                let join = AwarenessMessage::Join {
                    user_id: remote_id,
                    user_name: format!("Peer_{i}"),
                    user_color: CursorColor::from_uuid(remote_id),
                    device_info: None,
                };
                room.handle_message(&join);

                let cursor = AwarenessMessage::Cursor {
                    user_id: remote_id,
                    position: Vec2::new(i as f32 * 2.0, i as f32),
                    timestamp: 1,
                };
                room.handle_message(&cursor);
            }

            let start = std::time::Instant::now();
            for _ in 0..iters {
                let active = room.active_cursors();
                black_box(active);
            }
            start.elapsed()
        })
    });
}

// ─── Storage benchmarks ─────────────────────────────────────

fn bench_store_delta(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("logos_bench_store_delta_{}", Uuid::new_v4()));
    let config = StoreConfig {
        path: dir.clone(),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();
    let delta = vec![42u8; 256]; // Typical delta payload

    c.bench_function("store_delta_256B", |b| {
        let mut version = 0u64;
        b.iter(|| {
            store.store_delta(black_box(doc_id), black_box(version), black_box(&delta)).unwrap();
            version += 1;
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
}

fn bench_load_snapshot(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("logos_bench_load_snap_{}", Uuid::new_v4()));
    let config = StoreConfig {
        path: dir.clone(),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();
    let snapshot = vec![0u8; 4096]; // 4KB snapshot
    store.save_snapshot(doc_id, &snapshot).unwrap();

    c.bench_function("load_snapshot_4KB", |b| {
        b.iter(|| {
            black_box(store.load_snapshot(black_box(doc_id)).unwrap());
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
}

fn bench_save_snapshot(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("logos_bench_save_snap_{}", Uuid::new_v4()));
    let config = StoreConfig {
        path: dir.clone(),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();
    let snapshot = vec![0u8; 4096];

    c.bench_function("save_snapshot_4KB", |b| {
        b.iter(|| {
            store.save_snapshot(black_box(doc_id), black_box(&snapshot)).unwrap();
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
}

fn bench_lz4_compress_1kb(c: &mut Criterion) {
    // Repetitive data (design-like)
    let pattern = b"RGBA(128,64,32,255) transform(1.0,0.0,0.0,1.0,100.5,200.3) ";
    let mut data = Vec::new();
    while data.len() < 1024 {
        data.extend_from_slice(pattern);
    }
    data.truncate(1024);

    c.bench_function("lz4_compress_1KB_repetitive", |b| {
        b.iter(|| {
            black_box(CompressedDelta::compress(1, black_box(&data)));
        })
    });
}

fn bench_lz4_decompress_1kb(c: &mut Criterion) {
    let pattern = b"RGBA(128,64,32,255) transform(1.0,0.0,0.0,1.0,100.5,200.3) ";
    let mut data = Vec::new();
    while data.len() < 1024 {
        data.extend_from_slice(pattern);
    }
    data.truncate(1024);
    let compressed = CompressedDelta::compress(1, &data);

    c.bench_function("lz4_decompress_1KB", |b| {
        b.iter(|| {
            black_box(black_box(&compressed).decompress().unwrap());
        })
    });
}

fn bench_wal_append(c: &mut Criterion) {
    let doc_id = Uuid::new_v4();
    let payload = vec![42u8; 64];

    c.bench_function("wal_append_64B", |b| {
        let mut wal = WriteAheadLog::new(WalConfig {
            max_buffered_entries: 1_000_000,
            flush_threshold: 100_000_000,
            ..WalConfig::default()
        });
        b.iter(|| {
            let _ = wal.append_delta(black_box(doc_id), black_box(payload.clone()));
        })
    });
}

fn bench_wal_flush_1000(c: &mut Criterion) {
    let doc_id = Uuid::new_v4();
    let payload = vec![42u8; 64];

    c.bench_function("wal_flush_1000_entries", |b| {
        b.iter(|| {
            let mut wal = WriteAheadLog::new(WalConfig {
                max_buffered_entries: 2000,
                flush_threshold: 100_000_000,
                ..WalConfig::default()
            });
            for _ in 0..1000 {
                let _ = wal.append_delta(doc_id, payload.clone());
            }
            let entries = wal.flush();
            black_box(entries);
        })
    });
}

fn bench_store_load_deltas(c: &mut Criterion) {
    let dir = std::env::temp_dir().join(format!("logos_bench_load_deltas_{}", Uuid::new_v4()));
    let config = StoreConfig {
        path: dir.clone(),
        ..StoreConfig::default()
    };
    let store = DocumentStore::open(config).unwrap();
    let doc_id = Uuid::new_v4();

    // Pre-populate with 1000 deltas
    for i in 0..1000u64 {
        store.store_delta(doc_id, i, &vec![i as u8; 128]).unwrap();
    }

    c.bench_function("load_all_deltas_1000", |b| {
        b.iter(|| {
            black_box(store.load_all_deltas(black_box(doc_id)).unwrap());
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Auth benchmarks ───────────────────────────────────────────────

fn bench_token_issue(c: &mut Criterion) {
    let engine = TokenEngine::new([42u8; 32]);
    let user_id = Uuid::new_v4();

    c.bench_function("token_issue", |b| {
        b.iter(|| {
            let claims = Claims::new(black_box(user_id), "BenchUser");
            black_box(engine.issue(&claims).unwrap());
        })
    });
}

fn bench_token_verify(c: &mut Criterion) {
    let engine = TokenEngine::new([42u8; 32]);
    let user_id = Uuid::new_v4();
    let claims = Claims::new(user_id, "BenchUser");
    let token = engine.issue(&claims).unwrap();

    c.bench_function("token_verify", |b| {
        b.iter(|| {
            black_box(engine.verify(black_box(&token)).unwrap());
        })
    });
}

fn bench_token_verify_with_docs(c: &mut Criterion) {
    let engine = TokenEngine::new([42u8; 32]);
    let user_id = Uuid::new_v4();
    let doc_id = Uuid::new_v4();
    let claims = Claims::new(user_id, "BenchUser").with_docs(vec![doc_id]);
    let token = engine.issue(&claims).unwrap();

    c.bench_function("token_verify_with_docs", |b| {
        b.iter(|| {
            black_box(engine.verify_access(black_box(&token), black_box(&doc_id)).unwrap());
        })
    });
}

fn bench_rate_limit_check(c: &mut Criterion) {
    let config = RateLimitConfig {
        messages_per_second: 100_000.0, // Very high so we don't hit the limit
        burst_capacity: 1_000_000.0,
        ..Default::default()
    };
    let mut limiter = RateLimiter::new(config);
    let user_id = Uuid::new_v4();

    c.bench_function("rate_limit_check", |b| {
        b.iter(|| {
            black_box(limiter.check_user(black_box(user_id)));
        })
    });
}

fn bench_rate_limit_check_new_user(c: &mut Criterion) {
    let config = RateLimitConfig::default();
    let mut limiter = RateLimiter::new(config);

    c.bench_function("rate_limit_check_new_user", |b| {
        b.iter(|| {
            let user_id = Uuid::new_v4();
            black_box(limiter.check_user(black_box(user_id)));
        })
    });
}

fn bench_rate_limit_bandwidth(c: &mut Criterion) {
    let config = RateLimitConfig {
        room_bytes_per_second: u64::MAX, // Very high so we don't hit the limit
        ..Default::default()
    };
    let mut limiter = RateLimiter::new(config);
    let room_id = Uuid::new_v4();

    c.bench_function("rate_limit_bandwidth_check", |b| {
        b.iter(|| {
            black_box(limiter.check_room_bandwidth(black_box(room_id), black_box(1024)));
        })
    });
}

fn bench_multi_level_check_all(c: &mut Criterion) {
    let config = MultiLimitConfig {
        user_rate: 1_000_000.0,
        user_burst: 2_000_000.0,
        room_rate: 10_000_000.0,
        room_burst: 20_000_000.0,
        global_rate: 100_000_000.0,
        global_burst: 200_000_000.0,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();

    c.bench_function("multi_level_check_all", |b| {
        b.iter(|| {
            black_box(limiter.check_all(black_box(user_id), black_box(room_id)));
        })
    });
}

fn bench_multi_level_check_with_bandwidth(c: &mut Criterion) {
    let config = MultiLimitConfig {
        user_rate: 1_000_000.0,
        user_burst: 2_000_000.0,
        room_rate: 10_000_000.0,
        room_burst: 20_000_000.0,
        global_rate: 100_000_000.0,
        global_burst: 200_000_000.0,
        room_bytes_per_sec: u64::MAX,
        ..Default::default()
    };
    let mut limiter = MultiLevelLimiter::new(config);
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();

    c.bench_function("multi_level_check_with_bandwidth", |b| {
        b.iter(|| {
            black_box(limiter.check_all_with_bandwidth(
                black_box(user_id),
                black_box(room_id),
                black_box(1024),
            ));
        })
    });
}

fn bench_atomic_global_check(c: &mut Criterion) {
    let limiter = AtomicGlobalLimiter::new(100_000_000.0, 200_000_000.0);

    c.bench_function("atomic_global_check", |b| {
        b.iter(|| {
            black_box(limiter.check());
        })
    });
}

fn bench_backpressure_send(c: &mut Criterion) {
    let mut ch = BackpressureChannel::new(10_000, DropStrategy::DropNew);

    c.bench_function("backpressure_send", |b| {
        b.iter(|| {
            black_box(ch.send(black_box(42u64)));
            ch.recv(); // Drain to avoid filling
        })
    });
}

fn bench_backpressure_send_drop_oldest(c: &mut Criterion) {
    let mut ch = BackpressureChannel::new(100, DropStrategy::DropOldest);
    // Fill it first
    for i in 0..100 {
        ch.send(i);
    }

    c.bench_function("backpressure_send_drop_oldest", |b| {
        b.iter(|| {
            black_box(ch.send(black_box(999u64)));
        })
    });
}

fn bench_adaptive_limiter_record(c: &mut Criterion) {
    let mut limiter = AdaptiveLimiter::new(100_000.0, std::time::Duration::from_millis(10));

    c.bench_function("adaptive_limiter_record", |b| {
        b.iter(|| {
            limiter.record_latency(black_box(std::time::Duration::from_nanos(500)));
        })
    });
}

fn bench_atomic_drop_counter(c: &mut Criterion) {
    let counter = AtomicDropCounter::new();

    c.bench_function("atomic_drop_counter_record", |b| {
        b.iter(|| {
            counter.record_sent();
            black_box(counter.sent());
        })
    });
}

criterion_group!(
    benches,
    bench_delta_encode,
    bench_delta_decode,
    bench_delta_roundtrip,
    bench_awareness_encode,
    bench_peer_info_creation,
    bench_broadcast_raw,
    bench_broadcast_1000_messages,
    bench_offline_queue,
    bench_cursor_encode,
    bench_cursor_decode,
    bench_cursor_color_from_uuid,
    bench_presence_room_handle_cursor,
    bench_build_1000_cursor_instances,
    bench_active_cursors_1000,
    bench_store_delta,
    bench_load_snapshot,
    bench_save_snapshot,
    bench_lz4_compress_1kb,
    bench_lz4_decompress_1kb,
    bench_wal_append,
    bench_wal_flush_1000,
    bench_store_load_deltas,
    bench_token_issue,
    bench_token_verify,
    bench_token_verify_with_docs,
    bench_rate_limit_check,
    bench_rate_limit_check_new_user,
    bench_rate_limit_bandwidth,
    bench_multi_level_check_all,
    bench_multi_level_check_with_bandwidth,
    bench_atomic_global_check,
    bench_backpressure_send,
    bench_backpressure_send_drop_oldest,
    bench_adaptive_limiter_record,
    bench_atomic_drop_counter,
);
criterion_main!(benches);
