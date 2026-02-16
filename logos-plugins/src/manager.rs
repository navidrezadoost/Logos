//! Plugin lifecycle manager.
//!
//! Handles loading, starting, stopping, and unloading plugins.
//! Manages the registry of all installed plugins and their states.
//!
//! Architecture:
//! ```text
//! PluginManager
//!   ├── PluginInstance("auto-grid")  → Running
//!   ├── PluginInstance("color-gen")  → Loaded
//!   └── PluginInstance("hello")      → Stopped
//!
//! PluginInstance
//!   ├── manifest: PluginManifest
//!   ├── sandbox: Sandbox
//!   ├── host: PluginHost
//!   └── state: PluginState
//! ```

use crate::engine::JsEngine;
use crate::engine::WasmRuntime;
use crate::host::PluginHost;
use crate::manifest::PluginManifest;
use crate::runtime::{PluginValue, ResourceLimits, RuntimeError, RuntimeResult, Sandbox};
use logos_core::Document;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// State of a plugin instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    /// Manifest loaded, sandbox not yet created
    Loaded,
    /// Sandbox created, ready to execute
    Ready,
    /// Currently executing
    Running,
    /// Stopped (can be restarted)
    Stopped,
    /// Error state (must be reloaded)
    Error(String),
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loaded => write!(f, "loaded"),
            Self::Ready => write!(f, "ready"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Error(e) => write!(f, "error: {e}"),
        }
    }
}

/// Backend runtime for a plugin instance.
///
/// - `Sandbox`: Day 18 placeholder expression evaluator (for non-JS plugins)
/// - `JavaScript`: Real boa_engine ES2023 runtime (Day 19)
/// - `Wasm`: Wasmtime WebAssembly runtime with fuel metering
pub enum PluginRuntime {
    /// Placeholder expression evaluator
    Sandbox(Sandbox),
    /// Real JavaScript engine (boa_engine ES2023)
    JavaScript(JsEngine),
    /// Wasmtime WASM sandbox with host function bridge
    Wasm(WasmRuntime),
}

/// A running plugin instance.
pub struct PluginInstance {
    /// The plugin's manifest
    pub manifest: PluginManifest,
    /// The runtime backend
    pub runtime: PluginRuntime,
    /// Current state
    pub state: PluginState,
}

impl PluginInstance {
    /// Create a new instance from manifest + sandbox (legacy).
    fn new(manifest: PluginManifest, sandbox: Sandbox) -> Self {
        Self {
            manifest,
            runtime: PluginRuntime::Sandbox(sandbox),
            state: PluginState::Ready,
        }
    }

    /// Create a new instance from manifest + JsEngine.
    fn new_js(manifest: PluginManifest, engine: JsEngine) -> Self {
        Self {
            manifest,
            runtime: PluginRuntime::JavaScript(engine),
            state: PluginState::Ready,
        }
    }

    /// Create a new instance from manifest + WasmRuntime.
    fn new_wasm(manifest: PluginManifest, wasm_rt: WasmRuntime) -> Self {
        Self {
            manifest,
            runtime: PluginRuntime::Wasm(wasm_rt),
            state: PluginState::Ready,
        }
    }

    /// Execute a script in this plugin's runtime.
    pub fn execute(&mut self, script: &str) -> RuntimeResult<PluginValue> {
        if self.state == PluginState::Stopped {
            return Err(RuntimeError::ExecutionError(
                "plugin is stopped".to_string(),
            ));
        }
        if let PluginState::Error(ref e) = self.state {
            return Err(RuntimeError::ExecutionError(format!(
                "plugin is in error state: {e}"
            )));
        }
        self.state = PluginState::Running;
        let result = match &mut self.runtime {
            PluginRuntime::Sandbox(sb) => sb.execute(script),
            PluginRuntime::JavaScript(engine) => engine.execute(script),
            PluginRuntime::Wasm(wasm_rt) => wasm_rt.execute(script),
        };
        self.state = match &result {
            Ok(_) => PluginState::Ready,
            Err(e) => PluginState::Error(e.to_string()),
        };
        result
    }

    /// Stop this plugin.
    pub fn stop(&mut self) {
        match &mut self.runtime {
            PluginRuntime::Sandbox(sb) => sb.kill(),
            PluginRuntime::JavaScript(engine) => engine.kill(),
            PluginRuntime::Wasm(wasm_rt) => wasm_rt.kill(),
        }
        self.state = PluginState::Stopped;
    }
}

/// Plugin manager — registry and lifecycle coordinator.
///
/// Manages all plugin instances, handles loading from manifests,
/// creating sandboxes, wiring host functions, and state transitions.
pub struct PluginManager {
    /// Registered plugin instances by plugin ID
    plugins: HashMap<String, PluginInstance>,
    /// The active document (shared with hosts)
    document: Arc<RwLock<Document>>,
    /// Maximum number of simultaneous plugins
    max_plugins: usize,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new(document: Arc<RwLock<Document>>) -> Self {
        Self {
            plugins: HashMap::new(),
            document,
            max_plugins: 64,
        }
    }

    /// Set the maximum number of simultaneous plugins.
    pub fn set_max_plugins(&mut self, max: usize) {
        self.max_plugins = max;
    }

    /// Load a plugin from its manifest.
    ///
    /// For `.js` entry points, uses the real JavaScript engine (boa_engine).
    /// For `.wasm` entry points, uses the Wasmtime WASM runtime.
    /// For other entry points, uses the placeholder Sandbox evaluator.
    pub fn load(&mut self, manifest: PluginManifest) -> RuntimeResult<()> {
        // Validate manifest
        manifest
            .validate()
            .map_err(|e| RuntimeError::CompileError(e))?;

        // Check capacity
        if self.plugins.len() >= self.max_plugins {
            return Err(RuntimeError::ExecutionError(format!(
                "maximum plugin count ({}) reached",
                self.max_plugins
            )));
        }

        let plugin_id = manifest.id.to_string();

        // Check for duplicate
        if self.plugins.contains_key(&plugin_id) {
            return Err(RuntimeError::ExecutionError(format!(
                "plugin already loaded: {}",
                manifest.id
            )));
        }

        let limits = manifest
            .max_execution_time
            .map(|t| {
                let mut l = ResourceLimits::default();
                l.max_execution_time = t;
                l
            })
            .unwrap_or_default();

        let use_js = manifest.entry_point.ends_with(".js");
        let use_wasm = manifest.entry_point.ends_with(".wasm");

        let instance = if use_wasm {
            // Wasmtime WASM runtime
            let mut wasm_rt = WasmRuntime::new(&manifest.name, limits, manifest.permissions.clone())?;
            wasm_rt.register_document(Arc::clone(&self.document));
            PluginInstance::new_wasm(manifest, wasm_rt)
        } else if use_js {
            // Real JavaScript engine (Day 19)
            let mut engine = JsEngine::new(&manifest.name, limits, manifest.permissions.clone());
            engine.register_document(Arc::clone(&self.document));
            PluginInstance::new_js(manifest, engine)
        } else {
            // Legacy placeholder sandbox (Day 18)
            let mut sandbox = Sandbox::new(&manifest.name, limits);
            let host = PluginHost::new(
                Arc::clone(&self.document),
                manifest.permissions.clone(),
            );
            host.register_host_fns(&mut sandbox);
            PluginInstance::new(manifest, sandbox)
        };

        self.plugins.insert(plugin_id, instance);
        Ok(())
    }

    /// Unload a plugin by ID.
    pub fn unload(&mut self, plugin_id: &str) -> RuntimeResult<()> {
        if let Some(mut instance) = self.plugins.remove(plugin_id) {
            instance.stop();
            Ok(())
        } else {
            Err(RuntimeError::NotFound(format!(
                "plugin not found: {plugin_id}"
            )))
        }
    }

    /// Execute a script in a specific plugin's sandbox.
    pub fn execute(&mut self, plugin_id: &str, script: &str) -> RuntimeResult<PluginValue> {
        let instance = self.plugins.get_mut(plugin_id).ok_or_else(|| {
            RuntimeError::NotFound(format!("plugin not found: {plugin_id}"))
        })?;
        instance.execute(script)
    }

    /// Stop a plugin (but keep it loaded).
    pub fn stop(&mut self, plugin_id: &str) -> RuntimeResult<()> {
        let instance = self.plugins.get_mut(plugin_id).ok_or_else(|| {
            RuntimeError::NotFound(format!("plugin not found: {plugin_id}"))
        })?;
        instance.stop();
        Ok(())
    }

    /// Get a plugin's current state.
    pub fn state(&self, plugin_id: &str) -> Option<&PluginState> {
        self.plugins.get(plugin_id).map(|p| &p.state)
    }

    /// List all loaded plugin IDs.
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Count of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Get a plugin instance by ID.
    pub fn get(&self, plugin_id: &str) -> Option<&PluginInstance> {
        self.plugins.get(plugin_id)
    }

    /// Stop all plugins and clear the registry.
    pub fn shutdown(&mut self) {
        for (_, instance) in self.plugins.iter_mut() {
            instance.stop();
        }
        self.plugins.clear();
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::permissions::PermissionSet;

    fn test_document() -> Arc<RwLock<Document>> {
        Arc::new(RwLock::new(Document::new()))
    }

    fn hello_manifest() -> PluginManifest {
        PluginManifest::new("Hello Plugin")
            .with_entry_point("hello.js")
            .with_permissions(PermissionSet::read_only())
    }

    fn write_manifest() -> PluginManifest {
        PluginManifest::new("Writer Plugin")
            .with_entry_point("writer.js")
            .with_permissions(PermissionSet::document_full())
    }

    #[test]
    fn test_load_plugin() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        assert!(mgr.load(hello_manifest()).is_ok());
        assert_eq!(mgr.plugin_count(), 1);
    }

    #[test]
    fn test_load_duplicate() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let dup = manifest.clone();
        mgr.load(manifest).unwrap();
        assert!(mgr.load(dup).is_err());
    }

    #[test]
    fn test_load_max_capacity() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        mgr.set_max_plugins(1);
        mgr.load(hello_manifest()).unwrap();
        assert!(mgr.load(write_manifest()).is_err());
    }

    #[test]
    fn test_unload_plugin() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();
        assert!(mgr.unload(&id).is_ok());
        assert_eq!(mgr.plugin_count(), 0);
    }

    #[test]
    fn test_unload_nonexistent() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        assert!(mgr.unload("nope").is_err());
    }

    #[test]
    fn test_execute_plugin() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        let result = mgr.execute(&id, "42").unwrap();
        assert_eq!(result, PluginValue::Int(42));
    }

    #[test]
    fn test_execute_with_host_fn() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        let result = mgr.execute(&id, "Logos.getLayerCount()").unwrap();
        assert_eq!(result, PluginValue::Int(0));
    }

    #[test]
    fn test_create_rect_via_manager() {
        let doc = test_document();
        let mut mgr = PluginManager::new(Arc::clone(&doc));
        let manifest = write_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        // Create a rect via JS Logos API
        mgr.execute(&id, "Logos.createRect(10, 20, 100, 50)")
            .unwrap();

        // Verify via layer count
        let count = mgr.execute(&id, "Logos.getLayerCount()").unwrap();
        assert_eq!(count, PluginValue::Int(1));
    }

    #[test]
    fn test_plugin_state_transitions() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        assert_eq!(mgr.state(&id), Some(&PluginState::Ready));

        mgr.execute(&id, "42").unwrap();
        assert_eq!(mgr.state(&id), Some(&PluginState::Ready));

        mgr.stop(&id).unwrap();
        assert_eq!(mgr.state(&id), Some(&PluginState::Stopped));
    }

    #[test]
    fn test_stopped_plugin_rejects_execution() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = hello_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        mgr.stop(&id).unwrap();
        assert!(mgr.execute(&id, "42").is_err());
    }

    #[test]
    fn test_list_plugins() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        mgr.load(hello_manifest()).unwrap();
        mgr.load(write_manifest()).unwrap();

        let list = mgr.list_plugins();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_shutdown() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        mgr.load(hello_manifest()).unwrap();
        mgr.load(write_manifest()).unwrap();
        mgr.shutdown();
        assert_eq!(mgr.plugin_count(), 0);
    }

    #[test]
    fn test_plugin_state_display() {
        assert_eq!(PluginState::Loaded.to_string(), "loaded");
        assert_eq!(PluginState::Ready.to_string(), "ready");
        assert_eq!(PluginState::Running.to_string(), "running");
        assert_eq!(PluginState::Stopped.to_string(), "stopped");
        assert_eq!(
            PluginState::Error("boom".to_string()).to_string(),
            "error: boom"
        );
    }

    #[test]
    fn test_invalid_manifest_rejected() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        // Missing entry_point → validation fails
        let manifest = PluginManifest::new("Bad Plugin");
        assert!(mgr.load(manifest).is_err());
    }

    // ─── JavaScript Engine Integration Tests ───

    fn js_manifest() -> PluginManifest {
        PluginManifest::new("JS Plugin")
            .with_entry_point("main.js")
            .with_permissions(PermissionSet::document_full())
    }

    #[test]
    fn test_load_js_plugin() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = js_manifest();
        assert!(mgr.load(manifest).is_ok());
        assert_eq!(mgr.plugin_count(), 1);
    }

    #[test]
    fn test_js_plugin_uses_js_engine() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = js_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        // Should be a JsEngine backend
        let instance = mgr.get(&id).unwrap();
        assert!(matches!(instance.runtime, PluginRuntime::JavaScript(_)));
    }

    #[test]
    fn test_js_plugin_execute_arithmetic() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = js_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        let result = mgr.execute(&id, "2 + 3").unwrap();
        assert_eq!(result, PluginValue::Int(5));
    }

    #[test]
    fn test_js_plugin_host_api() {
        let doc = test_document();
        let mut mgr = PluginManager::new(Arc::clone(&doc));
        let manifest = js_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        // Create a rect via JS Logos API
        mgr.execute(&id, "Logos.createRect(10, 20, 100, 50)")
            .unwrap();

        let count = mgr.execute(&id, "Logos.getLayerCount()").unwrap();
        assert_eq!(count, PluginValue::Int(1));
    }

    #[test]
    fn test_js_plugin_es2023_features() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        let manifest = js_manifest();
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        // Arrow functions, template literals, destructuring
        let code = r#"
            const nums = [1, 2, 3, 4, 5];
            const sum = nums.reduce((a, b) => a + b, 0);
            sum;
        "#;
        let result = mgr.execute(&id, code).unwrap();
        assert_eq!(result, PluginValue::Int(15));
    }

    #[test]
    fn test_legacy_sandbox_still_works() {
        let doc = test_document();
        let mut mgr = PluginManager::new(doc);
        // .pjs extension → Sandbox (not .js)
        let manifest = PluginManifest::new("Legacy Plugin")
            .with_entry_point("main.pjs")
            .with_permissions(PermissionSet::read_only());
        let id = manifest.id.to_string();
        mgr.load(manifest).unwrap();

        let instance = mgr.get(&id).unwrap();
        assert!(matches!(instance.runtime, PluginRuntime::Sandbox(_)));
    }
}
