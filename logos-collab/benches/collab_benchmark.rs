use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_collab::protocol::{AwarenessState, PeerInfo, SyncMessage};
use logos_collab::broadcast::BroadcastGroup;
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
);
criterion_main!(benches);
