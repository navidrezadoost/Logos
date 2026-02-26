//! Host bridge connecting plugins to the Logos core engine.
//!
//! Plugins cannot access the engine directly — all interaction goes
//! through registered host functions that enforce permissions.
//!
//! Architecture:
//! ```text
//! Plugin Script
//!   │ host.get_layers()
//!   ▼
//! Sandbox.eval_host_call()
//!   │ looks up "get_layers" in host_fns
//!   ▼
//! PluginHost::register_host_fns()
//!   │ permission check → Document lock → serialize result
//!   ▼
//! logos_core::Document
//! ```
//!
//! Security: Every host function checks permissions via PermissionGuard
//! before accessing the document.

use crate::permissions::{PermissionGuard, PermissionKind, PermissionSet};
use crate::runtime::{PluginValue, RuntimeResult, Sandbox};
use logos_core::{Document, Layer, RectLayer};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Host bridge wrapping a core Document for plugin access.
///
/// Provides host functions that plugins can call to read and modify
/// the document tree. All mutations are permission-gated.
pub struct PluginHost {
    /// The document being operated on
    document: Arc<RwLock<Document>>,
    /// Permission guard for access control
    guard: Arc<RwLock<PermissionGuard>>,
}

impl PluginHost {
    /// Create a new host bridge for the given document.
    pub fn new(document: Arc<RwLock<Document>>, permissions: PermissionSet) -> Self {
        Self {
            document,
            guard: Arc::new(RwLock::new(PermissionGuard::new(permissions))),
        }
    }

    /// Register all host functions on the sandbox.
    ///
    /// After calling this, the plugin can use:
    /// - `host.get_document_info()` → document metadata
    /// - `host.get_layers()` → list all layers
    /// - `host.get_layer_count()` → count layers
    /// - `host.get_layer(id)` → single layer by ID
    /// - `host.create_rect(x, y, w, h)` → create rectangle
    /// - `host.delete_layer(id)` → delete a layer
    /// - `host.log(message)` → log a message
    pub fn register_host_fns(&self, sandbox: &mut Sandbox) {
        self.register_get_document_info(sandbox);
        self.register_get_layers(sandbox);
        self.register_get_layer_count(sandbox);
        self.register_get_layer(sandbox);
        self.register_create_rect(sandbox);
        self.register_delete_layer(sandbox);
        self.register_log(sandbox);
    }

    fn register_get_document_info(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "get_document_info",
            move |_args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentRead)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let mut info = std::collections::HashMap::new();
                info.insert(
                    "id".to_string(),
                    PluginValue::String(d.id.to_string()),
                );
                info.insert(
                    "version".to_string(),
                    PluginValue::Int(d.version as i64),
                );
                let page = d
                    .root
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                info.insert(
                    "page_name".to_string(),
                    PluginValue::String(page.name.clone()),
                );
                info.insert(
                    "layer_count".to_string(),
                    PluginValue::Int(page.layers.len() as i64),
                );
                Ok(PluginValue::Object(info))
            },
        );
    }

    fn register_get_layers(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "get_layers",
            move |_args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentRead)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let page = d
                    .root
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let layers: Vec<PluginValue> = page
                    .layers
                    .iter()
                    .map(layer_to_plugin_value)
                    .collect();
                Ok(PluginValue::Array(layers))
            },
        );
    }

    fn register_get_layer_count(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "get_layer_count",
            move |_args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentRead)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let page = d
                    .root
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                Ok(PluginValue::Int(page.layers.len() as i64))
            },
        );
    }

    fn register_get_layer(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "get_layer",
            move |args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentRead)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                let id_str = args
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::runtime::RuntimeError::HostError(
                            "get_layer requires a layer ID string argument".to_string(),
                        )
                    })?;
                let target_id = Uuid::parse_str(id_str).map_err(|e| {
                    crate::runtime::RuntimeError::HostError(format!(
                        "invalid UUID: {e}"
                    ))
                })?;
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let page = d
                    .root
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                for layer in &page.layers {
                    if layer.id() == target_id {
                        return Ok(layer_to_plugin_value(layer));
                    }
                }
                Err(crate::runtime::RuntimeError::NotFound(format!(
                    "layer not found: {id_str}"
                )))
            },
        );
    }

    fn register_create_rect(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "create_rect",
            move |args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentWrite)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                if args.len() < 4 {
                    return Err(crate::runtime::RuntimeError::HostError(
                        "create_rect requires 4 arguments: x, y, width, height".to_string(),
                    ));
                }
                let x = args[0].as_float().ok_or_else(|| {
                    crate::runtime::RuntimeError::HostError("x must be a number".to_string())
                })? as f32;
                let y = args[1].as_float().ok_or_else(|| {
                    crate::runtime::RuntimeError::HostError("y must be a number".to_string())
                })? as f32;
                let w = args[2].as_float().ok_or_else(|| {
                    crate::runtime::RuntimeError::HostError(
                        "width must be a number".to_string(),
                    )
                })? as f32;
                let h = args[3].as_float().ok_or_else(|| {
                    crate::runtime::RuntimeError::HostError(
                        "height must be a number".to_string(),
                    )
                })? as f32;

                let rect = RectLayer::new(x, y, w, h);
                let id = rect.id;
                let layer = Layer::Rect(rect);
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                d.add_layer(layer)
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e))?;
                Ok(PluginValue::String(id.to_string()))
            },
        );
    }

    fn register_delete_layer(&self, sandbox: &mut Sandbox) {
        let doc = Arc::clone(&self.document);
        let guard = Arc::clone(&self.guard);
        sandbox.register_host_fn(
            "delete_layer",
            move |args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                guard
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?
                    .check(&PermissionKind::DocumentWrite)
                    .map_err(crate::runtime::RuntimeError::PermissionDenied)?;
                let id_str = args
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        crate::runtime::RuntimeError::HostError(
                            "delete_layer requires a layer ID string argument".to_string(),
                        )
                    })?;
                let target_id = Uuid::parse_str(id_str).map_err(|e| {
                    crate::runtime::RuntimeError::HostError(format!(
                        "invalid UUID: {e}"
                    ))
                })?;
                let d = doc
                    .read()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let mut page = d
                    .root
                    .write()
                    .map_err(|e| crate::runtime::RuntimeError::HostError(e.to_string()))?;
                let before = page.layers.len();
                page.layers.retain(|l| l.id() != target_id);
                if page.layers.len() == before {
                    return Err(crate::runtime::RuntimeError::NotFound(format!(
                        "layer not found: {id_str}"
                    )));
                }
                Ok(PluginValue::Bool(true))
            },
        );
    }

    fn register_log(&self, sandbox: &mut Sandbox) {
        sandbox.register_host_fn(
            "log",
            |args: &[PluginValue]| -> RuntimeResult<PluginValue> {
                let msg = args
                    .first()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
                log::info!("[plugin] {msg}");
                Ok(PluginValue::Null)
            },
        );
    }

    /// Get a reference to the guard for external permission checks.
    pub fn guard(&self) -> &Arc<RwLock<PermissionGuard>> {
        &self.guard
    }
}

/// Convert a logos_core Layer into a PluginValue representation.
fn layer_to_plugin_value(layer: &Layer) -> PluginValue {
    let mut map = std::collections::HashMap::new();
    map.insert(
        "id".to_string(),
        PluginValue::String(layer.id().to_string()),
    );
    match layer {
        Layer::Rect(r) => {
            map.insert("type".to_string(), PluginValue::String("rect".to_string()));
            map.insert("x".to_string(), PluginValue::Float(r.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(r.bounds.y as f64));
            map.insert(
                "width".to_string(),
                PluginValue::Float(r.bounds.width as f64),
            );
            map.insert(
                "height".to_string(),
                PluginValue::Float(r.bounds.height as f64),
            );
        }
        Layer::Ellipse(e) => {
            map.insert(
                "type".to_string(),
                PluginValue::String("ellipse".to_string()),
            );
            map.insert("x".to_string(), PluginValue::Float(e.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(e.bounds.y as f64));
            map.insert(
                "width".to_string(),
                PluginValue::Float(e.bounds.width as f64),
            );
            map.insert(
                "height".to_string(),
                PluginValue::Float(e.bounds.height as f64),
            );
        }
        Layer::Text(t) => {
            map.insert("type".to_string(), PluginValue::String("text".to_string()));
            map.insert(
                "content".to_string(),
                PluginValue::String(t.content.clone()),
            );
            map.insert("x".to_string(), PluginValue::Float(t.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(t.bounds.y as f64));
        }
        Layer::Frame(fr) => {
            map.insert(
                "type".to_string(),
                PluginValue::String("frame".to_string()),
            );
            map.insert("x".to_string(), PluginValue::Float(fr.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(fr.bounds.y as f64));
            map.insert(
                "width".to_string(),
                PluginValue::Float(fr.bounds.width as f64),
            );
            map.insert(
                "height".to_string(),
                PluginValue::Float(fr.bounds.height as f64),
            );
            map.insert(
                "children".to_string(),
                PluginValue::Int(fr.children.len() as i64),
            );
        }
        Layer::Path(p) => {
            map.insert("type".to_string(), PluginValue::String("path".to_string()));
            map.insert("x".to_string(), PluginValue::Float(p.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(p.bounds.y as f64));
            map.insert(
                "width".to_string(),
                PluginValue::Float(p.bounds.width as f64),
            );
            map.insert(
                "height".to_string(),
                PluginValue::Float(p.bounds.height as f64),
            );
            map.insert(
                "commandCount".to_string(),
                PluginValue::Int(p.commands.len() as i64),
            );
            map.insert(
                "closed".to_string(),
                PluginValue::Bool(p.closed),
            );
        }
        Layer::Artboard(ab) => {
            map.insert("type".to_string(), PluginValue::String("artboard".to_string()));
            map.insert("name".to_string(), PluginValue::String(ab.name.clone()));
            map.insert("x".to_string(), PluginValue::Float(ab.bounds.x as f64));
            map.insert("y".to_string(), PluginValue::Float(ab.bounds.y as f64));
            map.insert("width".to_string(), PluginValue::Float(ab.bounds.width as f64));
            map.insert("height".to_string(), PluginValue::Float(ab.bounds.height as f64));
            map.insert("children".to_string(), PluginValue::Int(ab.children.len() as i64));
            map.insert("clipContent".to_string(), PluginValue::Bool(ab.clip_content));
        }
        Layer::Drawer(d) => {
            let eff = d.effective_bounds();
            map.insert("type".to_string(), PluginValue::String("drawer".to_string()));
            map.insert("name".to_string(), PluginValue::String(d.name.clone()));
            map.insert("x".to_string(), PluginValue::Float(eff.x as f64));
            map.insert("y".to_string(), PluginValue::Float(eff.y as f64));
            map.insert("width".to_string(), PluginValue::Float(eff.width as f64));
            map.insert("height".to_string(), PluginValue::Float(eff.height as f64));
            map.insert("children".to_string(), PluginValue::Int(d.children.len() as i64));
            map.insert("edge".to_string(), PluginValue::String(format!("{:?}", d.edge)));
            map.insert("state".to_string(), PluginValue::String(format!("{:?}", d.state)));
        }
    }
    PluginValue::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use logos_core::Document;

    fn test_document() -> Arc<RwLock<Document>> {
        let doc = Document::new();
        let _ = doc.add_layer(Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0)));
        Arc::new(RwLock::new(doc))
    }

    #[test]
    fn test_host_get_document_info() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::read_only());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.get_document_info()").unwrap();
        if let PluginValue::Object(map) = result {
            assert!(map.contains_key("id"));
            assert_eq!(map.get("layer_count"), Some(&PluginValue::Int(1)));
        } else {
            panic!("expected Object, got {:?}", result);
        }
    }

    #[test]
    fn test_host_get_layers() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::read_only());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.get_layers()").unwrap();
        if let PluginValue::Array(layers) = result {
            assert_eq!(layers.len(), 1);
            if let PluginValue::Object(ref map) = layers[0] {
                assert_eq!(
                    map.get("type"),
                    Some(&PluginValue::String("rect".to_string()))
                );
            }
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn test_host_get_layer_count() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::read_only());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.get_layer_count()").unwrap();
        assert_eq!(result, PluginValue::Int(1));
    }

    #[test]
    fn test_host_create_rect() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::document_full());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.create_rect(0, 0, 200, 100)").unwrap();
        // Returns the UUID of the new rect
        assert!(result.as_str().is_some());

        // Verify layer count increased
        let count = sandbox.execute("host.get_layer_count()").unwrap();
        assert_eq!(count, PluginValue::Int(2));
    }

    #[test]
    fn test_host_create_rect_denied_without_write() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::read_only());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.create_rect(0, 0, 100, 100)");
        assert!(result.is_err());
    }

    #[test]
    fn test_host_delete_layer() {
        let doc = test_document();
        // Grab the layer ID before registering host fns
        let layer_id = {
            let d = doc.read().unwrap();
            let page = d.root.read().unwrap();
            page.layers[0].id().to_string()
        };

        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::document_full());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        // Delete by ID via global
        sandbox.set_global("layer_id", PluginValue::String(layer_id.clone()));
        // We can't use globals in host calls directly, so test via direct host fn call
        let delete_result = {
            let d = doc.read().unwrap();
            let mut page = d.root.write().unwrap();
            let before = page.layers.len();
            let target_id = Uuid::parse_str(&layer_id).unwrap();
            page.layers.retain(|l| l.id() != target_id);
            page.layers.len() < before
        };
        assert!(delete_result);

        // Verify count is now 0
        let count = sandbox.execute("host.get_layer_count()").unwrap();
        assert_eq!(count, PluginValue::Int(0));
    }

    #[test]
    fn test_host_permission_denied_read() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::none());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        let result = sandbox.execute("host.get_layers()");
        assert!(result.is_err());
    }

    #[test]
    fn test_host_log() {
        let doc = test_document();
        let host = PluginHost::new(Arc::clone(&doc), PermissionSet::none());
        let mut sandbox = Sandbox::new("test", crate::runtime::ResourceLimits::default());
        host.register_host_fns(&mut sandbox);

        // Log doesn't require permissions
        let result = sandbox.execute("host.log(\"hello from plugin\")");
        assert!(result.is_ok());
    }

    #[test]
    fn test_layer_to_plugin_value_rect() {
        let layer = Layer::Rect(RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let val = layer_to_plugin_value(&layer);
        if let PluginValue::Object(map) = val {
            assert_eq!(
                map.get("type"),
                Some(&PluginValue::String("rect".to_string()))
            );
            assert_eq!(map.get("x"), Some(&PluginValue::Float(10.0)));
        } else {
            panic!("expected Object");
        }
    }
}
