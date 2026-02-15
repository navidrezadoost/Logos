//! Benchmarks for the plugin system.
//!
//! Measures:
//! - Sandbox creation (<1ms target)
//! - JsEngine creation (<5ms cold target)
//! - Script evaluation (<5ms sandbox / <10ms JS cold target)
//! - Host function calls (<500ns target)
//! - Permission checks (<50ns target)
//! - JS host API calls via Logos.* (<500ns target)
//! - Path creation (<10μs target) [Day 20]
//! - Selection operations (<10μs target) [Day 20]
//! - Undo/Redo (<10μs target) [Day 20]
//! - Event dispatch (<10μs target) [Day 20]
//! - UI panel create (<1μs target) [Day 21]
//! - UI message serialize (<500ns target) [Day 21]
//! - UI message roundtrip (<1μs target) [Day 21]
//! - UI bridge dispatch (<1μs target) [Day 21]
//! - UI permission check (<50ns target) [Day 21]
//! - UI createPanel via JS (<500μs target) [Day 21]
//! - Manifest parse (<10μs target) [Day 22]
//! - Signature verify (<1μs target) [Day 22]
//! - Package create (<5ms target) [Day 22]
//! - Package verify (<1ms target) [Day 22]
//! - Registry lookup (<1μs target) [Day 22]
//! - Registry install (<5ms target) [Day 22]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_core::{Document, Layer, PathCommand, PathLayer, Point, RectLayer};
use logos_plugins::engine::{JsEngine, UiBridge};
use logos_plugins::engine::ui::{
    DockPosition, UiMessage, UiPermission, UiPermissionSet, UiValue,
};
use logos_plugins::host::PluginHost;
use logos_plugins::manager::PluginManager;
use logos_plugins::manifest::{PluginManifest, PluginCategory};
use logos_plugins::marketplace::{
    MarketplaceClient, MarketplaceSearch, PackageBuilder, PublisherInfo, SortOrder,
    TrustLevel, TrustedPublishers,
};
use logos_plugins::packaging::{PluginPackage, IconSize};
use logos_plugins::permissions::{PermissionGuard, PermissionKind, PermissionSet};
use logos_plugins::registry::{PluginFilter, PluginRegistry, RegistrySource};
use logos_plugins::runtime::{PluginValue, ResourceLimits, Sandbox};
use logos_plugins::signing::{ContentHash, PluginKeyPair, SigningContext};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Generous limits for benchmarks — avoids TimeLimitExceeded panics
/// under Criterion's warm-up / measurement contention.
fn bench_limits() -> ResourceLimits {
    let mut l = ResourceLimits::default();
    l.max_execution_time = Duration::from_secs(5);
    l
}

fn bench_sandbox_create(c: &mut Criterion) {
    c.bench_function("sandbox_create", |b| {
        b.iter(|| {
            let s = Sandbox::new(black_box("bench-plugin"), ResourceLimits::default());
            black_box(s);
        });
    });
}

fn bench_js_engine_create(c: &mut Criterion) {
    c.bench_function("js_engine_create_cold", |b| {
        b.iter(|| {
            let engine = JsEngine::new(
                black_box("bench"),
                ResourceLimits::default(),
                PermissionSet::none(),
            );
            black_box(engine);
        });
    });
}

fn bench_eval_literal(c: &mut Criterion) {
    let mut sandbox = Sandbox::new("bench", ResourceLimits::default());
    c.bench_function("eval_literal_int", |b| {
        b.iter(|| {
            sandbox.execute(black_box("42")).unwrap();
        });
    });

    c.bench_function("eval_literal_string", |b| {
        b.iter(|| {
            sandbox.execute(black_box("\"hello world\"")).unwrap();
        });
    });
}

fn bench_js_eval(c: &mut Criterion) {
    let mut engine = JsEngine::new("bench", bench_limits(), PermissionSet::none());
    c.bench_function("js_eval_literal_int", |b| {
        b.iter(|| {
            engine.execute(black_box("42")).unwrap();
        });
    });

    c.bench_function("js_eval_arithmetic", |b| {
        b.iter(|| {
            engine.execute(black_box("2 + 3 * 4")).unwrap();
        });
    });

    c.bench_function("js_eval_string_concat", |b| {
        b.iter(|| {
            engine.execute(black_box("'hello' + ' ' + 'world'")).unwrap();
        });
    });

    c.bench_function("js_eval_arrow_fn", |b| {
        b.iter(|| {
            engine
                .execute(black_box("((x) => x * 2)(21)"))
                .unwrap();
        });
    });
}

fn bench_eval_host_fn(c: &mut Criterion) {
    let mut sandbox = Sandbox::new("bench", ResourceLimits::default());
    sandbox.register_host_fn(
        "add",
        |args: &[PluginValue]| {
            let a = args[0].as_int().unwrap_or(0);
            let b = args[1].as_int().unwrap_or(0);
            Ok(PluginValue::Int(a + b))
        },
    );

    c.bench_function("host_fn_call", |b| {
        b.iter(|| {
            sandbox.execute(black_box("host.add(1, 2)")).unwrap();
        });
    });
}

fn bench_js_host_api(c: &mut Criterion) {
    let doc = Document::new();
    for _ in 0..10 {
        let _ = doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0)));
    }
    let doc = Arc::new(RwLock::new(doc));

    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    c.bench_function("js_logos_getLayerCount", |b| {
        b.iter(|| {
            engine
                .execute(black_box("Logos.getLayerCount()"))
                .unwrap();
        });
    });

    c.bench_function("js_logos_getLayers_10", |b| {
        b.iter(|| {
            engine.execute(black_box("Logos.getLayers()")).unwrap();
        });
    });

    c.bench_function("js_logos_getDocumentInfo", |b| {
        b.iter(|| {
            engine
                .execute(black_box("Logos.getDocumentInfo()"))
                .unwrap();
        });
    });
}

fn bench_js_create_rect(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    c.bench_function("js_logos_createRect", |b| {
        b.iter(|| {
            engine
                .execute(black_box("Logos.createRect(10, 20, 100, 50)"))
                .unwrap();
        });
    });
}

fn bench_permission_check(c: &mut Criterion) {
    let perms = PermissionSet::document_full();
    let mut guard = PermissionGuard::new(perms);

    c.bench_function("permission_check_granted", |b| {
        b.iter(|| {
            guard
                .check(black_box(&PermissionKind::DocumentRead))
                .unwrap();
        });
    });

    let mut guard2 = PermissionGuard::new(PermissionSet::none());
    c.bench_function("permission_check_denied", |b| {
        b.iter(|| {
            let _ = guard2.check(black_box(&PermissionKind::DocumentRead));
        });
    });
}

fn bench_domain_check(c: &mut Criterion) {
    let mut perms = PermissionSet::none();
    perms.grant(PermissionKind::Network);
    perms.allow_domain("api.logos.dev");
    perms.allow_domain("cdn.logos.dev");

    c.bench_function("domain_check_allowed", |b| {
        b.iter(|| {
            perms.is_domain_allowed(black_box("api.logos.dev"));
        });
    });

    c.bench_function("domain_check_denied", |b| {
        b.iter(|| {
            perms.is_domain_allowed(black_box("evil.com"));
        });
    });
}

fn bench_host_get_layers(c: &mut Criterion) {
    let doc = Document::new();
    for _ in 0..10 {
        let _ = doc.add_layer(Layer::Rect(RectLayer::new(0.0, 0.0, 100.0, 100.0)));
    }
    let doc = Arc::new(RwLock::new(doc));
    let host = PluginHost::new(Arc::clone(&doc), PermissionSet::read_only());
    let mut sandbox = Sandbox::new("bench", ResourceLimits::default());
    host.register_host_fns(&mut sandbox);

    c.bench_function("host_get_layers_10", |b| {
        b.iter(|| {
            sandbox.execute(black_box("host.get_layers()")).unwrap();
        });
    });

    c.bench_function("host_get_layer_count", |b| {
        b.iter(|| {
            sandbox
                .execute(black_box("host.get_layer_count()"))
                .unwrap();
        });
    });
}

fn bench_host_create_rect(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let host = PluginHost::new(Arc::clone(&doc), PermissionSet::document_full());
    let mut sandbox = Sandbox::new("bench", ResourceLimits::default());
    host.register_host_fns(&mut sandbox);

    c.bench_function("host_create_rect", |b| {
        b.iter(|| {
            sandbox
                .execute(black_box("host.create_rect(10, 20, 100, 50)"))
                .unwrap();
        });
    });
}

fn bench_manager_load(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));

    c.bench_function("manager_load_js_plugin", |b| {
        b.iter(|| {
            let mut mgr = PluginManager::new(Arc::clone(&doc));
            let manifest = PluginManifest::new("bench-plugin")
                .with_entry_point("bench.js")
                .with_permissions(PermissionSet::read_only());
            mgr.load(black_box(manifest)).unwrap();
        });
    });
}

fn bench_plugin_value_json(c: &mut Criterion) {
    let value = PluginValue::Object({
        let mut map = std::collections::HashMap::new();
        map.insert("id".to_string(), PluginValue::String("abc-123".to_string()));
        map.insert("count".to_string(), PluginValue::Int(42));
        map.insert("active".to_string(), PluginValue::Bool(true));
        map
    });

    c.bench_function("plugin_value_to_json", |b| {
        b.iter(|| {
            black_box(value.to_json());
        });
    });
}

// ═══════════ Day 20: Path, Selection, Undo, Event Benchmarks ═══════════

fn bench_js_create_path(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    let code = r#"Logos.createPath([
        { type: "moveTo", x: 0, y: 0 },
        { type: "bezierTo", cp1x: 50, cp1y: -50, cp2x: 150, cp2y: -50, x: 200, y: 0 },
        { type: "lineTo", x: 200, y: 100 },
        { type: "close" }
    ])"#;

    c.bench_function("js_logos_createPath_bezier", |b| {
        b.iter(|| {
            engine.execute(black_box(code)).unwrap();
        });
    });
}

fn bench_js_selection(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    // Pre-create some rects to select
    engine.execute(r#"
        var ids = [];
        for (var i = 0; i < 5; i++) {
            ids.push(Logos.createRect(i*10, i*10, 50, 50));
        }
    "#).unwrap();

    c.bench_function("js_logos_getSelection", |b| {
        b.iter(|| {
            engine.execute(black_box("Logos.getSelection()")).unwrap();
        });
    });

    c.bench_function("js_logos_setSelection", |b| {
        b.iter(|| {
            engine.execute(black_box("Logos.setSelection(ids)")).unwrap();
        });
    });

    c.bench_function("js_logos_clearSelection", |b| {
        b.iter(|| {
            engine.execute(black_box("Logos.clearSelection()")).unwrap();
        });
    });
}

fn bench_js_undo_redo(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    c.bench_function("js_logos_undo_redo_cycle", |b| {
        b.iter(|| {
            engine.execute(black_box("Logos.createRect(0,0,50,50)")).unwrap();
            engine.execute(black_box("Logos.undo()")).unwrap();
        });
    });
}

fn bench_js_event_on(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    c.bench_function("js_logos_on_register", |b| {
        b.iter(|| {
            engine.execute(black_box(
                r#"Logos.on("layerAdded", function(e) {})"#
            )).unwrap();
        });
    });
}

fn bench_event_flush(c: &mut Criterion) {
    let doc = Arc::new(RwLock::new(Document::new()));
    let mut engine = JsEngine::new(
        "bench",
        bench_limits(),
        PermissionSet::document_full(),
    );
    engine.register_document(Arc::clone(&doc));

    // Register a callback
    engine.execute(r#"
        Logos.on("layerAdded", function(e) {
            globalThis.__benchCount = (globalThis.__benchCount || 0) + 1;
        });
    "#).unwrap();

    c.bench_function("event_flush_with_callback", |b| {
        b.iter(|| {
            // Create a rect to queue a layerAdded event
            engine.execute("Logos.createRect(0,0,10,10)").unwrap();
            // Flush the event queue (rate limited — first call dispatches, subsequent may skip)
            black_box(engine.flush_events());
        });
    });
}

fn bench_path_layer_core(c: &mut Criterion) {
    c.bench_function("core_path_layer_create", |b| {
        b.iter(|| {
            let path = PathLayer::new(vec![
                PathCommand::MoveTo(Point::new(0.0, 0.0)),
                PathCommand::BezierTo {
                    cp1: Point::new(50.0, -50.0),
                    cp2: Point::new(150.0, -50.0),
                    end: Point::new(200.0, 0.0),
                },
                PathCommand::LineTo(Point::new(200.0, 100.0)),
                PathCommand::Close,
            ]);
            black_box(path);
        });
    });
}

// ═══════════════════════════════════════════════════════════════
// Day 21 — UI system benchmarks
// ═══════════════════════════════════════════════════════════════

fn bench_ui_panel_create(c: &mut Criterion) {
    c.bench_function("ui_panel_create", |b| {
        b.iter(|| {
            let mut bridge = UiBridge::new();
            let pid = uuid::Uuid::new_v4();
            bridge.set_permissions(pid, UiPermissionSet::render_only());
            let id = bridge.create_panel(pid, "Bench Panel", DockPosition::Right).unwrap();
            black_box(id);
        });
    });
}

fn bench_ui_message_serialize(c: &mut Criterion) {
    c.bench_function("ui_message_serialize", |b| {
        let msg = UiMessage::ValueChanged {
            key: "opacity".to_string(),
            value: UiValue::Number(0.75),
        };
        b.iter(|| {
            let json = black_box(&msg).to_json();
            black_box(json);
        });
    });
}

fn bench_ui_message_roundtrip(c: &mut Criterion) {
    use std::collections::HashMap;
    c.bench_function("ui_message_roundtrip", |b| {
        let msg = UiMessage::Custom {
            kind: "propertyUpdate".to_string(),
            data: {
                let mut m = HashMap::new();
                m.insert("x".to_string(), UiValue::Number(100.0));
                m.insert("y".to_string(), UiValue::Number(200.0));
                m.insert("name".to_string(), UiValue::String("Layer 1".to_string()));
                m
            },
        };
        b.iter(|| {
            let json = msg.to_json();
            let json_str = json.unwrap();
            let parsed = UiMessage::from_json(&json_str);
            black_box(parsed);
        });
    });
}

fn bench_ui_bridge_dispatch(c: &mut Criterion) {
    c.bench_function("ui_bridge_dispatch", |b| {
        let mut bridge = UiBridge::new();
        let pid = uuid::Uuid::new_v4();
        bridge.set_permissions(pid, UiPermissionSet::full());
        let panel_id = bridge.create_panel(pid, "Dispatch Panel", DockPosition::Right).unwrap();

        b.iter(|| {
            let msg = UiMessage::UpdateValue {
                key: "x".to_string(),
                value: UiValue::Number(42.0),
            };
            bridge.send_to_panel(pid, panel_id, msg).unwrap();
            let drained = bridge.drain_outbox();
            black_box(drained);
        });
    });
}

fn bench_ui_permission_check(c: &mut Criterion) {
    c.bench_function("ui_permission_check", |b| {
        let perms = UiPermissionSet::full();
        b.iter(|| {
            let ok = perms.has(UiPermission::Render)
                && perms.has(UiPermission::ReadDocument)
                && perms.has(UiPermission::WriteDocument)
                && perms.has(UiPermission::Network);
            black_box(ok);
        });
    });
}

fn bench_ui_create_panel_js(c: &mut Criterion) {
    c.bench_function("js_ui_create_panel", |b| {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = JsEngine::new("bench-ui", bench_limits(), PermissionSet::document_full());
        engine.register_document(doc);

        b.iter(|| {
            let result = engine.execute(r#"
                var panelId = Logos.ui.createPanel("Bench", "right");
                panelId;
            "#);
            let _ = black_box(result);
        });
    });
}

// ─── Day 22: Signing, Packaging, Registry Benchmarks ───

fn bench_manifest_parse(c: &mut Criterion) {
    let manifest = PluginManifest::new("Bench Plugin")
        .with_version(1, 0, 0)
        .with_author("Bench Author")
        .with_entry_point("main.js")
        .with_category(PluginCategory::Layout)
        .with_description("A plugin for benchmarking")
        .with_tag("bench")
        .with_permissions(PermissionSet::document_full());

    let json = serde_json::to_string(&manifest).unwrap();

    c.bench_function("manifest_parse", |b| {
        b.iter(|| {
            let parsed: PluginManifest =
                serde_json::from_str(black_box(&json)).unwrap();
            black_box(parsed);
        });
    });
}

fn bench_signature_verify(c: &mut Criterion) {
    let kp = PluginKeyPair::generate();
    let data = b"plugin code bundle for benchmark verification";
    let hash = ContentHash::compute(data);
    let sig = kp.sign(&hash);

    c.bench_function("signature_verify", |b| {
        b.iter(|| {
            let result = black_box(&sig).verify(black_box(&hash));
            black_box(result).unwrap();
        });
    });
}

fn bench_package_create(c: &mut Criterion) {
    let manifest = PluginManifest::new("Packaged Plugin")
        .with_version(1, 0, 0)
        .with_entry_point("main.js")
        .with_permissions(PermissionSet::read_only());
    let code = "console.log('hello');".repeat(50).into_bytes();

    c.bench_function("package_create", |b| {
        b.iter(|| {
            let pkg = PluginPackage::create(
                black_box(&manifest),
                black_box(&code),
            ).unwrap();
            let bytes = pkg.to_bytes().unwrap();
            black_box(bytes);
        });
    });
}

fn bench_package_verify(c: &mut Criterion) {
    let manifest = PluginManifest::new("Verified Plugin")
        .with_version(1, 0, 0)
        .with_entry_point("main.js")
        .with_permissions(PermissionSet::read_only());
    let code = b"console.log('hello from verified plugin');";
    let mut pkg = PluginPackage::create(&manifest, code).unwrap();
    let kp = PluginKeyPair::generate();
    pkg.sign(&kp);
    let bytes = pkg.to_bytes().unwrap();

    c.bench_function("package_verify", |b| {
        b.iter(|| {
            let parsed = PluginPackage::from_bytes(black_box(&bytes)).unwrap();
            parsed.verify_signature().unwrap();
            parsed.verify_integrity().unwrap();
            black_box(parsed);
        });
    });
}

fn bench_registry_lookup(c: &mut Criterion) {
    let mut reg = PluginRegistry::new();
    // Pre-populate with 50 plugins
    let mut ids = Vec::new();
    for i in 0..50 {
        let manifest = PluginManifest::new(format!("Plugin {i}"))
            .with_version(1, 0, 0)
            .with_entry_point("main.js")
            .with_permissions(PermissionSet::read_only());
        let code = format!("console.log('Plugin {i}');");
        let pkg = PluginPackage::create(&manifest, code.as_bytes()).unwrap();
        let id = manifest.id.to_string();
        ids.push(id);
        reg.install(&pkg, RegistrySource::Local).unwrap();
    }

    let lookup_id = &ids[25]; // middle of the registry

    c.bench_function("registry_lookup", |b| {
        b.iter(|| {
            let result = reg.get(black_box(lookup_id));
            black_box(result).unwrap();
        });
    });
}

fn bench_registry_install(c: &mut Criterion) {
    let manifest = PluginManifest::new("Install Bench")
        .with_version(1, 0, 0)
        .with_entry_point("main.js")
        .with_permissions(PermissionSet::read_only());
    let code = b"console.log('install bench');";
    let mut pkg = PluginPackage::create(&manifest, code).unwrap();
    let kp = PluginKeyPair::generate();
    pkg.sign(&kp);

    c.bench_function("registry_install", |b| {
        b.iter(|| {
            let mut reg = PluginRegistry::new();
            reg.install(black_box(&pkg), RegistrySource::Local).unwrap();
            black_box(&reg);
        });
    });
}

// ─── Day 22: Marketplace Benchmarks ───

fn bench_marketplace_publish(c: &mut Criterion) {
    let manifest = PluginManifest::new("Publish Bench")
        .with_version(1, 0, 0)
        .with_entry_point("main.js")
        .with_category(PluginCategory::Layout)
        .with_permissions(PermissionSet::read_only());
    let code = b"console.log('marketplace publish bench');";
    let mut pkg = PluginPackage::create(&manifest, code).unwrap();
    let kp = PluginKeyPair::generate();
    pkg.sign(&kp);

    c.bench_function("marketplace_publish", |b| {
        b.iter(|| {
            let mut client = MarketplaceClient::new();
            let listing = client.publish(black_box(pkg.clone()), "pub_key").unwrap();
            black_box(listing);
        });
    });
}

fn bench_marketplace_search(c: &mut Criterion) {
    let mut client = MarketplaceClient::new();
    // Pre-populate with 50 plugins
    for i in 0..50 {
        let manifest = PluginManifest::new(format!("Plugin {i}"))
            .with_version(1, 0, 0)
            .with_entry_point("main.js")
            .with_category(if i % 3 == 0 {
                PluginCategory::Layout
            } else if i % 3 == 1 {
                PluginCategory::Color
            } else {
                PluginCategory::Export
            })
            .with_tag(if i % 2 == 0 { "grid" } else { "color" })
            .with_permissions(PermissionSet::read_only());
        let code = format!("console.log('Plugin {i}');");
        let pkg = PluginPackage::create(&manifest, code.as_bytes()).unwrap();
        client.publish(pkg, "pk").unwrap();
    }

    c.bench_function("marketplace_search_query", |b| {
        b.iter(|| {
            let results = client.search(
                black_box(&MarketplaceSearch::new().with_query("Plugin 2")),
            );
            black_box(results);
        });
    });

    c.bench_function("marketplace_search_category", |b| {
        b.iter(|| {
            let results = client.search(
                black_box(&MarketplaceSearch::new().with_category(PluginCategory::Layout)),
            );
            black_box(results);
        });
    });

    c.bench_function("marketplace_search_sorted", |b| {
        b.iter(|| {
            let results = client.search(
                black_box(
                    &MarketplaceSearch::new()
                        .sorted_by(SortOrder::Downloads)
                        .with_limit(10),
                ),
            );
            black_box(results);
        });
    });
}

fn bench_marketplace_download(c: &mut Criterion) {
    let mut client = MarketplaceClient::new();
    let manifest = PluginManifest::new("Download Bench")
        .with_version(1, 0, 0)
        .with_entry_point("main.js")
        .with_permissions(PermissionSet::read_only());
    let code = b"console.log('download bench');";
    let mut pkg = PluginPackage::create(&manifest, code).unwrap();
    let kp = PluginKeyPair::generate();
    pkg.sign(&kp);
    let id = manifest.id.to_string();
    client.publish(pkg, "pk").unwrap();

    c.bench_function("marketplace_download", |b| {
        b.iter(|| {
            let result = client.download(black_box(&id)).unwrap();
            black_box(result);
        });
    });
}

fn bench_publisher_check(c: &mut Criterion) {
    let mut publishers = TrustedPublishers::new();
    // Add 20 publishers
    for i in 0..20 {
        publishers.add_publisher(
            PublisherInfo::new(format!("Publisher {i}"), format!("key_{i:032x}"))
                .with_trust_level(TrustLevel::Verified),
        );
    }

    c.bench_function("publisher_trust_check", |b| {
        b.iter(|| {
            let trusted = publishers.is_trusted(black_box("key_00000000000000000000000000000010"));
            black_box(trusted);
        });
    });
}

fn bench_package_builder(c: &mut Criterion) {
    let kp = PluginKeyPair::generate();

    c.bench_function("package_builder_full", |b| {
        b.iter(|| {
            let pkg = PackageBuilder::new()
                .manifest(
                    PluginManifest::new("Built Plugin")
                        .with_version(1, 0, 0)
                        .with_entry_point("main.js")
                        .with_category(PluginCategory::Layout)
                        .with_permissions(PermissionSet::read_only()),
                )
                .code("console.log('built');")
                .icon(IconSize::Small, vec![0x89, 0x50])
                .build()
                .unwrap();
            black_box(pkg);
        });
    });

    c.bench_function("package_builder_signed", |b| {
        let key_bytes = *kp.private_key_bytes();
        b.iter(|| {
            let pkg = PackageBuilder::new()
                .manifest(
                    PluginManifest::new("Signed Built")
                        .with_version(1, 0, 0)
                        .with_entry_point("main.js")
                        .with_permissions(PermissionSet::read_only()),
                )
                .code("console.log('signed built');")
                .sign_with(PluginKeyPair::from_bytes(&key_bytes))
                .build()
                .unwrap();
            black_box(pkg);
        });
    });
}

criterion_group!(
    benches,
    bench_sandbox_create,
    bench_js_engine_create,
    bench_eval_literal,
    bench_js_eval,
    bench_eval_host_fn,
    bench_js_host_api,
    bench_js_create_rect,
    bench_permission_check,
    bench_domain_check,
    bench_host_get_layers,
    bench_host_create_rect,
    bench_manager_load,
    bench_plugin_value_json,
    bench_js_create_path,
    bench_js_selection,
    bench_js_undo_redo,
    bench_js_event_on,
    bench_event_flush,
    bench_path_layer_core,
    bench_ui_panel_create,
    bench_ui_message_serialize,
    bench_ui_message_roundtrip,
    bench_ui_bridge_dispatch,
    bench_ui_permission_check,
    bench_ui_create_panel_js,
    bench_manifest_parse,
    bench_signature_verify,
    bench_package_create,
    bench_package_verify,
    bench_registry_lookup,
    bench_registry_install,
    bench_marketplace_publish,
    bench_marketplace_search,
    bench_marketplace_download,
    bench_publisher_check,
    bench_package_builder,
);
criterion_main!(benches);
