//! Benchmarks for the plugin system.
//!
//! Measures:
//! - Sandbox creation (<1ms target)
//! - JsEngine creation (<5ms cold target)
//! - Script evaluation (<5ms sandbox / <10ms JS cold target)
//! - Host function calls (<500ns target)
//! - Permission checks (<50ns target)
//! - JS host API calls via Logos.* (<500ns target)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logos_core::{Document, Layer, RectLayer};
use logos_plugins::engine::JsEngine;
use logos_plugins::host::PluginHost;
use logos_plugins::manager::PluginManager;
use logos_plugins::manifest::PluginManifest;
use logos_plugins::permissions::{PermissionGuard, PermissionKind, PermissionSet};
use logos_plugins::runtime::{PluginValue, ResourceLimits, Sandbox};
use std::sync::{Arc, RwLock};

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
    let mut engine = JsEngine::new("bench", ResourceLimits::default(), PermissionSet::none());
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
        ResourceLimits::default(),
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
        ResourceLimits::default(),
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
);
criterion_main!(benches);
