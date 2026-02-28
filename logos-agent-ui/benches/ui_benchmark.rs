use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_agent_ui::{
    chat_model::{ChatConfig, ChatMessage, ChatSession, ConversationHistory},
    command_palette::{CommandRegistry, PaletteFilter},
    agent_dispatcher::{AgentDispatcher, AgentSlot, DispatchRequest, RoutingPolicy},
    context_bridge::{ContextBridge, EditorContext, PageInfo, SelectionInfo, ViewportInfo},
    ui_events::{AgentEvent, EventBus, EventHandler, UiEvent, UiEventKind},
};
use std::sync::Arc;

fn bench_conversation_push(c: &mut Criterion) {
    c.bench_function("conversation_push_100", |b| {
        b.iter(|| {
            let mut history = ConversationHistory::new(ChatConfig::default());
            for i in 0u64..100 {
                history.push(ChatMessage::user("session", black_box(format!("message number {}", i)), i));
            }
            black_box(history.len())
        });
    });
}

fn bench_palette_search(c: &mut Criterion) {
    let registry = CommandRegistry::new();
    c.bench_function("palette_fuzzy_search", |b| {
        b.iter(|| {
            let filter = PaletteFilter::from_query(black_box("agent create layer"));
            let results = registry.search(&filter);
            black_box(results.len())
        });
    });
}

fn bench_dispatcher_dispatch(c: &mut Criterion) {
    let mut dispatcher = AgentDispatcher::default();
    dispatcher.register(AgentSlot::new("slot-1", "Senior", "builtin"));
    dispatcher.register(AgentSlot::new("slot-2", "Mid", "builtin"));
    dispatcher.register(AgentSlot::new("slot-3", "Junior", "builtin"));

    c.bench_function("dispatcher_round_robin_3slots", |b| {
        b.iter(|| {
            let req = DispatchRequest::new(black_box("create a rectangle"), RoutingPolicy::RoundRobin);
            let resp = dispatcher.dispatch_sync(&req, 1000);
            black_box(resp.success)
        });
    });
}

fn bench_context_snapshot(c: &mut Criterion) {
    let mut bridge = ContextBridge::new();
    c.bench_function("context_snapshot_capture", |b| {
        b.iter(|| {
            let ctx = EditorContext::new(
                SelectionInfo { layer_ids: vec!["layer-1".into(), "layer-2".into()], ..Default::default() },
                ViewportInfo { zoom_pct: black_box(125.0), ..Default::default() },
                PageInfo { page_id: "p1".into(), page_name: "Main".into(), total_layers: 50, page_index: 0, total_pages: 5 },
                100,
            );
            let snap = bridge.capture(ctx);
            black_box(snap.id.len())
        });
    });
}

fn bench_event_bus_publish(c: &mut Criterion) {
    let mut bus = EventBus::new();
    for i in 0..10 {
        bus.subscribe(Arc::new(EventHandler::new(format!("handler-{}", i))));
    }

    c.bench_function("eventbus_publish_10_subscribers", |b| {
        b.iter(|| {
            let event = UiEvent::new(
                UiEventKind::Agent(AgentEvent::Certified {
                    session_id: black_box("sess-1".to_string()),
                    level: "Senior".into(),
                    score_pct: 90.0,
                }),
                1000,
            );
            bus.publish(event);
            black_box(bus.event_count())
        });
    });
}

criterion_group!(
    benches,
    bench_conversation_push,
    bench_palette_search,
    bench_dispatcher_dispatch,
    bench_context_snapshot,
    bench_event_bus_publish,
);
criterion_main!(benches);
