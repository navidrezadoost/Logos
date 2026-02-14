//! JavaScript host API — registers `Logos.*` functions in the JS context.
//!
//! Each function:
//! 1. Checks the deadline (timeout enforcement)
//! 2. Increments the host call counter
//! 3. Checks permissions via PermissionGuard
//! 4. Accesses the document
//! 5. Returns a JsValue result
//!
//! ## Available API
//!
//! | Function | Permission | Description |
//! |----------|-----------|-------------|
//! | `Logos.getDocumentInfo()` | DocumentRead | Document metadata |
//! | `Logos.getLayers()` | DocumentRead | All layers as array |
//! | `Logos.getLayerCount()` | DocumentRead | Number of layers |
//! | `Logos.getLayer(id)` | DocumentRead | Single layer by ID |
//! | `Logos.createRect(x,y,w,h)` | DocumentWrite | Create rectangle |
//! | `Logos.deleteLayer(id)` | DocumentWrite | Delete a layer |
//! | `Logos.log(msg)` | None | Log a message |
//! | `Logos.checkTimeout()` | None | Throws if timed out |
//!
//! ## Safety
//!
//! Uses `unsafe { NativeFunction::from_closure(...) }` because closures
//! capture `Arc<RwLock<Document>>` and `Arc<RwLock<PermissionGuard>>`,
//! which are NOT boa GC types. This is explicitly safe per boa_engine
//! documentation: only capturing `Gc<T>` / `JsObject` etc. would be
//! unsound.

use crate::permissions::{PermissionGuard, PermissionKind};
use boa_engine::{Context, JsString, JsValue, NativeFunction};
use logos_core::{Document, Layer, RectLayer};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use uuid::Uuid;

/// Register all `Logos.*` host functions on a boa Context.
///
/// This creates a `Logos` global object with methods that bridge
/// to the logos-core Document through permission-gated access.
pub fn register_logos_api(
    context: &mut Context,
    document: Arc<RwLock<Document>>,
    guard: Arc<RwLock<PermissionGuard>>,
    host_call_count: Arc<RwLock<u64>>,
    deadline: Arc<RwLock<Option<Instant>>>,
) {
    let mut obj = boa_engine::object::ObjectInitializer::new(context);

    // ─── Logos.getDocumentInfo() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentRead)?;

                    let d = doc.read().map_err(js_lock_error)?;
                    let page = d.root.read().map_err(js_lock_error)?;

                    let result = boa_engine::object::ObjectInitializer::new(ctx)
                        .property(
                            JsString::from("id"),
                            JsValue::new(JsString::from(d.id.to_string())),
                            boa_engine::property::Attribute::all(),
                        )
                        .property(
                            JsString::from("version"),
                            JsValue::new(d.version as i32),
                            boa_engine::property::Attribute::all(),
                        )
                        .property(
                            JsString::from("pageName"),
                            JsValue::new(JsString::from(page.name.as_str())),
                            boa_engine::property::Attribute::all(),
                        )
                        .property(
                            JsString::from("layerCount"),
                            JsValue::new(page.layers.len() as i32),
                            boa_engine::property::Attribute::all(),
                        )
                        .build();

                    Ok(JsValue::from(result))
                },
            )
        };
        obj.function(f, JsString::from("getDocumentInfo"), 0);
    }

    // ─── Logos.getLayers() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentRead)?;

                    let d = doc.read().map_err(js_lock_error)?;
                    let page = d.root.read().map_err(js_lock_error)?;

                    let arr = boa_engine::object::builtins::JsArray::new(ctx);
                    for layer in &page.layers {
                        let obj = layer_to_js_object(layer, ctx);
                        arr.push(JsValue::from(obj), ctx).map_err(|e| {
                            boa_engine::JsError::from_opaque(JsValue::new(
                                JsString::from(e.to_string()),
                            ))
                        })?;
                    }

                    Ok(arr.into())
                },
            )
        };
        obj.function(f, JsString::from("getLayers"), 0);
    }

    // ─── Logos.getLayerCount() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentRead)?;

                    let d = doc.read().map_err(js_lock_error)?;
                    let page = d.root.read().map_err(js_lock_error)?;

                    Ok(JsValue::new(page.layers.len() as i32))
                },
            )
        };
        obj.function(f, JsString::from("getLayerCount"), 0);
    }

    // ─── Logos.getLayer(id) ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentRead)?;

                    let id_str = args
                        .first()
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_std_string_escaped())
                        .ok_or_else(|| js_error("getLayer requires a string ID argument"))?;

                    let target_id = Uuid::parse_str(&id_str)
                        .map_err(|e| js_error(&format!("invalid UUID: {e}")))?;

                    let d = doc.read().map_err(js_lock_error)?;
                    let page = d.root.read().map_err(js_lock_error)?;

                    for layer in &page.layers {
                        if layer.id() == target_id {
                            return Ok(JsValue::from(layer_to_js_object(layer, ctx)));
                        }
                    }

                    Err(js_error(&format!("layer not found: {id_str}")))
                },
            )
        };
        obj.function(f, JsString::from("getLayer"), 1);
    }

    // ─── Logos.createRect(x, y, w, h) ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    if args.len() < 4 {
                        return Err(js_error(
                            "createRect requires 4 arguments: x, y, width, height",
                        ));
                    }

                    let x = js_to_f32(&args[0])?;
                    let y = js_to_f32(&args[1])?;
                    let w = js_to_f32(&args[2])?;
                    let h = js_to_f32(&args[3])?;

                    let rect = RectLayer::new(x, y, w, h);
                    let id = rect.id;
                    let layer = Layer::Rect(rect);

                    let d = doc.read().map_err(js_lock_error)?;
                    d.add_layer(layer)
                        .map_err(|e| js_error(&e))?;

                    Ok(JsValue::new(JsString::from(id.to_string())))
                },
            )
        };
        obj.function(f, JsString::from("createRect"), 4);
    }

    // ─── Logos.deleteLayer(id) ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let id_str = args
                        .first()
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_std_string_escaped())
                        .ok_or_else(|| {
                            js_error("deleteLayer requires a string ID argument")
                        })?;

                    let target_id = Uuid::parse_str(&id_str)
                        .map_err(|e| js_error(&format!("invalid UUID: {e}")))?;

                    let d = doc.read().map_err(js_lock_error)?;
                    let mut page = d.root.write().map_err(|e| {
                        boa_engine::JsError::from_opaque(JsValue::new(
                            JsString::from(format!("lock error: {e}")),
                        ))
                    })?;

                    let before = page.layers.len();
                    page.layers.retain(|l| l.id() != target_id);
                    if page.layers.len() == before {
                        return Err(js_error(&format!("layer not found: {id_str}")));
                    }

                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("deleteLayer"), 1);
    }

    // ─── Logos.log(msg) ───
    {
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);

                    let msg = args
                        .first()
                        .map(|v| {
                            v.to_string(ctx)
                                .map(|s| s.to_std_string_escaped())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    log::info!("[plugin] {msg}");
                    Ok(JsValue::undefined())
                },
            )
        };
        obj.function(f, JsString::from("log"), 1);
    }

    // ─── Logos.checkTimeout() ───
    {
        let dl = Arc::clone(&deadline);
        let calls = Arc::clone(&host_call_count);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("checkTimeout"), 0);
    }

    // Build and register the Logos global object
    let logos_obj = obj.build();
    let _ = context.register_global_property(
        JsString::from("Logos"),
        logos_obj,
        boa_engine::property::Attribute::all(),
    );
}

// ───────────────────── Helper Functions ─────────────────────

/// Create a JsError from a message string.
fn js_error(msg: &str) -> boa_engine::JsError {
    boa_engine::JsError::from_opaque(JsValue::new(JsString::from(msg)))
}

/// Check if execution has exceeded the deadline.
fn check_deadline(deadline: &Arc<RwLock<Option<Instant>>>) -> Result<(), boa_engine::JsError> {
    if let Some(dl) = *deadline.read().unwrap() {
        if Instant::now() > dl {
            return Err(js_error("timeout: execution time limit exceeded"));
        }
    }
    Ok(())
}

/// Increment the host call counter.
fn increment_calls(counter: &Arc<RwLock<u64>>) {
    *counter.write().unwrap() += 1;
}

/// Check a permission via the guard, returning a JsError on denial.
fn check_permission(
    guard: &Arc<RwLock<PermissionGuard>>,
    kind: &PermissionKind,
) -> Result<(), boa_engine::JsError> {
    guard
        .write()
        .map_err(|e| js_error(&format!("lock error: {e}")))?
        .check(kind)
        .map_err(|msg| js_error(&format!("permission denied: {msg}")))
}

/// Convert a JsValue to f32, supporting both Integer and Rational.
fn js_to_f32(val: &JsValue) -> Result<f32, boa_engine::JsError> {
    if let Some(n) = val.as_number() {
        Ok(n as f32)
    } else {
        Err(js_error("expected a number argument"))
    }
}

/// Convert a lock error to a JsError.
fn js_lock_error<T: std::fmt::Display>(err: T) -> boa_engine::JsError {
    js_error(&format!("lock error: {err}"))
}

/// Convert a logos_core Layer to a boa JsObject.
fn layer_to_js_object(layer: &Layer, ctx: &mut Context) -> boa_engine::JsObject {
    let mut builder = boa_engine::object::ObjectInitializer::new(ctx);

    builder
        .property(
            JsString::from("id"),
            JsValue::new(JsString::from(layer.id().to_string())),
            boa_engine::property::Attribute::all(),
        );

    match layer {
        Layer::Rect(r) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("rect")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(r.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(r.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(r.bounds.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(r.bounds.height as f64),
                    boa_engine::property::Attribute::all(),
                );
        }
        Layer::Ellipse(e) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("ellipse")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(e.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(e.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(e.bounds.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(e.bounds.height as f64),
                    boa_engine::property::Attribute::all(),
                );
        }
        Layer::Text(t) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("text")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("content"),
                    JsValue::new(JsString::from(t.content.as_str())),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(t.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(t.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                );
        }
        Layer::Frame(fr) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("frame")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(fr.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(fr.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(fr.bounds.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(fr.bounds.height as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("children"),
                    JsValue::new(fr.children.len() as i32),
                    boa_engine::property::Attribute::all(),
                );
        }
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::JsEngine;
    use crate::permissions::PermissionSet;
    use crate::runtime::{PluginValue, ResourceLimits};
    use logos_core::{Document, Layer, RectLayer};

    fn doc_with_rect() -> Arc<RwLock<Document>> {
        let doc = Document::new();
        doc.add_layer(Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0)))
            .unwrap();
        Arc::new(RwLock::new(doc))
    }

    fn engine_with_doc(doc: Arc<RwLock<Document>>, perms: PermissionSet) -> JsEngine {
        let mut engine = JsEngine::new("test", ResourceLimits::default(), perms);
        engine.register_document(doc);
        engine
    }

    #[test]
    fn test_js_get_layer_count() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());
        let result = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(result.as_int(), Some(1));
    }

    #[test]
    fn test_js_get_layers() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());
        let result = engine.execute("Logos.getLayers()").unwrap();
        if let PluginValue::Array(layers) = result {
            assert_eq!(layers.len(), 1);
            if let PluginValue::Object(ref layer) = layers[0] {
                assert_eq!(layer.get("type").and_then(|v: &PluginValue| v.as_str()), Some("rect"));
            } else {
                panic!("expected object in layers array");
            }
        } else {
            panic!("expected array, got {:?}", result);
        }
    }

    #[test]
    fn test_js_get_document_info() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());
        let result = engine.execute("Logos.getDocumentInfo()").unwrap();
        if let PluginValue::Object(info) = result {
            assert!(info.contains_key("id"));
            assert!(info.contains_key("layerCount"));
            assert_eq!(info.get("layerCount").and_then(|v: &PluginValue| v.as_int()), Some(1));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_js_create_rect() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.createRect(10, 20, 100, 50)").unwrap();
        assert!(result.as_str().is_some());
        assert!(result.as_str().unwrap().len() > 10);

        let count = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(count.as_int(), Some(1));
    }

    #[test]
    fn test_js_create_and_delete() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine
            .execute("var rectId = Logos.createRect(0, 0, 50, 50)")
            .unwrap();

        let count1 = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(count1.as_int(), Some(1));

        engine.execute("Logos.deleteLayer(rectId)").unwrap();

        let count2 = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(count2.as_int(), Some(0));
    }

    #[test]
    fn test_js_permission_denied_read() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::none());

        let result = engine.execute("Logos.getLayerCount()");
        assert!(result.is_err());
        if let Err(ref e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("permission denied") || msg.contains("Permission"),
                "error should mention permission: {msg}"
            );
        }
    }

    #[test]
    fn test_js_permission_denied_write() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        let result = engine.execute("Logos.createRect(0, 0, 50, 50)");
        assert!(result.is_err());
    }

    #[test]
    fn test_js_read_allowed_write_denied() {
        let doc = doc_with_rect();
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        let count = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(count.as_int(), Some(1));

        assert!(engine.execute("Logos.createRect(0,0,50,50)").is_err());
    }

    #[test]
    fn test_js_multi_statement_with_host() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createRect(0, 0, 100, 100);
            Logos.createRect(50, 50, 200, 200);
            Logos.createRect(100, 100, 300, 300);
            Logos.getLayerCount();
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_int(), Some(3));
    }

    #[test]
    fn test_js_loop_create_rects() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            for (let i = 0; i < 5; i++) {
                Logos.createRect(i * 10, i * 10, 50, 50);
            }
            Logos.getLayerCount();
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_int(), Some(5));
    }

    #[test]
    fn test_js_function_with_host() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            function createGrid(rows, cols, size) {
                for (let r = 0; r < rows; r++) {
                    for (let c = 0; c < cols; c++) {
                        Logos.createRect(c * size, r * size, size, size);
                    }
                }
                return Logos.getLayerCount();
            }
            createGrid(3, 3, 20);
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_int(), Some(9));
    }

    #[test]
    fn test_js_logos_log() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::none());

        engine.execute("Logos.log('hello from plugin')").unwrap();
    }
}
