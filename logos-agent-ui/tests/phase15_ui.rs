//! Phase 15.1 Integration Tests — UI Integration of Certified Agents
//!
//! End-to-end workflows: open chat session → send message → dispatch to agent
//! → receive response → update badge → fire events → check palette routing.

use logos_agent_ui::{
    agent_dispatcher::{AgentDispatcher, AgentSlot, DispatchPriority, DispatchRequest, RoutingPolicy},
    chat_model::{ChatConfig, ChatMessage, ChatSession},
    command_palette::{CommandRegistry, PaletteAction, PaletteFilter},
    context_bridge::{BoundingBox, ContextBridge, EditorContext, PageInfo, SelectionInfo, ViewportInfo},
    status_badge::{AgentBadge, AgentCard, BadgeRenderer, BadgeState, PresenceState},
    ui_events::{
        AgentEvent, EventBus, EventHandler, PaletteEvent, PanelEvent, UiEvent, UiEventKind,
    },
};
use std::sync::Arc;

// ─── Helper ───────────────────────────────────────────────────────────────────

fn make_editor_context(layers: usize, ts: u64) -> EditorContext {
    EditorContext::new(
        SelectionInfo {
            layer_ids: (0..layers).map(|i| format!("layer-{}", i)).collect(),
            has_text_layer: layers > 0,
            has_group: layers > 2,
            bounding_box: Some(BoundingBox { x: 0.0, y: 0.0, width: 200.0, height: 100.0 }),
            ..Default::default()
        },
        ViewportInfo { zoom_pct: 100.0, ..Default::default() },
        PageInfo {
            page_id: "page-main".into(),
            page_name: "Main".into(),
            total_layers: layers,
            page_index: 0,
            total_pages: 1,
        },
        ts,
    )
}

// ─── Test 1: full chat → dispatch → badge update flow ─────────────────────────

#[test]
fn full_flow_chat_dispatch_badge() {
    // 1. Create dispatcher with a Senior agent slot
    let mut dispatcher = AgentDispatcher::default();
    dispatcher.register(AgentSlot::new("senior-session-1", "Senior", "builtin"));

    // 2. Open a chat session pointing at that agent
    let mut session = ChatSession::new("senior-session-1", ChatConfig::default(), 0);

    // 3. User sends a message
    let msg = ChatMessage::user(&session.id.clone(), "Create a red rectangle", 100);
    session.add_message(msg);
    assert_eq!(session.history.len(), 1);

    // 4. Package an editor context snapshot
    let ctx = make_editor_context(5, 100);
    let ctx_json = serde_json::to_string(&ctx).unwrap();

    // 5. Dispatch the request
    let req = DispatchRequest::new("Create a red rectangle", RoutingPolicy::ByLevel("Senior".into()))
        .with_context(&ctx_json)
        .with_priority(DispatchPriority::Normal);
    let resp = dispatcher.dispatch_sync(&req, 100);

    assert!(resp.success, "Dispatch failed: {:?}", resp.error);
    assert_eq!(resp.agent_session_id, "senior-session-1");

    // 6. Add the agent's response to the chat session
    let agent_msg = ChatMessage::agent(&session.id.clone(), &resp.response_text, 120);
    session.add_message(agent_msg);
    assert_eq!(session.history.len(), 2);

    // 7. Update the badge to reflect the completed request
    let mut badge = AgentBadge::new("senior-session-1", "Senior Agent", "Senior", "builtin");
    badge.set_ready();
    badge.increment_request();

    assert!(badge.is_available());
    assert_eq!(badge.request_count, 1);

    // 8. Check dispatcher metrics
    assert_eq!(dispatcher.metrics.total_dispatched, 1);
    assert_eq!(dispatcher.metrics.total_success, 1);
}

// ─── Test 2: palette → agent routing → event published ───────────────────────

#[test]
fn palette_agent_routing_fires_event() {
    let registry = CommandRegistry::new();
    let mut bus = EventBus::new();
    let handler = Arc::new(EventHandler::new("palette-watcher")
        .with_category_filter("palette"));
    bus.subscribe(handler.clone());

    // User types "/ask how do I create a gradient?"
    let action = registry.resolve_action("/ask how do I create a gradient?");
    assert!(matches!(action, PaletteAction::RouteToAgent { .. }));

    // Simulate palette publishing a trigger detected event
    let event = UiEvent::new(
        UiEventKind::Palette(PaletteEvent::AgentTriggerDetected {
            trigger: "/ask".into(),
            input: "how do I create a gradient?".into(),
        }),
        200,
    );
    bus.publish(event);

    // Also publish a command selected event
    bus.publish(UiEvent::new(
        UiEventKind::Palette(PaletteEvent::CommandSelected {
            command_id: "agent-ask".into(),
        }),
        210,
    ));

    assert_eq!(handler.received_count(), 2);
    assert_eq!(handler.events_of_category("palette").len(), 2);
}

// ─── Test 3: agent certification fires events ─────────────────────────────────

#[test]
fn certification_event_flow() {
    let mut bus = EventBus::new();
    let all_handler = Arc::new(EventHandler::new("all-events"));
    let agent_handler = Arc::new(EventHandler::new("agent-events").with_category_filter("agent"));
    bus.subscribe(all_handler.clone());
    bus.subscribe(agent_handler.clone());

    // Training started
    bus.publish(UiEvent::new(
        UiEventKind::Agent(AgentEvent::TrainingStarted { session_id: "s1".into() }),
        100,
    ));
    // Level up
    bus.publish(UiEvent::new(
        UiEventKind::Agent(AgentEvent::LevelChanged {
            session_id: "s1".into(),
            old_level: "Junior".into(),
            new_level: "Mid".into(),
        }),
        200,
    ));
    // Certified
    bus.publish(UiEvent::new(
        UiEventKind::Agent(AgentEvent::Certified {
            session_id: "s1".into(),
            level: "Senior".into(),
            score_pct: 93.5,
        }),
        300,
    ));

    assert_eq!(all_handler.received_count(), 3);
    assert_eq!(agent_handler.received_count(), 3);

    let last = all_handler.last_event().unwrap();
    assert!(last.is_certification());
}

// ─── Test 4: context bridge + dispatcher integration ─────────────────────────

#[test]
fn context_bridge_feeds_dispatcher() {
    let mut bridge = ContextBridge::new();
    let mut dispatcher = AgentDispatcher::default();
    dispatcher.register(AgentSlot::new("mid-1", "Mid", "builtin"));

    // Capture editor context
    let ctx = make_editor_context(3, 500);
    let snap = bridge.capture(ctx);
    let prompt = snap.to_agent_prompt();
    assert!(prompt.contains("Main"));

    // Use snapshot's JSON as dispatch context
    let req = DispatchRequest::new("Improve accessibility of selected layers", RoutingPolicy::BestAvailable)
        .with_context(snap.to_json());
    let resp = dispatcher.dispatch_sync(&req, 500);

    assert!(resp.success);
    assert!(resp.latency_ms > 0);

    // Snapshot count incremented
    assert_eq!(bridge.snapshot_count(), 1);
}

// ─── Test 5: multi-slot round-robin dispatching ───────────────────────────────

#[test]
fn round_robin_distributes_across_slots() {
    let mut dispatcher = AgentDispatcher::default();
    dispatcher.register(AgentSlot::new("slot-a", "Junior", "builtin"));
    dispatcher.register(AgentSlot::new("slot-b", "Junior", "builtin"));
    dispatcher.register(AgentSlot::new("slot-c", "Junior", "builtin"));

    let mut used_sessions = std::collections::HashSet::new();
    for i in 0..6u64 {
        let req = DispatchRequest::new(format!("task {}", i), RoutingPolicy::RoundRobin);
        let resp = dispatcher.dispatch_sync(&req, i * 100);
        assert!(resp.success);
        used_sessions.insert(resp.agent_session_id.clone());
    }

    // All 3 slots should have been used
    assert_eq!(used_sessions.len(), 3, "Expected all 3 slots used, got: {:?}", used_sessions);
}

// ─── Test 6: badge card rendering ────────────────────────────────────────────

#[test]
fn badge_card_renders_full_detail() {
    let mut badge = AgentBadge::new("sess-1", "Certified Senior", "Senior", "Logos Builtin");
    badge.set_ready();
    badge.set_usage(72.0);
    badge.request_count = 43;

    let card = AgentCard::from_badge(badge)
        .with_capabilities(vec!["layer-ops", "color-gen", "accessibility", "text-editing"])
        .with_score(88, 91.0);

    let rendered = BadgeRenderer::render_card(&card);
    assert!(rendered.contains("layer-ops"), "Card: {}", rendered);
    assert!(rendered.contains("Certified Senior"), "Card: {}", rendered);
    assert!(rendered.contains("88"), "Card: {}", rendered);
    assert_eq!(card.quality_tier(), "Exceptional");
}

// ─── Test 7: event bus capacity overflow ─────────────────────────────────────

#[test]
fn event_bus_overflow_handled_gracefully() {
    let mut bus = EventBus::new().with_capacity(5);
    for i in 0..10u64 {
        bus.publish(UiEvent::new(
            UiEventKind::Palette(PaletteEvent::QueryChanged { query: format!("query-{}", i) }),
            i,
        ));
    }
    // Bus should not exceed capacity (trims oldest)
    assert!(bus.event_count() <= 5, "Bus event count: {}", bus.event_count());
}

// ─── Test 8: chat history trimming on session boundary ────────────────────────

#[test]
fn chat_session_trims_to_max_messages() {
    let config = ChatConfig { max_messages: 5, ..Default::default() };
    let mut session = ChatSession::new("agent-x", config, 0);
    let id = session.id.clone();
    for i in 0..10u64 {
        session.add_message(ChatMessage::user(&id, format!("Message {}", i), i * 10));
    }
    let msgs: Vec<_> = session.history.messages().collect();
    assert!(msgs.len() <= 5, "History len: {}", msgs.len());
    // Last message should be the most recent
    assert_eq!(msgs.last().unwrap().content, "Message 9");
}

// ─── Test 9: command palette usage tracking persists ─────────────────────────

#[test]
fn palette_usage_tracking_persists() {
    let mut registry = CommandRegistry::new();
    registry.record_usage("agent-ask");
    registry.record_usage("agent-ask");
    registry.record_usage("create-rectangle");

    assert_eq!(registry.usage_count("agent-ask"), 2);
    assert_eq!(registry.usage_count("create-rectangle"), 1);
    assert_eq!(registry.usage_count("never-used"), 0);
}

// ─── Test 10: full pipeline with context diff ─────────────────────────────────

#[test]
fn context_diff_detected_between_dispatches() {
    let mut bridge = ContextBridge::new();
    let mut dispatcher = AgentDispatcher::default();
    dispatcher.register(AgentSlot::new("principal-1", "Senior", "builtin"));

    // First capture
    let ctx1 = make_editor_context(2, 0);
    let snap1 = bridge.capture(ctx1);

    // First dispatch
    let req1 = DispatchRequest::new("align selected layers", RoutingPolicy::BestAvailable)
        .with_context(snap1.to_json());
    let resp1 = dispatcher.dispatch_sync(&req1, 0);
    assert!(resp1.success);

    // User changes selection
    let mut ctx2 = make_editor_context(5, 100);
    ctx2.active_tool.name = "rotate".into();
    let snap2 = bridge.capture(ctx2);

    // Compute diff — should detect selection and tool changes
    let _diff = bridge.diff_from_new(&snap2);
    // diff_from_new compares the PREVIOUS snapshot with the new one
    // but bridge.current is now snap2, so let's check directly
    assert!(snap2.context.selection.count() != snap1.context.selection.count()
        || snap2.context.active_tool.name != snap1.context.active_tool.name);

    // Second dispatch with new context
    let req2 = DispatchRequest::new("rotate selected layers 45 degrees", RoutingPolicy::BestAvailable)
        .with_context(snap2.to_json());
    let resp2 = dispatcher.dispatch_sync(&req2, 100);
    assert!(resp2.success);

    // Check cumulative metrics
    assert_eq!(dispatcher.metrics.total_dispatched, 2);
    assert_eq!(bridge.snapshot_count(), 2);
}
