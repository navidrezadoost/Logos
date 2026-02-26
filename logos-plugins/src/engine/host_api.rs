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
//! | `Logos.createPath(commands)` | DocumentWrite | Create path/bezier |
//! | `Logos.deleteLayer(id)` | DocumentWrite | Delete a layer |
//! | `Logos.getSelection()` | DocumentRead | Get selected layer IDs |
//! | `Logos.setSelection(ids)` | DocumentWrite | Set selection |
//! | `Logos.clearSelection()` | DocumentWrite | Clear selection |
//! | `Logos.undo()` | DocumentWrite | Undo last action |
//! | `Logos.redo()` | DocumentWrite | Redo last undone action |
//! | `Logos.on(event, callback)` | None | Register event listener |
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

use crate::engine::events::{EventBus, EventData, EventKind, EventPayload};
use crate::engine::ui::{
    DockPosition, PanelSize, UiBridge, UiComponent, UiMessage, UiValue,
};
use crate::permissions::{PermissionGuard, PermissionKind};
use boa_engine::{Context, JsString, JsValue, NativeFunction};
use logos_core::{Document, Layer, PathCommand, PathLayer, Point, RectLayer, UndoAction, UndoStack};
use std::collections::HashMap;
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
    undo_stack: Arc<RwLock<UndoStack>>,
    event_bus: Arc<RwLock<EventBus>>,
    plugin_id: Uuid,
    ui_bridge: Arc<RwLock<UiBridge>>,
) {
    // ═══════════════════════════════════════════════════════════════
    // Build Logos.ui sub-object FIRST (separate ObjectInitializer
    // scope to avoid double-borrowing context)
    // ═══════════════════════════════════════════════════════════════
    let ui_built = {
        let mut ui_obj = boa_engine::object::ObjectInitializer::new(context);

        // ─── Logos.ui.createPanel(title, dock, [options]) ───
        {
            let bridge = Arc::clone(&ui_bridge);
            let calls = Arc::clone(&host_call_count);
            let dl = Arc::clone(&deadline);
            let pid = plugin_id;
            let f = unsafe {
                NativeFunction::from_closure(
                    move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                        check_deadline(&dl)?;
                        increment_calls(&calls);

                        let title = args
                            .first()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                            .transpose()
                            .map_err(|_| js_error("createPanel: title must be a string"))?
                            .unwrap_or_else(|| "Plugin Panel".to_string());

                        let dock_str = args
                            .get(1)
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                            .transpose()
                            .map_err(|_| js_error("createPanel: dock must be a string"))?
                            .unwrap_or_else(|| "right".to_string());
                        let dock = DockPosition::from_str(&dock_str)
                            .unwrap_or(DockPosition::Right);

                        let mut size = PanelSize::default();
                        let mut components = Vec::new();

                        if let Some(opts_val) = args.get(2) {
                            if let Some(opts_obj) = opts_val.as_object() {
                                if let Ok(w_val) = opts_obj.get(JsString::from("width"), ctx) {
                                    if let Some(w) = w_val.as_number() {
                                        size.preferred_width = w as u32;
                                    }
                                }
                                if let Ok(h_val) = opts_obj.get(JsString::from("height"), ctx) {
                                    if let Some(h) = h_val.as_number() {
                                        size.preferred_height = h as u32;
                                    }
                                }
                                if let Ok(comp_val) = opts_obj.get(JsString::from("components"), ctx) {
                                    if let Some(comp_obj) = comp_val.as_object() {
                                        if let Ok(len_val) = comp_obj.get(JsString::from("length"), ctx) {
                                            if let Some(len) = len_val.as_number() {
                                                for i in 0..len as u32 {
                                                    if let Ok(item) = comp_obj.get(i, ctx) {
                                                        if let Some(c) = parse_js_component(&item, ctx) {
                                                            components.push(c);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let mut b = bridge.write().map_err(|e| js_error(&e.to_string()))?;
                        let panel_id = if components.is_empty() {
                            b.create_panel(pid, title, dock)
                        } else {
                            b.create_panel_full(pid, title, dock, size, components)
                        }.map_err(|e| js_error(&e))?;

                        Ok(JsValue::new(JsString::from(panel_id.to_string())))
                    },
                )
            };
            ui_obj.function(f, JsString::from("createPanel"), 2);
        }

        // ─── Logos.ui.closePanel(panelId) ───
        {
            let bridge = Arc::clone(&ui_bridge);
            let calls = Arc::clone(&host_call_count);
            let dl = Arc::clone(&deadline);
            let pid = plugin_id;
            let f = unsafe {
                NativeFunction::from_closure(
                    move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                        check_deadline(&dl)?;
                        increment_calls(&calls);

                        let id_str = args
                            .first()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                            .transpose()
                            .map_err(|_| js_error("closePanel: panelId must be a string"))?
                            .ok_or_else(|| js_error("closePanel requires a panel ID"))?;
                        let panel_id = Uuid::parse_str(&id_str)
                            .map_err(|e| js_error(&format!("invalid panel ID: {e}")))?;

                        let mut b = bridge.write().map_err(|e| js_error(&e.to_string()))?;
                        b.close_panel(pid, panel_id).map_err(|e| js_error(&e))?;

                        Ok(JsValue::new(true))
                    },
                )
            };
            ui_obj.function(f, JsString::from("closePanel"), 1);
        }

        // ─── Logos.ui.sendMessage(panelId, message) ───
        {
            let bridge = Arc::clone(&ui_bridge);
            let calls = Arc::clone(&host_call_count);
            let dl = Arc::clone(&deadline);
            let pid = plugin_id;
            let f = unsafe {
                NativeFunction::from_closure(
                    move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                        check_deadline(&dl)?;
                        increment_calls(&calls);

                        let id_str = args
                            .first()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                            .transpose()
                            .map_err(|_| js_error("sendMessage: panelId must be a string"))?
                            .ok_or_else(|| js_error("sendMessage requires a panel ID"))?;
                        let panel_id = Uuid::parse_str(&id_str)
                            .map_err(|e| js_error(&format!("invalid panel ID: {e}")))?;

                        let msg = args
                            .get(1)
                            .map(|v| parse_js_ui_message(v, ctx))
                            .transpose()?
                            .ok_or_else(|| js_error("sendMessage requires a message object"))?;

                        let mut b = bridge.write().map_err(|e| js_error(&e.to_string()))?;
                        b.send_to_panel(pid, panel_id, msg).map_err(|e| js_error(&e))?;

                        Ok(JsValue::new(true))
                    },
                )
            };
            ui_obj.function(f, JsString::from("sendMessage"), 2);
        }

        // ─── Logos.ui.updatePanel(panelId, components) ───
        {
            let bridge = Arc::clone(&ui_bridge);
            let calls = Arc::clone(&host_call_count);
            let dl = Arc::clone(&deadline);
            let pid = plugin_id;
            let f = unsafe {
                NativeFunction::from_closure(
                    move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                        check_deadline(&dl)?;
                        increment_calls(&calls);

                        let id_str = args
                            .first()
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                            .transpose()
                            .map_err(|_| js_error("updatePanel: panelId must be a string"))?
                            .ok_or_else(|| js_error("updatePanel requires a panel ID"))?;
                        let panel_id = Uuid::parse_str(&id_str)
                            .map_err(|e| js_error(&format!("invalid panel ID: {e}")))?;

                        let mut components = Vec::new();
                        if let Some(arr_val) = args.get(1) {
                            if let Some(arr_obj) = arr_val.as_object() {
                                if let Ok(len_val) = arr_obj.get(JsString::from("length"), ctx) {
                                    if let Some(len) = len_val.as_number() {
                                        for i in 0..len as u32 {
                                            if let Ok(item) = arr_obj.get(i, ctx) {
                                                if let Some(c) = parse_js_component(&item, ctx) {
                                                    components.push(c);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        let mut b = bridge.write().map_err(|e| js_error(&e.to_string()))?;
                        b.update_panel_components(pid, panel_id, components)
                            .map_err(|e| js_error(&e))?;

                        Ok(JsValue::new(true))
                    },
                )
            };
            ui_obj.function(f, JsString::from("updatePanel"), 2);
        }

        // ─── Logos.ui.getPanels() ───
        {
            let bridge = Arc::clone(&ui_bridge);
            let calls = Arc::clone(&host_call_count);
            let dl = Arc::clone(&deadline);
            let pid = plugin_id;
            let f = unsafe {
                NativeFunction::from_closure(
                    move |_this: &JsValue, _args: &[JsValue], ctx: &mut Context| {
                        check_deadline(&dl)?;
                        increment_calls(&calls);

                        let b = bridge.read().map_err(|e| js_error(&e.to_string()))?;
                        let panels = b.plugin_panels(pid);

                        let arr = boa_engine::object::builtins::JsArray::new(ctx);
                        for panel in panels {
                            let panel_obj = boa_engine::object::ObjectInitializer::new(ctx)
                                .property(
                                    JsString::from("id"),
                                    JsValue::new(JsString::from(panel.id.to_string())),
                                    boa_engine::property::Attribute::all(),
                                )
                                .property(
                                    JsString::from("title"),
                                    JsValue::new(JsString::from(panel.title.as_str())),
                                    boa_engine::property::Attribute::all(),
                                )
                                .property(
                                    JsString::from("dock"),
                                    JsValue::new(JsString::from(panel.dock.as_str())),
                                    boa_engine::property::Attribute::all(),
                                )
                                .property(
                                    JsString::from("state"),
                                    JsValue::new(JsString::from(panel.state.as_str())),
                                    boa_engine::property::Attribute::all(),
                                )
                                .property(
                                    JsString::from("componentCount"),
                                    JsValue::new(panel.components.len() as i32),
                                    boa_engine::property::Attribute::all(),
                                )
                                .build();
                            arr.push(JsValue::from(panel_obj), ctx)
                                .map_err(|e| js_error(&e.to_string()))?;
                        }

                        Ok(JsValue::from(arr))
                    },
                )
            };
            ui_obj.function(f, JsString::from("getPanels"), 0);
        }

        ui_obj.build()
    };

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
        let undo = Arc::clone(&undo_stack);
        let events = Arc::clone(&event_bus);
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

                    // Push undo action
                    if let Ok(mut us) = undo.write() {
                        us.push(UndoAction::AddLayer(layer.clone()));
                    }

                    let d = doc.read().map_err(js_lock_error)?;
                    d.add_layer(layer)
                        .map_err(|e| js_error(&e))?;

                    // Emit layerAdded event
                    if let Ok(mut eb) = events.write() {
                        let mut data = HashMap::new();
                        data.insert("id".to_string(), EventData::String(id.to_string()));
                        data.insert("type".to_string(), EventData::String("rect".to_string()));
                        eb.emit(EventPayload {
                            kind: EventKind::LayerAdded,
                            data,
                        });
                    }

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
        let undo = Arc::clone(&undo_stack);
        let events = Arc::clone(&event_bus);
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
                    let removed = d
                        .remove_layer(target_id)
                        .map_err(|e| js_error(&e))?;

                    // Push undo action
                    if let Ok(mut us) = undo.write() {
                        us.push(UndoAction::RemoveLayer(removed));
                    }

                    // Emit layerRemoved event
                    if let Ok(mut eb) = events.write() {
                        let mut data = HashMap::new();
                        data.insert("id".to_string(), EventData::String(id_str));
                        eb.emit(EventPayload {
                            kind: EventKind::LayerRemoved,
                            data,
                        });
                    }

                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("deleteLayer"), 1);
    }

    // ─── Logos.createPath(commands) ───
    // commands: array of {type: "moveTo"|"lineTo"|"quadTo"|"bezierTo"|"close", ...coords}
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let undo = Arc::clone(&undo_stack);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let cmds_val = args
                        .first()
                        .ok_or_else(|| js_error("createPath requires a commands array"))?;

                    let cmds_obj = cmds_val
                        .as_object()
                        .ok_or_else(|| js_error("createPath argument must be an array"))?;

                    let length_val = cmds_obj
                        .get(JsString::from("length"), ctx)
                        .map_err(|e| js_error(&e.to_string()))?;
                    let length = length_val
                        .as_number()
                        .ok_or_else(|| js_error("commands must be an array with length"))? as usize;

                    if length == 0 {
                        return Err(js_error("createPath requires at least one command"));
                    }

                    let mut commands = Vec::with_capacity(length);
                    for i in 0..length {
                        let elem = cmds_obj
                            .get(i as u32, ctx)
                            .map_err(|e| js_error(&e.to_string()))?;
                        let elem_obj = elem
                            .as_object()
                            .ok_or_else(|| js_error(&format!("command[{i}] must be an object")))?;

                        let type_val = elem_obj
                            .get(JsString::from("type"), ctx)
                            .map_err(|e| js_error(&e.to_string()))?;
                        let cmd_type = type_val
                            .as_string()
                            .map(|s| s.to_std_string_escaped())
                            .ok_or_else(|| js_error(&format!("command[{i}].type must be a string")))?;

                        let cmd = match cmd_type.as_str() {
                            "moveTo" => {
                                let x = js_obj_f32(&elem_obj, "x", ctx)?;
                                let y = js_obj_f32(&elem_obj, "y", ctx)?;
                                PathCommand::MoveTo(Point::new(x, y))
                            }
                            "lineTo" => {
                                let x = js_obj_f32(&elem_obj, "x", ctx)?;
                                let y = js_obj_f32(&elem_obj, "y", ctx)?;
                                PathCommand::LineTo(Point::new(x, y))
                            }
                            "quadTo" => {
                                let cx = js_obj_f32(&elem_obj, "cx", ctx)?;
                                let cy = js_obj_f32(&elem_obj, "cy", ctx)?;
                                let x = js_obj_f32(&elem_obj, "x", ctx)?;
                                let y = js_obj_f32(&elem_obj, "y", ctx)?;
                                PathCommand::QuadTo {
                                    ctrl: Point::new(cx, cy),
                                    end: Point::new(x, y),
                                }
                            }
                            "bezierTo" => {
                                let cp1x = js_obj_f32(&elem_obj, "cp1x", ctx)?;
                                let cp1y = js_obj_f32(&elem_obj, "cp1y", ctx)?;
                                let cp2x = js_obj_f32(&elem_obj, "cp2x", ctx)?;
                                let cp2y = js_obj_f32(&elem_obj, "cp2y", ctx)?;
                                let x = js_obj_f32(&elem_obj, "x", ctx)?;
                                let y = js_obj_f32(&elem_obj, "y", ctx)?;
                                PathCommand::BezierTo {
                                    cp1: Point::new(cp1x, cp1y),
                                    cp2: Point::new(cp2x, cp2y),
                                    end: Point::new(x, y),
                                }
                            }
                            "close" => PathCommand::Close,
                            other => {
                                return Err(js_error(&format!(
                                    "unknown command type: {other}"
                                )));
                            }
                        };
                        commands.push(cmd);
                    }

                    let path = PathLayer::new(commands);
                    let id = path.id;
                    let layer = Layer::Path(path);

                    // Push undo action
                    if let Ok(mut us) = undo.write() {
                        us.push(UndoAction::AddLayer(layer.clone()));
                    }

                    let d = doc.read().map_err(js_lock_error)?;
                    d.add_layer(layer)
                        .map_err(|e| js_error(&e))?;

                    // Emit layerAdded event
                    if let Ok(mut eb) = events.write() {
                        let mut data = HashMap::new();
                        data.insert("id".to_string(), EventData::String(id.to_string()));
                        data.insert("type".to_string(), EventData::String("path".to_string()));
                        eb.emit(EventPayload {
                            kind: EventKind::LayerAdded,
                            data,
                        });
                    }

                    Ok(JsValue::new(JsString::from(id.to_string())))
                },
            )
        };
        obj.function(f, JsString::from("createPath"), 1);
    }

    // ─── Logos.getSelection() ───
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
                    let sel = d.get_selection().map_err(|e| js_error(&e))?;

                    let arr = boa_engine::object::builtins::JsArray::new(ctx);
                    for id in &sel {
                        arr.push(
                            JsValue::new(JsString::from(id.to_string())),
                            ctx,
                        )
                        .map_err(|e| js_error(&e.to_string()))?;
                    }

                    Ok(arr.into())
                },
            )
        };
        obj.function(f, JsString::from("getSelection"), 0);
    }

    // ─── Logos.setSelection(ids) ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let arr_val = args
                        .first()
                        .ok_or_else(|| js_error("setSelection requires an array of IDs"))?;

                    let arr_obj = arr_val
                        .as_object()
                        .ok_or_else(|| js_error("setSelection argument must be an array"))?;

                    let length_val = arr_obj
                        .get(JsString::from("length"), ctx)
                        .map_err(|e| js_error(&e.to_string()))?;
                    let length = length_val
                        .as_number()
                        .ok_or_else(|| js_error("argument must be an array with length"))? as usize;

                    let mut ids = Vec::with_capacity(length);
                    for i in 0..length {
                        let elem = arr_obj
                            .get(i as u32, ctx)
                            .map_err(|e| js_error(&e.to_string()))?;
                        let id_str = elem
                            .as_string()
                            .map(|s| s.to_std_string_escaped())
                            .ok_or_else(|| js_error(&format!("selection[{i}] must be a string")))?;
                        let uuid = Uuid::parse_str(&id_str)
                            .map_err(|e| js_error(&format!("invalid UUID at [{i}]: {e}")))?;
                        ids.push(uuid);
                    }

                    let d = doc.read().map_err(js_lock_error)?;
                    d.set_selection(ids.clone()).map_err(|e| js_error(&e))?;

                    // Emit selectionChanged event
                    if let Ok(mut eb) = events.write() {
                        let mut data = HashMap::new();
                        data.insert(
                            "ids".to_string(),
                            EventData::StringArray(ids.iter().map(|id| id.to_string()).collect()),
                        );
                        eb.emit(EventPayload {
                            kind: EventKind::SelectionChanged,
                            data,
                        });
                    }

                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("setSelection"), 1);
    }

    // ─── Logos.clearSelection() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let d = doc.read().map_err(js_lock_error)?;
                    d.clear_selection().map_err(|e| js_error(&e))?;

                    // Emit selectionChanged event with empty array
                    if let Ok(mut eb) = events.write() {
                        let mut data = HashMap::new();
                        data.insert("ids".to_string(), EventData::StringArray(Vec::new()));
                        eb.emit(EventPayload {
                            kind: EventKind::SelectionChanged,
                            data,
                        });
                    }

                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("clearSelection"), 0);
    }

    // ─── Logos.undo() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let undo = Arc::clone(&undo_stack);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let action = {
                        let mut us = undo.write().map_err(js_lock_error)?;
                        us.pop_undo()
                    };

                    match action {
                        Some(UndoAction::AddLayer(layer)) => {
                            // Undo an add → remove the layer
                            let id = layer.id();
                            let d = doc.read().map_err(js_lock_error)?;
                            let _ = d.remove_layer(id);

                            if let Ok(mut eb) = events.write() {
                                let mut data = HashMap::new();
                                data.insert("id".to_string(), EventData::String(id.to_string()));
                                eb.emit(EventPayload {
                                    kind: EventKind::LayerRemoved,
                                    data,
                                });
                            }

                            Ok(JsValue::new(true))
                        }
                        Some(UndoAction::RemoveLayer(layer)) => {
                            // Undo a remove → re-add the layer
                            let id = layer.id();
                            let d = doc.read().map_err(js_lock_error)?;
                            d.add_layer(layer)
                                .map_err(|e| js_error(&e))?;

                            if let Ok(mut eb) = events.write() {
                                let mut data = HashMap::new();
                                data.insert("id".to_string(), EventData::String(id.to_string()));
                                data.insert("type".to_string(), EventData::String("restored".to_string()));
                                eb.emit(EventPayload {
                                    kind: EventKind::LayerAdded,
                                    data,
                                });
                            }

                            Ok(JsValue::new(true))
                        }
                        None => Ok(JsValue::new(false)),
                    }
                },
            )
        };
        obj.function(f, JsString::from("undo"), 0);
    }

    // ─── Logos.redo() ───
    {
        let doc = Arc::clone(&document);
        let g = Arc::clone(&guard);
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let undo = Arc::clone(&undo_stack);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, _args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);
                    check_permission(&g, &PermissionKind::DocumentWrite)?;

                    let action = {
                        let mut us = undo.write().map_err(js_lock_error)?;
                        us.pop_redo()
                    };

                    match action {
                        Some(UndoAction::AddLayer(layer)) => {
                            // Redo an add → re-add the layer
                            let id = layer.id();
                            let d = doc.read().map_err(js_lock_error)?;
                            d.add_layer(layer)
                                .map_err(|e| js_error(&e))?;

                            if let Ok(mut eb) = events.write() {
                                let mut data = HashMap::new();
                                data.insert("id".to_string(), EventData::String(id.to_string()));
                                eb.emit(EventPayload {
                                    kind: EventKind::LayerAdded,
                                    data,
                                });
                            }

                            Ok(JsValue::new(true))
                        }
                        Some(UndoAction::RemoveLayer(layer)) => {
                            // Redo a remove → remove the layer again
                            let id = layer.id();
                            let d = doc.read().map_err(js_lock_error)?;
                            let _ = d.remove_layer(id);

                            if let Ok(mut eb) = events.write() {
                                let mut data = HashMap::new();
                                data.insert("id".to_string(), EventData::String(id.to_string()));
                                eb.emit(EventPayload {
                                    kind: EventKind::LayerRemoved,
                                    data,
                                });
                            }

                            Ok(JsValue::new(true))
                        }
                        None => Ok(JsValue::new(false)),
                    }
                },
            )
        };
        obj.function(f, JsString::from("redo"), 0);
    }

    // ─── Logos.on(event, callback) ───
    {
        let calls = Arc::clone(&host_call_count);
        let dl = Arc::clone(&deadline);
        let events = Arc::clone(&event_bus);
        let f = unsafe {
            NativeFunction::from_closure(
                move |_this: &JsValue, args: &[JsValue], _ctx: &mut Context| {
                    check_deadline(&dl)?;
                    increment_calls(&calls);

                    let event_name = args
                        .first()
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_std_string_escaped())
                        .ok_or_else(|| js_error("on() requires event name as first argument"))?;

                    let kind = EventKind::from_str(&event_name)
                        .ok_or_else(|| js_error(&format!("unknown event: {event_name}")))?;

                    let callback = args
                        .get(1)
                        .and_then(|v| v.as_object())
                        .ok_or_else(|| js_error("on() requires a callback function as second argument"))?
                        .clone();

                    if !callback.is_callable() {
                        return Err(js_error("on() second argument must be a function"));
                    }

                    events
                        .write()
                        .map_err(js_lock_error)?
                        .on(kind, callback);

                    Ok(JsValue::new(true))
                },
            )
        };
        obj.function(f, JsString::from("on"), 2);
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

    // Attach pre-built Logos.ui sub-object
    obj.property(
        JsString::from("ui"),
        JsValue::from(ui_built),
        boa_engine::property::Attribute::all(),
    );

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

/// Extract a numeric property from a JS object as f32.
fn js_obj_f32(
    obj: &boa_engine::JsObject,
    key: &str,
    ctx: &mut Context,
) -> Result<f32, boa_engine::JsError> {
    let val = obj
        .get(JsString::from(key), ctx)
        .map_err(|e| js_error(&e.to_string()))?;
    val.as_number()
        .map(|n| n as f32)
        .ok_or_else(|| js_error(&format!("property '{key}' must be a number")))
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
        Layer::Path(p) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("path")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(p.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(p.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(p.bounds.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(p.bounds.height as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("commandCount"),
                    JsValue::new(p.commands.len() as i32),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("closed"),
                    JsValue::new(p.closed),
                    boa_engine::property::Attribute::all(),
                );
        }
        Layer::Artboard(ab) => {
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("artboard")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("name"),
                    JsValue::new(JsString::from(ab.name.as_str())),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(ab.bounds.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(ab.bounds.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(ab.bounds.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(ab.bounds.height as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("children"),
                    JsValue::new(ab.children.len() as i32),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("clipContent"),
                    JsValue::new(ab.clip_content),
                    boa_engine::property::Attribute::all(),
                );
        }
        Layer::Drawer(d) => {
            let eff = d.effective_bounds();
            builder
                .property(
                    JsString::from("type"),
                    JsValue::new(JsString::from("drawer")),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("name"),
                    JsValue::new(JsString::from(d.name.as_str())),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("x"),
                    JsValue::rational(eff.x as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("y"),
                    JsValue::rational(eff.y as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("width"),
                    JsValue::rational(eff.width as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("height"),
                    JsValue::rational(eff.height as f64),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("children"),
                    JsValue::new(d.children.len() as i32),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("edge"),
                    JsValue::new(JsString::from(format!("{:?}", d.edge))),
                    boa_engine::property::Attribute::all(),
                )
                .property(
                    JsString::from("state"),
                    JsValue::new(JsString::from(format!("{:?}", d.state))),
                    boa_engine::property::Attribute::all(),
                );
        }
    }

    builder.build()
}

// ───────────────────── UI Helper Functions ─────────────────────

/// Parse a JavaScript object into a UiComponent.
///
/// Expects objects like:
/// - `{ type: "label", text: "Hello" }`
/// - `{ type: "button", label: "Click", action: "submit" }`
/// - `{ type: "numberInput", label: "X", key: "x", value: 0, min: -1000, max: 1000, step: 1 }`
/// - `{ type: "colorPicker", label: "Fill", key: "fill", value: "#FF0000FF" }`
/// - `{ type: "toggle", label: "Visible", key: "visible", value: true }`
/// - `{ type: "textInput", label: "Name", key: "name", value: "Layer 1", placeholder: "Enter name" }`
/// - `{ type: "select", label: "Font", key: "font", value: "Arial", options: ["Arial", "Helvetica"] }`
/// - `{ type: "separator" }`
/// - `{ type: "propertyEditor" }`
/// - `{ type: "layerList", syncSelection: true }`
fn parse_js_component(val: &JsValue, ctx: &mut Context) -> Option<UiComponent> {
    let obj = val.as_object()?;

    let type_str = obj.get(JsString::from("type"), ctx).ok()?
        .to_string(ctx).ok()?.to_std_string_escaped();

    match type_str.as_str() {
        "label" => {
            let text = js_obj_string(&obj, "text", ctx).unwrap_or_default();
            Some(UiComponent::Label { text })
        }
        "button" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let action = js_obj_string(&obj, "action", ctx).unwrap_or_default();
            Some(UiComponent::Button { label, action })
        }
        "numberInput" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let key = js_obj_string(&obj, "key", ctx).unwrap_or_default();
            let value = js_obj_f64(&obj, "value", ctx).unwrap_or(0.0);
            let min = js_obj_f64(&obj, "min", ctx).unwrap_or(f64::MIN);
            let max = js_obj_f64(&obj, "max", ctx).unwrap_or(f64::MAX);
            let step = js_obj_f64(&obj, "step", ctx).unwrap_or(1.0);
            Some(UiComponent::NumberInput { label, key, value, min, max, step })
        }
        "textInput" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let key = js_obj_string(&obj, "key", ctx).unwrap_or_default();
            let value = js_obj_string(&obj, "value", ctx).unwrap_or_default();
            let placeholder = js_obj_string(&obj, "placeholder", ctx).unwrap_or_default();
            Some(UiComponent::TextInput { label, key, value, placeholder })
        }
        "colorPicker" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let key = js_obj_string(&obj, "key", ctx).unwrap_or_default();
            let value = js_obj_string(&obj, "value", ctx).unwrap_or_else(|| "#000000FF".to_string());
            Some(UiComponent::ColorPicker { label, key, value })
        }
        "toggle" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let key = js_obj_string(&obj, "key", ctx).unwrap_or_default();
            let value = obj.get(JsString::from("value"), ctx).ok()
                .and_then(|v| v.as_boolean()).unwrap_or(false);
            Some(UiComponent::Toggle { label, key, value })
        }
        "select" => {
            let label = js_obj_string(&obj, "label", ctx).unwrap_or_default();
            let key = js_obj_string(&obj, "key", ctx).unwrap_or_default();
            let value = js_obj_string(&obj, "value", ctx).unwrap_or_default();
            let mut options = Vec::new();
            if let Ok(opts_val) = obj.get(JsString::from("options"), ctx) {
                if let Some(opts_obj) = opts_val.as_object() {
                    if let Ok(len_val) = opts_obj.get(JsString::from("length"), ctx) {
                        if let Some(len) = len_val.as_number() {
                            for i in 0..len as u32 {
                                if let Ok(item) = opts_obj.get(i, ctx) {
                                    if let Ok(s) = item.to_string(ctx) {
                                        options.push(s.to_std_string_escaped());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(UiComponent::Select { label, key, value, options })
        }
        "separator" => Some(UiComponent::Separator),
        "propertyEditor" => Some(UiComponent::PropertyEditor),
        "layerList" => {
            let sync = obj.get(JsString::from("syncSelection"), ctx).ok()
                .and_then(|v| v.as_boolean()).unwrap_or(false);
            Some(UiComponent::LayerList { sync_selection: sync })
        }
        _ => None,
    }
}

/// Parse a JavaScript object into a UiMessage.
///
/// Expects objects with a `type` field:
/// - `{ type: "updateValue", key: "x", value: 42 }`
/// - `{ type: "notification", message: "Saved!", level: "success" }`
/// - `{ type: "setTitle", title: "New Title" }`
/// - `{ type: "custom", kind: "myEvent", data: { ... } }`
fn parse_js_ui_message(val: &JsValue, ctx: &mut Context) -> Result<UiMessage, boa_engine::JsError> {
    let obj = val.as_object()
        .ok_or_else(|| js_error("message must be an object"))?;

    let type_str = obj.get(JsString::from("type"), ctx)
        .map_err(|_| js_error("message must have a 'type' field"))?
        .to_string(ctx)
        .map_err(|_| js_error("message type must be a string"))?
        .to_std_string_escaped();

    match type_str.as_str() {
        "updateValue" => {
            let key = js_obj_string(&obj, "key", ctx)
                .ok_or_else(|| js_error("updateValue requires 'key'"))?;
            let value = obj.get(JsString::from("value"), ctx)
                .map(|v| js_value_to_ui_value(&v, ctx))
                .unwrap_or(UiValue::Null);
            Ok(UiMessage::UpdateValue { key, value })
        }
        "notification" => {
            let message = js_obj_string(&obj, "message", ctx)
                .ok_or_else(|| js_error("notification requires 'message'"))?;
            let level_str = js_obj_string(&obj, "level", ctx)
                .unwrap_or_else(|| "info".to_string());
            let level = match level_str.as_str() {
                "warning" => crate::engine::ui::NotificationLevel::Warning,
                "error" => crate::engine::ui::NotificationLevel::Error,
                "success" => crate::engine::ui::NotificationLevel::Success,
                _ => crate::engine::ui::NotificationLevel::Info,
            };
            Ok(UiMessage::ShowNotification { message, level })
        }
        "setTitle" => {
            let title = js_obj_string(&obj, "title", ctx)
                .ok_or_else(|| js_error("setTitle requires 'title'"))?;
            Ok(UiMessage::SetTitle { title })
        }
        "custom" => {
            let kind = js_obj_string(&obj, "kind", ctx)
                .unwrap_or_else(|| "unknown".to_string());
            let mut data = HashMap::new();
            if let Ok(data_val) = obj.get(JsString::from("data"), ctx) {
                if let Some(data_obj) = data_val.as_object() {
                    if let Ok(keys) = data_obj.own_property_keys(ctx) {
                        for pk in keys {
                            let k_str = match &pk {
                                boa_engine::property::PropertyKey::String(s) => {
                                    s.to_std_string_escaped()
                                }
                                boa_engine::property::PropertyKey::Index(idx) => {
                                    idx.get().to_string()
                                }
                                _ => continue, // skip symbols
                            };
                            if let Ok(v) = data_obj.get(pk, ctx) {
                                data.insert(k_str, js_value_to_ui_value(&v, ctx));
                            }
                        }
                    }
                }
            }
            Ok(UiMessage::Custom { kind, data })
        }
        _ => Err(js_error(&format!("unknown message type: {type_str}"))),
    }
}

/// Convert a JsValue to a UiValue.
fn js_value_to_ui_value(val: &JsValue, ctx: &mut Context) -> UiValue {
    if val.is_null_or_undefined() {
        UiValue::Null
    } else if let Some(b) = val.as_boolean() {
        UiValue::Bool(b)
    } else if let Some(n) = val.as_number() {
        UiValue::Number(n)
    } else if let Ok(s) = val.to_string(ctx) {
        // Check if it's actually a string type (not a number/bool coerced to string)
        if val.is_string() {
            UiValue::String(s.to_std_string_escaped())
        } else {
            // It was coerced — treat as string fallback
            UiValue::String(s.to_std_string_escaped())
        }
    } else {
        UiValue::Null
    }
}

/// Extract a string property from a JS object.
fn js_obj_string(obj: &boa_engine::JsObject, key: &str, ctx: &mut Context) -> Option<String> {
    obj.get(JsString::from(key), ctx)
        .ok()
        .and_then(|v| {
            if v.is_null_or_undefined() {
                None
            } else {
                v.to_string(ctx).ok().map(|s| s.to_std_string_escaped())
            }
        })
}

/// Extract a f64 property from a JS object.
fn js_obj_f64(obj: &boa_engine::JsObject, key: &str, ctx: &mut Context) -> Option<f64> {
    obj.get(JsString::from(key), ctx)
        .ok()
        .and_then(|v| v.as_number())
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

    // ═══════════ Day 20: Path API Tests ═══════════

    #[test]
    fn test_js_create_path_line() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "lineTo", x: 100, y: 100 },
                { type: "lineTo", x: 200, y: 0 },
                { type: "close" }
            ]);
        "#;
        let result = engine.execute(code).unwrap();
        assert!(result.as_str().is_some(), "should return UUID string");
        assert!(result.as_str().unwrap().len() > 10);
    }

    #[test]
    fn test_js_create_path_bezier() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "bezierTo", cp1x: 50, cp1y: -50, cp2x: 150, cp2y: -50, x: 200, y: 0 }
            ]);
        "#;
        let result = engine.execute(code).unwrap();
        assert!(result.as_str().is_some());

        let count = engine.execute("Logos.getLayerCount()").unwrap();
        assert_eq!(count.as_int(), Some(1));
    }

    #[test]
    fn test_js_create_path_quad() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "quadTo", cx: 100, cy: 200, x: 200, y: 0 }
            ]);
        "#;
        let result = engine.execute(code).unwrap();
        assert!(result.as_str().is_some());
    }

    #[test]
    fn test_js_get_path_layer() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            var pathId = Logos.createPath([
                { type: "moveTo", x: 10, y: 20 },
                { type: "lineTo", x: 110, y: 120 }
            ]);
            Logos.getLayer(pathId);
        "#;
        let result = engine.execute(code).unwrap();
        if let PluginValue::Object(ref obj) = result {
            assert_eq!(obj.get("type").and_then(|v: &PluginValue| v.as_str()), Some("path"));
            assert!(obj.contains_key("commandCount"));
        } else {
            panic!("expected object, got {:?}", result);
        }
    }

    #[test]
    fn test_js_create_path_empty_errors() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.createPath([])");
        assert!(result.is_err(), "empty path should error");
    }

    #[test]
    fn test_js_create_path_unknown_type_errors() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute(r#"Logos.createPath([{ type: "invalid", x: 0, y: 0 }])"#);
        assert!(result.is_err(), "unknown command type should error");
    }

    #[test]
    fn test_js_create_path_permission_denied() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        let code = r#"Logos.createPath([{ type: "moveTo", x: 0, y: 0 }])"#;
        assert!(engine.execute(code).is_err(), "read-only should deny createPath");
    }

    #[test]
    fn test_js_create_path_complex_shape() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        // A complex shape: heart-like bezier curve
        let code = r#"
            Logos.createPath([
                { type: "moveTo", x: 100, y: 200 },
                { type: "bezierTo", cp1x: 100, cp1y: 100, cp2x: 0, cp2y: 100, x: 0, y: 200 },
                { type: "bezierTo", cp1x: 0, cp1y: 300, cp2x: 100, cp2y: 400, x: 100, y: 500 },
                { type: "bezierTo", cp1x: 100, cp1y: 400, cp2x: 200, cp2y: 300, x: 200, y: 200 },
                { type: "bezierTo", cp1x: 200, cp1y: 100, cp2x: 100, cp2y: 100, x: 100, y: 200 },
                { type: "close" }
            ]);
        "#;
        let result = engine.execute(code).unwrap();
        assert!(result.as_str().is_some());

        let layers = engine.execute("Logos.getLayers()").unwrap();
        if let PluginValue::Array(arr) = layers {
            assert_eq!(arr.len(), 1);
            if let PluginValue::Object(ref obj) = arr[0] {
                assert_eq!(obj.get("type").and_then(|v: &PluginValue| v.as_str()), Some("path"));
                assert_eq!(obj.get("commandCount").and_then(|v: &PluginValue| v.as_int()), Some(6));
            }
        }
    }

    // ═══════════ Day 20: Selection API Tests ═══════════

    #[test]
    fn test_js_get_selection_empty() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.getSelection()").unwrap();
        if let PluginValue::Array(arr) = result {
            assert!(arr.is_empty(), "initial selection should be empty");
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_js_set_and_get_selection() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            var id1 = Logos.createRect(0, 0, 50, 50);
            var id2 = Logos.createRect(100, 100, 50, 50);
            Logos.setSelection([id1, id2]);
            Logos.getSelection();
        "#;
        let result = engine.execute(code).unwrap();
        if let PluginValue::Array(arr) = result {
            assert_eq!(arr.len(), 2, "should have 2 selected items");
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_js_clear_selection() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            var id = Logos.createRect(0, 0, 50, 50);
            Logos.setSelection([id]);
            Logos.clearSelection();
            Logos.getSelection();
        "#;
        let result = engine.execute(code).unwrap();
        if let PluginValue::Array(arr) = result {
            assert!(arr.is_empty(), "selection should be cleared");
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_js_selection_permission_read() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        // getSelection should work with read-only
        let result = engine.execute("Logos.getSelection()");
        assert!(result.is_ok(), "getSelection should work with read-only");
    }

    #[test]
    fn test_js_selection_permission_write_denied() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        // setSelection should be denied with read-only
        let result = engine.execute("Logos.setSelection([])");
        assert!(result.is_err(), "setSelection should be denied with read-only");
    }

    #[test]
    fn test_js_clear_selection_permission_denied() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        let result = engine.execute("Logos.clearSelection()");
        assert!(result.is_err(), "clearSelection should be denied with read-only");
    }

    #[test]
    fn test_js_set_selection_invalid_uuid() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.setSelection(['not-a-uuid'])");
        assert!(result.is_err(), "invalid UUID should error");
    }

    // ═══════════ Day 20: Undo/Redo Tests ═══════════

    #[test]
    fn test_js_undo_create_rect() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute("Logos.createRect(0, 0, 50, 50)").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));

        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(0));
    }

    #[test]
    fn test_js_undo_redo_cycle() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute("Logos.createRect(10, 20, 100, 50)").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));

        // Undo → 0 layers
        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(0));

        // Redo → 1 layer
        engine.execute("Logos.redo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));
    }

    #[test]
    fn test_js_undo_empty_returns_false() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.undo()").unwrap();
        assert_eq!(result.as_bool(), Some(false), "undo on empty stack should return false");
    }

    #[test]
    fn test_js_redo_empty_returns_false() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute("Logos.redo()").unwrap();
        assert_eq!(result.as_bool(), Some(false), "redo on empty stack should return false");
    }

    #[test]
    fn test_js_undo_delete_layer() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute("var id = Logos.createRect(0, 0, 50, 50)").unwrap();
        engine.execute("Logos.deleteLayer(id)").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(0));

        // Undo the delete → layer should reappear
        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));
    }

    #[test]
    fn test_js_undo_multiple_actions() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute("Logos.createRect(0, 0, 50, 50)").unwrap();
        engine.execute("Logos.createRect(100, 100, 50, 50)").unwrap();
        engine.execute("Logos.createRect(200, 200, 50, 50)").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(3));

        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(2));

        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));

        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(0));
    }

    #[test]
    fn test_js_undo_permission_denied() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::read_only());

        assert!(engine.execute("Logos.undo()").is_err());
        assert!(engine.execute("Logos.redo()").is_err());
    }

    #[test]
    fn test_js_undo_path() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "lineTo", x: 100, y: 100 }
            ]);
        "#;
        engine.execute(code).unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));

        engine.execute("Logos.undo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(0));

        engine.execute("Logos.redo()").unwrap();
        assert_eq!(engine.execute("Logos.getLayerCount()").unwrap().as_int(), Some(1));
    }

    // ═══════════ Day 20: Event System Tests ═══════════

    #[test]
    fn test_js_on_event_registration() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.on("selectionChanged", function(e) {
                globalThis.__lastEvent = e.event;
            });
            true;
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn test_js_on_invalid_event_name() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute(r#"Logos.on("nonExistentEvent", function() {})"#);
        assert!(result.is_err(), "unknown event should error");
    }

    #[test]
    fn test_js_on_non_function_callback() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let result = engine.execute(r#"Logos.on("selectionChanged", "not a function")"#);
        assert!(result.is_err(), "non-function callback should error");
    }

    #[test]
    fn test_js_event_dispatch_selection() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        // Register callback
        engine.execute(r#"
            globalThis.__eventCount = 0;
            Logos.on("selectionChanged", function(e) {
                globalThis.__eventCount++;
            });
        "#).unwrap();

        // Trigger selection change (emits event)
        engine.execute("var id = Logos.createRect(0, 0, 50, 50)").unwrap();
        engine.execute("Logos.setSelection([id])").unwrap();

        // Flush events
        let invoked = engine.flush_events();
        assert!(invoked >= 1, "at least one callback should have been invoked");

        let count = engine.execute("globalThis.__eventCount").unwrap();
        assert!(count.as_int().unwrap_or(0) >= 1, "event handler should have been called");
    }

    #[test]
    fn test_js_event_dispatch_layer_added() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute(r#"
            globalThis.__addedIds = [];
            Logos.on("layerAdded", function(e) {
                globalThis.__addedIds.push(e.id);
            });
        "#).unwrap();

        engine.execute("Logos.createRect(0, 0, 50, 50)").unwrap();
        engine.execute("Logos.createRect(100, 100, 50, 50)").unwrap();

        engine.flush_events();

        let result = engine.execute("globalThis.__addedIds.length").unwrap();
        // Due to rate limiting, at least 1 should fire (maybe not 2)
        assert!(result.as_int().unwrap_or(0) >= 1, "at least one layerAdded event should fire");
    }

    #[test]
    fn test_js_event_dispatch_layer_removed() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        engine.execute(r#"
            globalThis.__removedCount = 0;
            Logos.on("layerRemoved", function(e) {
                globalThis.__removedCount++;
            });
        "#).unwrap();

        engine.execute("var id = Logos.createRect(0, 0, 50, 50)").unwrap();
        engine.flush_events(); // flush layerAdded

        // Small delay to pass rate limiter
        std::thread::sleep(std::time::Duration::from_millis(20));

        engine.execute("Logos.deleteLayer(id)").unwrap();
        engine.flush_events();

        let count = engine.execute("globalThis.__removedCount").unwrap();
        assert_eq!(count.as_int(), Some(1), "layerRemoved event should fire");
    }

    // ═══════════ Day 20: Integration Tests ═══════════

    #[test]
    fn test_js_full_workflow_path_select_undo() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            // Create a triangle path
            var pathId = Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "lineTo", x: 100, y: 200 },
                { type: "lineTo", x: 200, y: 0 },
                { type: "close" }
            ]);

            // Create a rect
            var rectId = Logos.createRect(50, 50, 100, 100);

            // Select both
            Logos.setSelection([pathId, rectId]);

            // Verify
            var sel = Logos.getSelection();
            var count = Logos.getLayerCount();

            // Undo rect creation
            Logos.undo();

            var count2 = Logos.getLayerCount();
            count + ":" + count2;
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_str(), Some("2:1"));
    }

    #[test]
    fn test_js_mixed_layers_query() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            Logos.createRect(0, 0, 50, 50);
            Logos.createPath([
                { type: "moveTo", x: 0, y: 0 },
                { type: "bezierTo", cp1x: 50, cp1y: -50, cp2x: 150, cp2y: -50, x: 200, y: 0 }
            ]);
            Logos.createRect(100, 100, 50, 50);

            var layers = Logos.getLayers();
            var types = [];
            for (var i = 0; i < layers.length; i++) {
                types.push(layers[i].type);
            }
            types.join(",");
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_str(), Some("rect,path,rect"));
    }

    #[test]
    fn test_js_delete_and_undo_preserves_path() {
        let doc = Arc::new(RwLock::new(Document::new()));
        let mut engine = engine_with_doc(Arc::clone(&doc), PermissionSet::document_full());

        let code = r#"
            var pathId = Logos.createPath([
                { type: "moveTo", x: 10, y: 20 },
                { type: "lineTo", x: 110, y: 120 },
                { type: "lineTo", x: 210, y: 20 },
                { type: "close" }
            ]);

            Logos.deleteLayer(pathId);
            Logos.undo(); // undo delete → path reappears

            var layer = Logos.getLayer(pathId);
            layer.type;
        "#;
        let result = engine.execute(code).unwrap();
        assert_eq!(result.as_str(), Some("path"));
    }
}
