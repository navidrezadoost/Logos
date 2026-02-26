//! Wasmtime-based WASM plugin runtime.
//!
//! Provides true process-level isolation via WebAssembly with:
//! - Fuel metering for CPU limits
//! - Memory limits via Wasmtime resource limiter
//! - Host function bridge to Document API via JSON serialization
//! - Shared linear memory buffers for data exchange
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │                  WasmRuntime                      │
//! │  ┌──────────────────────────────────────────┐    │
//! │  │         wasmtime::Instance               │    │
//! │  │  ┌────────────┐  ┌──────────────────┐   │    │
//! │  │  │ WASM Module │  │  Linear Memory   │   │    │
//! │  │  │ (compiled)  │  │  (request/resp)  │   │    │
//! │  │  └────────────┘  └──────────────────┘   │    │
//! │  └──────────────────────────────────────────┘    │
//! │  ┌─────────────┐  ┌──────────────────────────┐  │
//! │  │ Fuel Meter  │  │  Host Functions (Linker) │  │
//! │  │ (CPU limit) │  │  ├─ get_document_info    │  │
//! │  │             │  │  ├─ get_layers           │  │
//! │  │             │  │  ├─ create_rect          │  │
//! │  │             │  │  ├─ delete_layer         │  │
//! │  │             │  │  └─ log_message          │  │
//! │  └─────────────┘  └──────────────────────────┘  │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! ## FFI Protocol
//!
//! WASM modules exchange data with the host via a request/response buffer
//! in linear memory. The protocol uses JSON serialization:
//!
//! 1. Plugin calls host function (e.g. `host_get_layers()`)
//! 2. Host reads request args from WASM memory (if any)
//! 3. Host executes, serializes result to JSON
//! 4. Host writes JSON bytes + length to response buffer
//! 5. Plugin reads response buffer
//!
//! Response buffer layout:
//! ```text
//! Offset 0: [u32] response length (little-endian)
//! Offset 4: [u8; N] JSON response bytes (UTF-8)
//! ```
//!
//! ## Plugin WASM Module Contract
//!
//! A valid Logos WASM plugin must export:
//! - `memory` — linear memory (min 1 page)
//! - `logos_init()` → i32 — called once after load, returns 0 on success
//! - `logos_execute(ptr: i32, len: i32)` → i32 — execute a command
//! - `logos_alloc(size: i32)` → i32 — allocate bytes in WASM memory
//! - `logos_dealloc(ptr: i32, len: i32)` — free allocated bytes
//!
//! The host imports provided to the module (namespace `logos`):
//! - `logos::host_get_document_info()` → i32 (response ptr)
//! - `logos::host_get_layers()` → i32 (response ptr)
//! - `logos::host_get_layer_count()` → i32 (count)
//! - `logos::host_get_layer(ptr: i32, len: i32)` → i32 (response ptr)
//! - `logos::host_create_rect(x: f32, y: f32, w: f32, h: f32)` → i32 (response ptr)
//! - `logos::host_delete_layer(ptr: i32, len: i32)` → i32 (1=ok, 0=fail)
//! - `logos::host_log(ptr: i32, len: i32)` — log message to host console

use crate::permissions::{PermissionGuard, PermissionKind, PermissionSet};
use crate::runtime::{PluginValue, ResourceLimits, RuntimeError, RuntimeResult};
use logos_core::{Camera, Document, Layer, PathCommand, Point};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// The response buffer offset within WASM linear memory.
/// First 64KB is reserved for the plugin stack. Response buffer starts at 64KB.
const RESPONSE_BUFFER_OFFSET: usize = 65536;

/// Maximum response buffer size (1MB).
const RESPONSE_BUFFER_MAX: usize = 1024 * 1024;

/// Default fuel amount per execution (maps to ~10ms of CPU at ~100M fuel/sec).
const DEFAULT_FUEL: u64 = 1_000_000;

/// A Wasmtime-based WebAssembly plugin runtime.
///
/// Provides memory-safe, fuel-metered execution of WASM plugin modules
/// with host function bindings to the Logos Document API.
pub struct WasmRuntime {
    /// Wasmtime engine (shared, compiled module cache).
    engine: wasmtime::Engine,
    /// Compiled WASM module (ready to instantiate).
    module: Option<wasmtime::Module>,
    /// Plugin name for diagnostics.
    name: String,
    /// Resource limits (fuel, memory).
    limits: ResourceLimits,
    /// Permission set for host function access control.
    permissions: PermissionSet,
    /// Document reference for host functions.
    document: Option<Arc<RwLock<Document>>>,
    /// Whether the runtime has been killed.
    killed: bool,
    /// Execution statistics.
    fuel_consumed: u64,
    host_calls: u64,
    /// Current camera state (shared with host functions).
    camera: Arc<RwLock<Camera>>,
    /// Notifications emitted during execution.
    notifications: Arc<RwLock<Vec<Notification>>>,
}

/// Configuration for the Wasmtime engine.
fn create_engine_config(limits: &ResourceLimits) -> wasmtime::Config {
    let mut config = wasmtime::Config::new();
    // Enable fuel metering for CPU limits
    config.consume_fuel(true);
    // Set compilation strategy
    config.cranelift_opt_level(wasmtime::OptLevel::Speed);
    // Memory limits are enforced via Store's limiter
    let _ = limits; // limits used at Store level, not engine level
    config
}

/// Convert fuel amount from resource limits.
/// Maps max_execution_time to fuel units.
/// ~100M fuel ≈ 1 second of execution on modern hardware.
fn fuel_from_limits(limits: &ResourceLimits) -> u64 {
    let millis = limits.max_execution_time.as_millis() as u64;
    // 100K fuel per millisecond
    millis.saturating_mul(100_000).max(DEFAULT_FUEL)
}

/// Data stored in the Wasmtime Store — accessible from host functions.
pub struct HostState {
    /// Shared document for host function access.
    document: Arc<RwLock<Document>>,
    /// Permission guard for access control.
    guard: PermissionGuard,
    /// Host call counter.
    host_calls: u64,
    /// Max host calls allowed.
    max_host_calls: usize,
    /// Log output buffer.
    log_output: Vec<String>,
    /// Wasmtime resource limiter.
    store_limits: wasmtime::StoreLimits,
    /// Current camera / viewport state.
    camera: Camera,
    /// Notification output buffer (toasts, confirms, prompts).
    notifications: Vec<Notification>,
    /// Lifecycle callback registry (event_name → registered).
    lifecycle_hooks: HashMap<String, bool>,
}

/// A notification emitted by a plugin.
#[derive(Clone, Debug, PartialEq)]
pub enum Notification {
    /// Toast message with text.
    Toast(String),
    /// Confirmation dialog with message — result stored separately.
    Confirm(String),
    /// Prompt dialog with message — result stored separately.
    Prompt(String),
}

impl HostState {
    fn new(
        document: Arc<RwLock<Document>>,
        permissions: PermissionSet,
        max_host_calls: usize,
        camera: Arc<RwLock<Camera>>,
        _notifications: Arc<RwLock<Vec<Notification>>>,
    ) -> Self {
        Self {
            document,
            guard: PermissionGuard::new(permissions),
            host_calls: 0,
            max_host_calls,
            log_output: Vec::new(),
            store_limits: wasmtime::StoreLimitsBuilder::new()
                .memory_size(50 * 1024 * 1024) // 50MB
                .build(),
            camera: (*camera.read().unwrap()),
            notifications: Vec::new(),
            lifecycle_hooks: HashMap::new(),
        }
    }

    /// Check and increment host call counter.
    fn count_call(&mut self) -> Result<(), wasmtime::Error> {
        self.host_calls += 1;
        if self.host_calls as usize > self.max_host_calls {
            return Err(wasmtime::Error::msg(format!(
                "host call limit exceeded: {} > {}",
                self.host_calls, self.max_host_calls
            )));
        }
        Ok(())
    }

    /// Check a permission, returning a Wasmtime-compatible error.
    fn check_permission(&mut self, kind: &PermissionKind) -> Result<(), wasmtime::Error> {
        self.guard
            .check(kind)
            .map_err(|e| wasmtime::Error::msg(format!("permission denied: {e}")))
    }
}

impl WasmRuntime {
    /// Create a new WASM runtime with the given name and resource limits.
    pub fn new(name: &str, limits: ResourceLimits, permissions: PermissionSet) -> RuntimeResult<Self> {
        let config = create_engine_config(&limits);
        let engine = wasmtime::Engine::new(&config)
            .map_err(|e| RuntimeError::CompileError(format!("failed to create wasmtime engine: {e}")))?;

        Ok(Self {
            engine,
            module: None,
            name: name.to_string(),
            limits,
            permissions,
            document: None,
            killed: false,
            fuel_consumed: 0,
            host_calls: 0,
            camera: Arc::new(RwLock::new(Camera::default())),
            notifications: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// Attach a document for host function access.
    pub fn register_document(&mut self, document: Arc<RwLock<Document>>) {
        self.document = Some(document);
    }

    /// Set the camera state for viewport host functions.
    pub fn set_camera(&mut self, camera: Camera) {
        *self.camera.write().unwrap() = camera;
    }

    /// Get the current camera state.
    pub fn get_camera(&self) -> Camera {
        *self.camera.read().unwrap()
    }

    /// Get notifications emitted during the last execution.
    pub fn take_notifications(&self) -> Vec<Notification> {
        let mut n = self.notifications.write().unwrap();
        std::mem::take(&mut *n)
    }

    /// Load a WASM module from bytes.
    ///
    /// The module is compiled and validated by Wasmtime's Cranelift backend.
    /// Invalid or malicious modules are rejected at this stage.
    pub fn load_module(&mut self, wasm_bytes: &[u8]) -> RuntimeResult<()> {
        if self.killed {
            return Err(RuntimeError::ExecutionError("runtime has been killed".into()));
        }

        let module = wasmtime::Module::new(&self.engine, wasm_bytes)
            .map_err(|e| RuntimeError::CompileError(format!("WASM compilation failed: {e}")))?;

        self.module = Some(module);
        Ok(())
    }

    /// Load a WASM module from WAT (WebAssembly Text Format).
    ///
    /// Useful for testing — allows writing WASM plugins in human-readable text.
    pub fn load_wat(&mut self, wat: &str) -> RuntimeResult<()> {
        if self.killed {
            return Err(RuntimeError::ExecutionError("runtime has been killed".into()));
        }

        // Wasmtime's Module::new auto-detects WAT vs binary format
        let module = wasmtime::Module::new(&self.engine, wat)
            .map_err(|e| RuntimeError::CompileError(format!("WAT compilation failed: {e}")))?;

        self.module = Some(module);
        Ok(())
    }

    /// Execute the plugin's `logos_init` export function.
    ///
    /// Returns Ok(()) if init returns 0, or an error otherwise.
    pub fn init(&mut self) -> RuntimeResult<PluginValue> {
        self.call_export("logos_init", &[])
    }

    /// Execute the plugin's `logos_execute` export with a command string.
    ///
    /// The command is written to WASM linear memory, and the plugin
    /// processes it via its `logos_execute(ptr, len)` export.
    pub fn execute(&mut self, command: &str) -> RuntimeResult<PluginValue> {
        if self.killed {
            return Err(RuntimeError::ExecutionError("runtime has been killed".into()));
        }

        let module = self.module.as_ref().ok_or_else(|| {
            RuntimeError::ExecutionError("no module loaded".into())
        })?;

        let document = self.document.clone().ok_or_else(|| {
            RuntimeError::ExecutionError("no document attached".into())
        })?;

        let fuel = fuel_from_limits(&self.limits);
        let mut store = wasmtime::Store::new(
            &self.engine,
            HostState::new(
                document,
                self.permissions.clone(),
                self.limits.max_host_calls,
                self.camera.clone(),
                self.notifications.clone(),
            ),
        );

        // Set fuel for CPU limiting
        store.set_fuel(fuel).map_err(|e| {
            RuntimeError::ExecutionError(format!("failed to set fuel: {e}"))
        })?;

        // Set memory limits via HostState's StoreLimits
        store.limiter(|state| &mut state.store_limits);

        // Create linker with host functions
        let mut linker = wasmtime::Linker::new(&self.engine);
        register_host_functions(&mut linker)?;

        // Instantiate module
        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            RuntimeError::ExecutionError(format!("WASM instantiation failed: {e}"))
        })?;

        // Get memory export
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| RuntimeError::ExecutionError("WASM module has no 'memory' export".into()))?;

        // Write command string to WASM memory
        let cmd_bytes = command.as_bytes();
        let cmd_offset = RESPONSE_BUFFER_OFFSET + RESPONSE_BUFFER_MAX;

        // Ensure memory is large enough
        let needed_pages = ((cmd_offset + cmd_bytes.len()) / 65536) + 1;
        let current_pages = memory.size(&store) as usize;
        if needed_pages > current_pages {
            let grow_by = (needed_pages - current_pages) as u64;
            memory.grow(&mut store, grow_by).map_err(|_| {
                RuntimeError::MemoryLimitExceeded {
                    used: needed_pages * 65536,
                    limit: self.limits.max_memory_bytes,
                }
            })?;
        }

        // Write command to memory
        memory.data_mut(&mut store)[cmd_offset..cmd_offset + cmd_bytes.len()]
            .copy_from_slice(cmd_bytes);

        // Call logos_execute(ptr, len)
        let execute_fn = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "logos_execute")
            .map_err(|e| RuntimeError::ExecutionError(format!("missing logos_execute export: {e}")))?;

        let result = execute_fn
            .call(&mut store, (cmd_offset as i32, cmd_bytes.len() as i32))
            .map_err(|e| {
                // Check if it was a fuel exhaustion (trap or out-of-fuel)
                let msg = e.to_string();
                if msg.contains("fuel") || msg.contains("wasm trap") || e.downcast_ref::<wasmtime::Trap>().is_some() {
                    RuntimeError::TimeLimitExceeded {
                        elapsed: self.limits.max_execution_time,
                        limit: self.limits.max_execution_time,
                    }
                } else {
                    RuntimeError::ExecutionError(format!("WASM execution failed: {e}"))
                }
            })?;

        // Read execution stats
        let remaining_fuel = store.get_fuel().unwrap_or(0);
        self.fuel_consumed += fuel - remaining_fuel;
        self.host_calls += store.data().host_calls;

        // Copy camera state back from host
        *self.camera.write().unwrap() = store.data().camera;
        // Copy notifications back
        {
            let mut notifs = self.notifications.write().unwrap();
            notifs.extend(store.data_mut().notifications.drain(..));
        }

        // Read response from buffer
        let resp_data = memory.data(&store);
        if result > 0 && (result as usize) < resp_data.len() {
            // Response was written to buffer — read length prefix + JSON
            let resp_offset = result as usize;
            if resp_offset + 4 <= resp_data.len() {
                let len = u32::from_le_bytes([
                    resp_data[resp_offset],
                    resp_data[resp_offset + 1],
                    resp_data[resp_offset + 2],
                    resp_data[resp_offset + 3],
                ]) as usize;

                if len > 0 && resp_offset + 4 + len <= resp_data.len() {
                    let json_bytes = &resp_data[resp_offset + 4..resp_offset + 4 + len];
                    if let Ok(json_str) = std::str::from_utf8(json_bytes) {
                        return json_to_plugin_value(json_str);
                    }
                }
            }
        }

        // Return the raw i32 result
        Ok(PluginValue::Int(result as i64))
    }

    /// Call an exported function by name with no arguments.
    pub fn call_export(&mut self, name: &str, _args: &[PluginValue]) -> RuntimeResult<PluginValue> {
        if self.killed {
            return Err(RuntimeError::ExecutionError("runtime has been killed".into()));
        }

        let module = self.module.as_ref().ok_or_else(|| {
            RuntimeError::ExecutionError("no module loaded".into())
        })?;

        let document = self.document.clone().unwrap_or_else(|| {
            Arc::new(RwLock::new(Document::new()))
        });

        let fuel = fuel_from_limits(&self.limits);
        let mut store = wasmtime::Store::new(
            &self.engine,
            HostState::new(
                document,
                self.permissions.clone(),
                self.limits.max_host_calls,
                self.camera.clone(),
                self.notifications.clone(),
            ),
        );

        store.set_fuel(fuel).map_err(|e| {
            RuntimeError::ExecutionError(format!("failed to set fuel: {e}"))
        })?;

        let mut linker = wasmtime::Linker::new(&self.engine);
        register_host_functions(&mut linker)?;

        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            RuntimeError::ExecutionError(format!("WASM instantiation failed: {e}"))
        })?;

        // Try i32 return first, then void
        if let Ok(func) = instance.get_typed_func::<(), i32>(&mut store, name) {
            let result = func.call(&mut store, ()).map_err(|e| {
                if e.to_string().contains("fuel") {
                    RuntimeError::TimeLimitExceeded {
                        elapsed: self.limits.max_execution_time,
                        limit: self.limits.max_execution_time,
                    }
                } else {
                    RuntimeError::ExecutionError(format!("WASM call '{name}' failed: {e}"))
                }
            })?;

            let remaining_fuel = store.get_fuel().unwrap_or(0);
            self.fuel_consumed += fuel - remaining_fuel;
            self.host_calls += store.data().host_calls;

            Ok(PluginValue::Int(result as i64))
        } else if let Ok(func) = instance.get_typed_func::<(), ()>(&mut store, name) {
            func.call(&mut store, ()).map_err(|e| {
                RuntimeError::ExecutionError(format!("WASM call '{name}' failed: {e}"))
            })?;

            let remaining_fuel = store.get_fuel().unwrap_or(0);
            self.fuel_consumed += fuel - remaining_fuel;

            Ok(PluginValue::Null)
        } else {
            Err(RuntimeError::ExecutionError(format!(
                "export '{name}' not found or has unsupported signature"
            )))
        }
    }

    /// Kill this runtime — no further execution is possible.
    pub fn kill(&mut self) {
        self.killed = true;
        self.module = None;
    }

    /// Whether this runtime has been killed.
    pub fn is_killed(&self) -> bool {
        self.killed
    }

    /// Get the runtime name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get total fuel consumed across all executions.
    pub fn fuel_consumed(&self) -> u64 {
        self.fuel_consumed
    }

    /// Get total host function calls made.
    pub fn host_calls(&self) -> u64 {
        self.host_calls
    }

    /// Get the log output from the last execution.
    pub fn permissions(&self) -> &PermissionSet {
        &self.permissions
    }
}

// ── Host Function Registration ──────────────────────────────────

/// Register all Logos host functions on the Wasmtime linker.
///
/// These are importable by WASM modules under the `logos` namespace.
fn register_host_functions(linker: &mut wasmtime::Linker<HostState>) -> RuntimeResult<()> {
    // logos::host_log(ptr, len) — log a message
    linker
        .func_wrap("logos", "host_log", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<(), wasmtime::Error> {
            caller.data_mut().count_call()?;
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
            let start = ptr as usize;
            let end = start + len as usize;
            // Copy bytes out first, then drop the borrow
            let msg = {
                let data = memory.data(&caller);
                if end > data.len() {
                    return Err(wasmtime::Error::msg("out of bounds memory access"));
                }
                std::str::from_utf8(&data[start..end])
                    .map_err(|e| wasmtime::Error::msg(format!("invalid UTF-8: {e}")))?
                    .to_string()
            };
            caller.data_mut().log_output.push(msg);
            Ok(())
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_log: {e}")))?;

    // logos::host_get_layer_count() → i32
    linker
        .func_wrap("logos", "host_get_layer_count", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            let doc = caller.data().document.clone();
            let d = doc.read()
                .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
            let root = d.root.read()
                .map_err(|e| wasmtime::Error::msg(format!("root lock: {e}")))?;
            Ok(root.layers.len() as i32)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_layer_count: {e}")))?;

    // logos::host_create_rect(x, y, w, h) → i32 (writes UUID to response buffer)
    linker
        .func_wrap("logos", "host_create_rect", |mut caller: wasmtime::Caller<'_, HostState>, x: f32, y: f32, w: f32, h: f32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            // Phase 1: create layer and add to document
            let layer = Layer::Rect(logos_core::RectLayer::new(x, y, w, h));
            let id = layer.id();
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                d.add_layer(layer)
                    .map_err(|e| wasmtime::Error::msg(format!("add_layer: {e}")))?;
                // Wrap UUID as proper JSON string
                serde_json::json!(id.to_string()).to_string().into_bytes()
            }; // doc read guard dropped here
            // Phase 2: write JSON UUID to response buffer
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
            let resp_offset = RESPONSE_BUFFER_OFFSET;
            let len_bytes = (json_bytes.len() as u32).to_le_bytes();
            let data = memory.data_mut(&mut caller);
            if resp_offset + 4 + json_bytes.len() > data.len() {
                return Err(wasmtime::Error::msg("response buffer overflow"));
            }
            data[resp_offset..resp_offset + 4].copy_from_slice(&len_bytes);
            data[resp_offset + 4..resp_offset + 4 + json_bytes.len()].copy_from_slice(&json_bytes);
            Ok(resp_offset as i32)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_create_rect: {e}")))?;

    // logos::host_delete_layer(ptr, len) → i32 (1=success, 0=not found)
    linker
        .func_wrap("logos", "host_delete_layer", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
            let data = memory.data(&caller);
            let start = ptr as usize;
            let end = start + len as usize;
            if end > data.len() {
                return Err(wasmtime::Error::msg("out of bounds memory access"));
            }
            let id_str = std::str::from_utf8(&data[start..end])
                .map_err(|e| wasmtime::Error::msg(format!("invalid UTF-8: {e}")))?;
            let id = uuid::Uuid::parse_str(id_str)
                .map_err(|e| wasmtime::Error::msg(format!("invalid UUID: {e}")))?;
            let doc = caller.data().document.clone();
            let d = doc.read()
                .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
            match d.remove_layer(id) {
                Ok(_) => Ok(1),
                Err(_) => Ok(0),
            }
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_delete_layer: {e}")))?;

    // logos::host_get_document_info() → i32 (writes JSON to response buffer)
    linker
        .func_wrap("logos", "host_get_document_info", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            // Phase 1: read document and serialize to JSON
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                let root = d.root.read()
                    .map_err(|e| wasmtime::Error::msg(format!("root lock: {e}")))?;
                let info = serde_json::json!({
                    "id": d.id.to_string(),
                    "version": d.version,
                    "page_name": root.name,
                    "layer_count": root.layers.len(),
                });
                info.to_string().into_bytes()
            }; // all read guards dropped
            // Phase 2: write JSON to response buffer
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
            let resp_offset = RESPONSE_BUFFER_OFFSET;
            let len_bytes = (json_bytes.len() as u32).to_le_bytes();
            let data = memory.data_mut(&mut caller);
            if resp_offset + 4 + json_bytes.len() > data.len() {
                return Err(wasmtime::Error::msg("response buffer overflow"));
            }
            data[resp_offset..resp_offset + 4].copy_from_slice(&len_bytes);
            data[resp_offset + 4..resp_offset + 4 + json_bytes.len()].copy_from_slice(&json_bytes);
            Ok(resp_offset as i32)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_document_info: {e}")))?;

    // logos::host_get_layers() → i32 (writes JSON array to response buffer)
    linker
        .func_wrap("logos", "host_get_layers", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            // Phase 1: read layers and serialize to JSON
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                let root = d.root.read()
                    .map_err(|e| wasmtime::Error::msg(format!("root lock: {e}")))?;
                let layers: Vec<serde_json::Value> = root.layers.iter().map(|l| {
                    layer_to_json(l)
                }).collect();
                serde_json::to_string(&layers)
                    .unwrap_or_else(|_| "[]".to_string())
                    .into_bytes()
            }; // all read guards dropped
            // Phase 2: write JSON to response buffer
            if json_bytes.len() > RESPONSE_BUFFER_MAX {
                return Err(wasmtime::Error::msg("response too large"));
            }
            let memory = caller.get_export("memory")
                .and_then(|e| e.into_memory())
                .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
            let resp_offset = RESPONSE_BUFFER_OFFSET;
            let len_bytes = (json_bytes.len() as u32).to_le_bytes();
            let data = memory.data_mut(&mut caller);
            if resp_offset + 4 + json_bytes.len() > data.len() {
                return Err(wasmtime::Error::msg("response buffer overflow"));
            }
            data[resp_offset..resp_offset + 4].copy_from_slice(&len_bytes);
            data[resp_offset + 4..resp_offset + 4 + json_bytes.len()].copy_from_slice(&json_bytes);
            Ok(resp_offset as i32)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_layers: {e}")))?;

    // ── Document (read): get_selection ───────────────────────

    // logos::host_get_selection() → i32 (writes JSON array of UUID strings to response buffer)
    linker
        .func_wrap("logos", "host_get_selection", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                let sel = d.get_selection()
                    .map_err(|e| wasmtime::Error::msg(format!("selection lock: {e}")))?;
                let ids: Vec<String> = sel.iter().map(|id| id.to_string()).collect();
                serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()).into_bytes()
            };
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_selection: {e}")))?;

    // ── Document (read): get_layer_by_id ─────────────────────

    // logos::host_get_layer_by_id(ptr, len) → i32 (writes JSON layer or null to response buffer)
    linker
        .func_wrap("logos", "host_get_layer_by_id", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            let id = read_uuid_from_memory(&mut caller, ptr, len)?;
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                match d.find_layer_by_id(id)
                    .map_err(|e| wasmtime::Error::msg(format!("find_layer: {e}")))? {
                    Some(layer) => serde_json::to_string(&layer_to_json(&layer))
                        .unwrap_or_else(|_| "null".to_string()).into_bytes(),
                    None => b"null".to_vec(),
                }
            };
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_layer_by_id: {e}")))?;

    // ── Document (write): create_text ────────────────────────

    // logos::host_create_text(ptr, len, x, y, w, h) → i32 (writes UUID JSON to response buffer)
    linker
        .func_wrap("logos", "host_create_text", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32, x: f32, y: f32, w: f32, h: f32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            let content = read_string_from_memory(&mut caller, ptr, len)?;
            let layer = Layer::Text(logos_core::TextLayer::new(&content, x, y, w, h));
            let id = layer.id();
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                d.add_layer(layer)
                    .map_err(|e| wasmtime::Error::msg(format!("add_layer: {e}")))?;
                serde_json::json!(id.to_string()).to_string().into_bytes()
            };
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_create_text: {e}")))?;

    // ── Document (write): create_path ────────────────────────

    // logos::host_create_path(ptr, len) → i32 (reads JSON path commands from memory, writes UUID)
    linker
        .func_wrap("logos", "host_create_path", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            let json_str = read_string_from_memory(&mut caller, ptr, len)?;
            let commands = parse_path_commands(&json_str)
                .map_err(|e| wasmtime::Error::msg(format!("invalid path commands: {e}")))?;
            let layer = Layer::Path(logos_core::PathLayer::new(commands));
            let id = layer.id();
            let json_bytes = {
                let doc = caller.data().document.clone();
                let d = doc.read()
                    .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
                d.add_layer(layer)
                    .map_err(|e| wasmtime::Error::msg(format!("add_layer: {e}")))?;
                serde_json::json!(id.to_string()).to_string().into_bytes()
            };
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_create_path: {e}")))?;

    // ── Selection: set_selection ──────────────────────────────

    // logos::host_set_selection(ptr, len) → i32 (reads JSON array of UUID strings)
    linker
        .func_wrap("logos", "host_set_selection", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            let json_str = read_string_from_memory(&mut caller, ptr, len)?;
            let ids: Vec<String> = serde_json::from_str(&json_str)
                .map_err(|e| wasmtime::Error::msg(format!("invalid JSON: {e}")))?;
            let uuids: Vec<uuid::Uuid> = ids.iter().map(|s| {
                uuid::Uuid::parse_str(s).map_err(|e| wasmtime::Error::msg(format!("invalid UUID '{s}': {e}")))
            }).collect::<Result<_, _>>()?;
            let doc = caller.data().document.clone();
            let d = doc.read()
                .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
            d.set_selection(uuids)
                .map_err(|e| wasmtime::Error::msg(format!("set_selection: {e}")))?;
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_set_selection: {e}")))?;

    // ── Selection: clear_selection ────────────────────────────

    // logos::host_clear_selection() → i32
    linker
        .func_wrap("logos", "host_clear_selection", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentWrite)?;
            let doc = caller.data().document.clone();
            let d = doc.read()
                .map_err(|e| wasmtime::Error::msg(format!("document lock: {e}")))?;
            d.clear_selection()
                .map_err(|e| wasmtime::Error::msg(format!("clear_selection: {e}")))?;
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_clear_selection: {e}")))?;

    // ── Selection: on_selection_changed ───────────────────────

    // logos::host_on_selection_changed() → i32 (registers interest in selection changes)
    linker
        .func_wrap("logos", "host_on_selection_changed", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::DocumentRead)?;
            caller.data_mut().lifecycle_hooks.insert("selection_changed".to_string(), true);
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_on_selection_changed: {e}")))?;

    // ── Viewport: get_camera ─────────────────────────────────

    // logos::host_get_camera() → i32 (writes JSON camera to response buffer)
    linker
        .func_wrap("logos", "host_get_camera", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            let cam = caller.data().camera;
            let json_bytes = serde_json::json!({
                "x": cam.x,
                "y": cam.y,
                "zoom": cam.zoom,
            }).to_string().into_bytes();
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_get_camera: {e}")))?;

    // ── Viewport: set_camera ─────────────────────────────────

    // logos::host_set_camera(x, y, zoom) → i32
    linker
        .func_wrap("logos", "host_set_camera", |mut caller: wasmtime::Caller<'_, HostState>, x: f32, y: f32, zoom: f32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            if zoom <= 0.0 || zoom > 100.0 {
                return Err(wasmtime::Error::msg(format!("invalid zoom: {zoom} (must be 0 < z <= 100)")));
            }
            caller.data_mut().camera = Camera::new(x, y, zoom);
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_set_camera: {e}")))?;

    // ── Viewport: screen_to_world ────────────────────────────

    // logos::host_screen_to_world(sx, sy) → i32 (writes JSON point to response buffer)
    linker
        .func_wrap("logos", "host_screen_to_world", |mut caller: wasmtime::Caller<'_, HostState>, sx: f32, sy: f32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            let cam = caller.data().camera;
            let world = cam.screen_to_world(sx, sy);
            let json_bytes = serde_json::json!({
                "x": world.x,
                "y": world.y,
            }).to_string().into_bytes();
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_screen_to_world: {e}")))?;

    // ── Notifications: show_toast ────────────────────────────

    // logos::host_show_toast(ptr, len) → i32
    linker
        .func_wrap("logos", "host_show_toast", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::Notifications)?;
            let msg = read_string_from_memory(&mut caller, ptr, len)?;
            caller.data_mut().notifications.push(Notification::Toast(msg));
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_show_toast: {e}")))?;

    // ── Notifications: confirm ───────────────────────────────

    // logos::host_confirm(ptr, len) → i32 (always returns 1 = yes in sandbox, writes to buffer)
    linker
        .func_wrap("logos", "host_confirm", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::Notifications)?;
            let msg = read_string_from_memory(&mut caller, ptr, len)?;
            caller.data_mut().notifications.push(Notification::Confirm(msg));
            // In sandbox mode, confirm always returns 1 (yes)
            // Real UI integration would block and return user's choice
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_confirm: {e}")))?;

    // ── Notifications: prompt ────────────────────────────────

    // logos::host_prompt(ptr, len) → i32 (writes empty string to response buffer)
    linker
        .func_wrap("logos", "host_prompt", |mut caller: wasmtime::Caller<'_, HostState>, ptr: i32, len: i32| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().check_permission(&PermissionKind::Notifications)?;
            let msg = read_string_from_memory(&mut caller, ptr, len)?;
            caller.data_mut().notifications.push(Notification::Prompt(msg));
            // In sandbox mode, prompt returns empty string; real UI would block
            let json_bytes = serde_json::json!("").to_string().into_bytes();
            write_response_buffer(&mut caller, &json_bytes)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_prompt: {e}")))?;

    // ── Lifecycle: on_load ───────────────────────────────────

    // logos::host_on_load() → i32 (registers the on_load lifecycle hook)
    linker
        .func_wrap("logos", "host_on_load", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().lifecycle_hooks.insert("on_load".to_string(), true);
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_on_load: {e}")))?;

    // ── Lifecycle: on_unload ─────────────────────────────────

    // logos::host_on_unload() → i32 (registers the on_unload lifecycle hook)
    linker
        .func_wrap("logos", "host_on_unload", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().lifecycle_hooks.insert("on_unload".to_string(), true);
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_on_unload: {e}")))?;

    // ── Lifecycle: on_frame ──────────────────────────────────

    // logos::host_on_frame() → i32 (registers the on_frame lifecycle hook)
    linker
        .func_wrap("logos", "host_on_frame", |mut caller: wasmtime::Caller<'_, HostState>| -> Result<i32, wasmtime::Error> {
            caller.data_mut().count_call()?;
            caller.data_mut().lifecycle_hooks.insert("on_frame".to_string(), true);
            Ok(1)
        })
        .map_err(|e| RuntimeError::CompileError(format!("failed to register host_on_frame: {e}")))?;

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

/// Write a JSON byte slice to the response buffer in WASM memory.
/// Returns the response buffer offset as i32.
fn write_response_buffer(caller: &mut wasmtime::Caller<'_, HostState>, json_bytes: &[u8]) -> Result<i32, wasmtime::Error> {
    if json_bytes.len() > RESPONSE_BUFFER_MAX {
        return Err(wasmtime::Error::msg("response too large"));
    }
    let memory = caller.get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
    let resp_offset = RESPONSE_BUFFER_OFFSET;
    let len_bytes = (json_bytes.len() as u32).to_le_bytes();
    let data = memory.data_mut(caller);
    if resp_offset + 4 + json_bytes.len() > data.len() {
        return Err(wasmtime::Error::msg("response buffer overflow"));
    }
    data[resp_offset..resp_offset + 4].copy_from_slice(&len_bytes);
    data[resp_offset + 4..resp_offset + 4 + json_bytes.len()].copy_from_slice(json_bytes);
    Ok(resp_offset as i32)
}

/// Read a UTF-8 string from WASM linear memory at (ptr, len).
fn read_string_from_memory(caller: &mut wasmtime::Caller<'_, HostState>, ptr: i32, len: i32) -> Result<String, wasmtime::Error> {
    let memory = caller.get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("no memory export"))?;
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;
    if end > data.len() {
        return Err(wasmtime::Error::msg("out of bounds memory access"));
    }
    std::str::from_utf8(&data[start..end])
        .map(|s| s.to_string())
        .map_err(|e| wasmtime::Error::msg(format!("invalid UTF-8: {e}")))
}

/// Read a UUID string from WASM linear memory at (ptr, len).
fn read_uuid_from_memory(caller: &mut wasmtime::Caller<'_, HostState>, ptr: i32, len: i32) -> Result<uuid::Uuid, wasmtime::Error> {
    let s = read_string_from_memory(caller, ptr, len)?;
    uuid::Uuid::parse_str(&s)
        .map_err(|e| wasmtime::Error::msg(format!("invalid UUID: {e}")))
}

/// Parse a JSON array of path commands into PathCommand objects.
///
/// Expected format:
/// ```json
/// [
///   {"type": "moveTo", "x": 0.0, "y": 0.0},
///   {"type": "lineTo", "x": 100.0, "y": 50.0},
///   {"type": "quadTo", "cx": 50, "cy": 100, "x": 100, "y": 0},
///   {"type": "bezierTo", "cx1": 1, "cy1": 2, "cx2": 3, "cy2": 4, "x": 5, "y": 6},
///   {"type": "close"}
/// ]
/// ```
fn parse_path_commands(json: &str) -> Result<Vec<PathCommand>, String> {
    let arr: Vec<serde_json::Value> = serde_json::from_str(json)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    let mut commands = Vec::with_capacity(arr.len());
    for item in &arr {
        let cmd_type = item["type"].as_str().ok_or("missing 'type' field")?;
        match cmd_type {
            "moveTo" => {
                let x = item["x"].as_f64().ok_or("moveTo: missing 'x'")? as f32;
                let y = item["y"].as_f64().ok_or("moveTo: missing 'y'")? as f32;
                commands.push(PathCommand::MoveTo(Point::new(x, y)));
            }
            "lineTo" => {
                let x = item["x"].as_f64().ok_or("lineTo: missing 'x'")? as f32;
                let y = item["y"].as_f64().ok_or("lineTo: missing 'y'")? as f32;
                commands.push(PathCommand::LineTo(Point::new(x, y)));
            }
            "quadTo" => {
                let cx = item["cx"].as_f64().ok_or("quadTo: missing 'cx'")? as f32;
                let cy = item["cy"].as_f64().ok_or("quadTo: missing 'cy'")? as f32;
                let x = item["x"].as_f64().ok_or("quadTo: missing 'x'")? as f32;
                let y = item["y"].as_f64().ok_or("quadTo: missing 'y'")? as f32;
                commands.push(PathCommand::QuadTo {
                    ctrl: Point::new(cx, cy),
                    end: Point::new(x, y),
                });
            }
            "bezierTo" => {
                let cx1 = item["cx1"].as_f64().ok_or("bezierTo: missing 'cx1'")? as f32;
                let cy1 = item["cy1"].as_f64().ok_or("bezierTo: missing 'cy1'")? as f32;
                let cx2 = item["cx2"].as_f64().ok_or("bezierTo: missing 'cx2'")? as f32;
                let cy2 = item["cy2"].as_f64().ok_or("bezierTo: missing 'cy2'")? as f32;
                let x = item["x"].as_f64().ok_or("bezierTo: missing 'x'")? as f32;
                let y = item["y"].as_f64().ok_or("bezierTo: missing 'y'")? as f32;
                commands.push(PathCommand::BezierTo {
                    cp1: Point::new(cx1, cy1),
                    cp2: Point::new(cx2, cy2),
                    end: Point::new(x, y),
                });
            }
            "close" => {
                commands.push(PathCommand::Close);
            }
            other => return Err(format!("unknown command type: '{other}'")),
        }
    }
    Ok(commands)
}

/// Convert a Layer to a JSON value for serialization to WASM.
fn layer_to_json(layer: &Layer) -> serde_json::Value {
    match layer {
        Layer::Rect(r) => serde_json::json!({
            "type": "rect",
            "id": r.id.to_string(),
            "x": r.bounds.x,
            "y": r.bounds.y,
            "width": r.bounds.width,
            "height": r.bounds.height,
        }),
        Layer::Ellipse(e) => serde_json::json!({
            "type": "ellipse",
            "id": e.id.to_string(),
            "x": e.bounds.x,
            "y": e.bounds.y,
            "width": e.bounds.width,
            "height": e.bounds.height,
        }),
        Layer::Text(t) => serde_json::json!({
            "type": "text",
            "id": t.id.to_string(),
            "content": t.content,
            "x": t.bounds.x,
            "y": t.bounds.y,
            "width": t.bounds.width,
            "height": t.bounds.height,
        }),
        Layer::Frame(f) => serde_json::json!({
            "type": "frame",
            "id": f.id.to_string(),
            "x": f.bounds.x,
            "y": f.bounds.y,
            "width": f.bounds.width,
            "height": f.bounds.height,
            "children": f.children.iter().map(layer_to_json).collect::<Vec<_>>(),
        }),
        Layer::Path(p) => serde_json::json!({
            "type": "path",
            "id": p.id.to_string(),
            "x": p.bounds.x,
            "y": p.bounds.y,
            "width": p.bounds.width,
            "height": p.bounds.height,
        }),
        Layer::Artboard(ab) => {
            let children: Vec<_> = ab.children.iter().map(layer_to_json).collect();
            serde_json::json!({
                "type": "artboard",
                "id": ab.id.to_string(),
                "name": ab.name,
                "x": ab.bounds.x,
                "y": ab.bounds.y,
                "width": ab.bounds.width,
                "height": ab.bounds.height,
                "clipContent": ab.clip_content,
                "children": children,
            })
        }
        Layer::Drawer(d) => {
            let eff = d.effective_bounds();
            let children: Vec<_> = d.children.iter().map(layer_to_json).collect();
            serde_json::json!({
                "type": "drawer",
                "id": d.id.to_string(),
                "name": d.name,
                "x": eff.x,
                "y": eff.y,
                "width": eff.width,
                "height": eff.height,
                "edge": format!("{:?}", d.edge),
                "state": format!("{:?}", d.state),
                "sizeOpen": d.size_open,
                "sizeClosed": d.size_closed,
                "children": children,
            })
        }
        Layer::Section(s) => {
            let eff = s.computed_bounds();
            let children: Vec<_> = s.children.iter().map(layer_to_json).collect();
            serde_json::json!({
                "type": "section",
                "id": s.id.to_string(),
                "name": s.name,
                "x": eff.x,
                "y": eff.y,
                "width": eff.width,
                "height": eff.height,
                "collapsed": s.is_collapsed,
                "children": children,
            })
        }
    }
}

/// Parse a JSON string into a PluginValue.
fn json_to_plugin_value(json: &str) -> RuntimeResult<PluginValue> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| RuntimeError::ExecutionError(format!("invalid JSON response: {e}")))?;
    Ok(serde_value_to_plugin_value(&v))
}

/// Convert a serde_json Value to a PluginValue.
fn serde_value_to_plugin_value(v: &serde_json::Value) -> PluginValue {
    match v {
        serde_json::Value::Null => PluginValue::Null,
        serde_json::Value::Bool(b) => PluginValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PluginValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                PluginValue::Float(f)
            } else {
                PluginValue::Null
            }
        }
        serde_json::Value::String(s) => PluginValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            PluginValue::Array(arr.iter().map(serde_value_to_plugin_value).collect())
        }
        serde_json::Value::Object(map) => {
            let mut hm = HashMap::new();
            for (k, v) in map {
                hm.insert(k.clone(), serde_value_to_plugin_value(v));
            }
            PluginValue::Object(hm)
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionSet;
    use crate::runtime::ResourceLimits;
    use std::time::Duration;

    // Helper: minimal WAT module that just returns 42
    const WAT_RETURN_42: &str = r#"
        (module
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 42
            )
        )
    "#;

    // Helper: WAT module that calls host_log
    const WAT_LOG: &str = r#"
        (module
            (import "logos" "host_log" (func $log (param i32 i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "hello from wasm")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                ;; Call host_log with ptr=0, len=15
                i32.const 0
                i32.const 15
                call $log
                i32.const 0
            )
        )
    "#;

    // Helper: WAT module that calls host_get_layer_count
    const WAT_LAYER_COUNT: &str = r#"
        (module
            (import "logos" "host_get_layer_count" (func $count (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $count
            )
        )
    "#;

    // Helper: WAT module that calls host_create_rect
    const WAT_CREATE_RECT: &str = r#"
        (module
            (import "logos" "host_create_rect" (func $create (param f32 f32 f32 f32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                f32.const 10.0
                f32.const 20.0
                f32.const 100.0
                f32.const 50.0
                call $create
            )
        )
    "#;

    // Helper: WAT infinite loop (for fuel testing)
    const WAT_INFINITE_LOOP: &str = r#"
        (module
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                (loop $forever
                    br $forever
                )
                i32.const 0
            )
        )
    "#;

    // Helper: WAT module that calls host_get_document_info
    const WAT_DOC_INFO: &str = r#"
        (module
            (import "logos" "host_get_document_info" (func $info (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $info
            )
        )
    "#;

    // Helper: WAT module that calls host_get_layers
    const WAT_GET_LAYERS: &str = r#"
        (module
            (import "logos" "host_get_layers" (func $layers (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $layers
            )
        )
    "#;

    // Helper: WAT module that calls host_delete_layer with a UUID from memory
    const WAT_DELETE_LAYER: &str = r#"
        (module
            (import "logos" "host_delete_layer" (func $del (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                ;; Read UUID from the command input (ptr, len passed as params)
                local.get 0  ;; ptr
                local.get 1  ;; len
                call $del
            )
        )
    "#;

    fn make_runtime(perms: PermissionSet) -> WasmRuntime {
        WasmRuntime::new("test-plugin", ResourceLimits::default(), perms).unwrap()
    }

    fn make_doc() -> Arc<RwLock<Document>> {
        Arc::new(RwLock::new(Document::new()))
    }

    // ── Engine creation tests ────────────────────────────────

    #[test]
    fn test_create_runtime() {
        let rt = make_runtime(PermissionSet::none());
        assert_eq!(rt.name(), "test-plugin");
        assert!(!rt.is_killed());
        assert_eq!(rt.fuel_consumed(), 0);
        assert_eq!(rt.host_calls(), 0);
    }

    #[test]
    fn test_create_runtime_with_permissions() {
        let rt = make_runtime(PermissionSet::document_full());
        assert!(rt.permissions().has(&PermissionKind::DocumentRead));
        assert!(rt.permissions().has(&PermissionKind::DocumentWrite));
        assert!(!rt.permissions().has(&PermissionKind::Network));
    }

    #[test]
    fn test_kill_runtime() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.kill();
        assert!(rt.is_killed());
        assert!(rt.load_wat(WAT_RETURN_42).is_err());
    }

    // ── Module loading tests ─────────────────────────────────

    #[test]
    fn test_load_wat_valid() {
        let mut rt = make_runtime(PermissionSet::none());
        assert!(rt.load_wat(WAT_RETURN_42).is_ok());
    }

    #[test]
    fn test_load_wat_invalid() {
        let mut rt = make_runtime(PermissionSet::none());
        let result = rt.load_wat("(not valid wat)");
        assert!(result.is_err());
        if let Err(RuntimeError::CompileError(msg)) = result {
            assert!(msg.contains("parse") || msg.contains("WAT"));
        }
    }

    #[test]
    fn test_load_module_invalid_bytes() {
        let mut rt = make_runtime(PermissionSet::none());
        let result = rt.load_module(&[0, 1, 2, 3]);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_without_module() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_without_document() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        let result = rt.execute("test");
        assert!(result.is_err());
    }

    // ── Basic execution tests ────────────────────────────────

    #[test]
    fn test_execute_return_42() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test").unwrap();
        assert_eq!(result, PluginValue::Int(42));
    }

    #[test]
    fn test_init_returns_zero() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        rt.register_document(make_doc());
        let result = rt.init().unwrap();
        assert_eq!(result, PluginValue::Int(0));
    }

    #[test]
    fn test_fuel_consumed_after_execution() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        rt.register_document(make_doc());
        rt.execute("test").unwrap();
        assert!(rt.fuel_consumed() > 0);
    }

    #[test]
    fn test_execute_after_kill() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        rt.register_document(make_doc());
        rt.kill();
        assert!(rt.execute("test").is_err());
    }

    // ── Fuel metering / resource limit tests ─────────────────

    #[test]
    fn test_infinite_loop_fuel_exhaustion() {
        let mut limits = ResourceLimits::default();
        limits.max_execution_time = Duration::from_millis(1); // very low fuel
        let mut rt = WasmRuntime::new("loop-test", limits, PermissionSet::none()).unwrap();
        rt.load_wat(WAT_INFINITE_LOOP).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "infinite loop should be interrupted");
        // Any error is acceptable — fuel exhaustion manifests as either
        // TimeLimitExceeded or ExecutionError with a trap
    }

    #[test]
    fn test_fuel_from_limits() {
        let limits = ResourceLimits::default(); // 10ms
        let fuel = fuel_from_limits(&limits);
        assert_eq!(fuel, 1_000_000); // 10ms × 100K = 1M
    }

    #[test]
    fn test_fuel_from_limits_custom() {
        let mut limits = ResourceLimits::default();
        limits.max_execution_time = Duration::from_millis(100);
        let fuel = fuel_from_limits(&limits);
        assert_eq!(fuel, 10_000_000); // 100ms × 100K = 10M
    }

    #[test]
    fn test_fuel_minimum() {
        let mut limits = ResourceLimits::default();
        limits.max_execution_time = Duration::from_nanos(1);
        let fuel = fuel_from_limits(&limits);
        assert_eq!(fuel, DEFAULT_FUEL); // clamped to minimum
    }

    // ── Host function: log ───────────────────────────────────

    #[test]
    fn test_host_log() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_LOG).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        // Log doesn't require permissions — should succeed
        assert!(result.is_ok());
        assert!(rt.host_calls() > 0);
    }

    // ── Host function: get_layer_count ────────────────────────

    #[test]
    fn test_host_get_layer_count_empty() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_LAYER_COUNT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test").unwrap();
        assert_eq!(result, PluginValue::Int(0));
    }

    #[test]
    fn test_host_get_layer_count_with_layers() {
        let doc = make_doc();
        {
            let d = doc.read().unwrap();
            let layer = Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 100.0, 100.0));
            d.add_layer(layer).unwrap();
            let layer2 = Layer::Rect(logos_core::RectLayer::new(10.0, 10.0, 50.0, 50.0));
            d.add_layer(layer2).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_LAYER_COUNT).unwrap();
        rt.register_document(doc);
        let result = rt.execute("test").unwrap();
        assert_eq!(result, PluginValue::Int(2));
    }

    #[test]
    fn test_host_get_layer_count_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none()); // no permissions!
        rt.load_wat(WAT_LAYER_COUNT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err()); // should fail — no DocumentRead permission
    }

    // ── Host function: create_rect ───────────────────────────

    #[test]
    fn test_host_create_rect() {
        let doc = make_doc();
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_CREATE_RECT).unwrap();
        rt.register_document(doc.clone());
        let result = rt.execute("test");
        assert!(result.is_ok(), "create_rect failed: {:?}", result.err());
        // Verify the layer was actually created
        let d = doc.read().unwrap();
        let root = d.root.read().unwrap();
        assert_eq!(root.layers.len(), 1);
    }

    #[test]
    fn test_host_create_rect_permission_denied() {
        let mut rt = make_runtime(PermissionSet::read_only()); // read-only!
        rt.load_wat(WAT_CREATE_RECT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err()); // should fail — no DocumentWrite permission
    }

    // ── Host function: get_document_info ──────────────────────

    #[test]
    fn test_host_get_document_info() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_DOC_INFO).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok());
        // The result should be a response buffer pointer — parsed as JSON
        let val = result.unwrap();
        match val {
            PluginValue::Object(ref map) => {
                assert!(map.contains_key("id"));
                assert!(map.contains_key("version"));
                assert!(map.contains_key("layer_count"));
            }
            PluginValue::Int(ptr) => {
                // Response was written to buffer — the ptr is the offset
                assert!(ptr > 0);
            }
            _ => {} // Either form is acceptable
        }
    }

    // ── Host function: get_layers ────────────────────────────

    #[test]
    fn test_host_get_layers_empty() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_GET_LAYERS).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_host_get_layers_with_data() {
        let doc = make_doc();
        {
            let d = doc.read().unwrap();
            d.add_layer(Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_GET_LAYERS).unwrap();
        rt.register_document(doc);
        let result = rt.execute("test");
        assert!(result.is_ok());
    }

    // ── Host function: delete_layer ──────────────────────────

    #[test]
    fn test_host_delete_layer() {
        let doc = make_doc();
        let layer_id;
        {
            let d = doc.read().unwrap();
            let layer = Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 100.0, 100.0));
            layer_id = layer.id();
            d.add_layer(layer).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_DELETE_LAYER).unwrap();
        rt.register_document(doc.clone());
        // The WAT module reads (ptr, len) from the logos_execute params
        // We pass the UUID string as the "command"
        let result = rt.execute(&layer_id.to_string());
        assert!(result.is_ok());
        // Verify deletion
        let d = doc.read().unwrap();
        let root = d.root.read().unwrap();
        assert_eq!(root.layers.len(), 0);
    }

    #[test]
    fn test_host_delete_layer_not_found() {
        let doc = make_doc();
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_DELETE_LAYER).unwrap();
        rt.register_document(doc);
        let fake_id = uuid::Uuid::new_v4().to_string();
        let result = rt.execute(&fake_id);
        // Should return 0 (not found) without error
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PluginValue::Int(0));
    }

    // ── Multiple executions ──────────────────────────────────

    #[test]
    fn test_multiple_executions() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_RETURN_42).unwrap();
        rt.register_document(make_doc());
        for _ in 0..5 {
            let result = rt.execute("test").unwrap();
            assert_eq!(result, PluginValue::Int(42));
        }
        assert!(rt.fuel_consumed() > 0);
    }

    #[test]
    fn test_host_calls_accumulate() {
        let doc = make_doc();
        {
            let d = doc.read().unwrap();
            d.add_layer(Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 100.0, 100.0))).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_LAYER_COUNT).unwrap();
        rt.register_document(doc);
        rt.execute("test").unwrap();
        rt.execute("test").unwrap();
        rt.execute("test").unwrap();
        assert_eq!(rt.host_calls(), 3);
    }

    // ── Config / fuel helpers ────────────────────────────────

    #[test]
    fn test_engine_config_consumes_fuel() {
        let limits = ResourceLimits::default();
        let config = create_engine_config(&limits);
        // Engine should create successfully with fuel enabled
        let engine = wasmtime::Engine::new(&config);
        assert!(engine.is_ok());
    }

    fn make_cam() -> Arc<RwLock<Camera>> {
        Arc::new(RwLock::new(Camera::default()))
    }

    fn make_notifs() -> Arc<RwLock<Vec<Notification>>> {
        Arc::new(RwLock::new(Vec::new()))
    }

    #[test]
    fn test_host_state_count_call() {
        let mut state = HostState::new(make_doc(), PermissionSet::none(), 3, make_cam(), make_notifs());
        assert!(state.count_call().is_ok()); // 1
        assert!(state.count_call().is_ok()); // 2
        assert!(state.count_call().is_ok()); // 3
        assert!(state.count_call().is_err()); // 4 > limit of 3
    }

    #[test]
    fn test_host_state_check_permission_granted() {
        let mut state = HostState::new(make_doc(), PermissionSet::read_only(), 100, make_cam(), make_notifs());
        assert!(state.check_permission(&PermissionKind::DocumentRead).is_ok());
    }

    #[test]
    fn test_host_state_check_permission_denied() {
        let mut state = HostState::new(make_doc(), PermissionSet::none(), 100, make_cam(), make_notifs());
        assert!(state.check_permission(&PermissionKind::DocumentRead).is_err());
    }

    // ── JSON conversion tests ────────────────────────────────

    #[test]
    fn test_json_to_plugin_value_null() {
        let v = json_to_plugin_value("null").unwrap();
        assert_eq!(v, PluginValue::Null);
    }

    #[test]
    fn test_json_to_plugin_value_int() {
        let v = json_to_plugin_value("42").unwrap();
        assert_eq!(v, PluginValue::Int(42));
    }

    #[test]
    fn test_json_to_plugin_value_float() {
        let v = json_to_plugin_value("3.14").unwrap();
        assert_eq!(v, PluginValue::Float(3.14));
    }

    #[test]
    fn test_json_to_plugin_value_string() {
        let v = json_to_plugin_value("\"hello\"").unwrap();
        assert_eq!(v, PluginValue::String("hello".to_string()));
    }

    #[test]
    fn test_json_to_plugin_value_array() {
        let v = json_to_plugin_value("[1,2,3]").unwrap();
        match v {
            PluginValue::Array(arr) => assert_eq!(arr.len(), 3),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_json_to_plugin_value_object() {
        let v = json_to_plugin_value("{\"key\": \"value\"}").unwrap();
        match v {
            PluginValue::Object(map) => {
                assert_eq!(map.get("key"), Some(&PluginValue::String("value".to_string())));
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn test_json_to_plugin_value_invalid() {
        let result = json_to_plugin_value("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_layer_to_json_rect() {
        let layer = Layer::Rect(logos_core::RectLayer::new(10.0, 20.0, 100.0, 50.0));
        let json = layer_to_json(&layer);
        assert_eq!(json["type"], "rect");
        assert_eq!(json["x"], 10.0);
        assert_eq!(json["y"], 20.0);
        assert_eq!(json["width"], 100.0);
        assert_eq!(json["height"], 50.0);
    }

    #[test]
    fn test_response_buffer_constants() {
        assert_eq!(RESPONSE_BUFFER_OFFSET, 65536);
        assert_eq!(RESPONSE_BUFFER_MAX, 1024 * 1024);
        assert_eq!(DEFAULT_FUEL, 1_000_000);
    }

    // ═══════════════════════════════════════════════════════════
    // ── Week 2 Host Function Tests ────────────────────────────
    // ═══════════════════════════════════════════════════════════

    // ── WAT modules for Week 2 host functions ────────────────

    // WAT: calls host_get_selection() → i32
    const WAT_GET_SELECTION: &str = r#"
        (module
            (import "logos" "host_get_selection" (func $sel (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $sel
            )
        )
    "#;

    // WAT: calls host_get_layer_by_id(ptr, len) — reads UUID from execute params
    const WAT_GET_LAYER_BY_ID: &str = r#"
        (module
            (import "logos" "host_get_layer_by_id" (func $get (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                call $get
            )
        )
    "#;

    // WAT: calls host_create_text(ptr, len, x, y, w, h) — text from data segment
    const WAT_CREATE_TEXT: &str = r#"
        (module
            (import "logos" "host_create_text" (func $txt (param i32 i32 f32 f32 f32 f32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "Hello World")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 0     ;; ptr
                i32.const 11    ;; len "Hello World"
                f32.const 10.0  ;; x
                f32.const 20.0  ;; y
                f32.const 200.0 ;; w
                f32.const 50.0  ;; h
                call $txt
            )
        )
    "#;

    // WAT: calls host_create_path(ptr, len) — JSON path commands from data segment
    const WAT_CREATE_PATH: &str = r#"
        (module
            (import "logos" "host_create_path" (func $path (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "[{\"type\":\"moveTo\",\"x\":0,\"y\":0},{\"type\":\"lineTo\",\"x\":100,\"y\":50},{\"type\":\"close\"}]")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 0    ;; ptr
                i32.const 81   ;; len of the JSON
                call $path
            )
        )
    "#;

    // WAT: calls host_set_selection(ptr, len) — reads JSON array from execute params
    const WAT_SET_SELECTION: &str = r#"
        (module
            (import "logos" "host_set_selection" (func $set (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                call $set
            )
        )
    "#;

    // WAT: calls host_clear_selection() → i32
    const WAT_CLEAR_SELECTION: &str = r#"
        (module
            (import "logos" "host_clear_selection" (func $clr (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $clr
            )
        )
    "#;

    // WAT: calls host_on_selection_changed() → i32
    const WAT_ON_SELECTION_CHANGED: &str = r#"
        (module
            (import "logos" "host_on_selection_changed" (func $hook (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $hook
            )
        )
    "#;

    // WAT: calls host_get_camera() → i32
    const WAT_GET_CAMERA: &str = r#"
        (module
            (import "logos" "host_get_camera" (func $cam (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $cam
            )
        )
    "#;

    // WAT: calls host_set_camera(x, y, zoom) → i32
    const WAT_SET_CAMERA: &str = r#"
        (module
            (import "logos" "host_set_camera" (func $set (param f32 f32 f32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                f32.const 100.0
                f32.const 200.0
                f32.const 2.0
                call $set
            )
        )
    "#;

    // WAT: calls host_set_camera with invalid zoom (0.0)
    const WAT_SET_CAMERA_INVALID: &str = r#"
        (module
            (import "logos" "host_set_camera" (func $set (param f32 f32 f32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                f32.const 0.0
                f32.const 0.0
                f32.const 0.0
                call $set
            )
        )
    "#;

    // WAT: calls host_screen_to_world(sx, sy) → i32
    const WAT_SCREEN_TO_WORLD: &str = r#"
        (module
            (import "logos" "host_screen_to_world" (func $stw (param f32 f32) (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                f32.const 400.0
                f32.const 300.0
                call $stw
            )
        )
    "#;

    // WAT: calls host_show_toast(ptr, len) — text from data segment
    const WAT_SHOW_TOAST: &str = r#"
        (module
            (import "logos" "host_show_toast" (func $toast (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "Layer created!")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 0
                i32.const 14
                call $toast
            )
        )
    "#;

    // WAT: calls host_confirm(ptr, len) — text from data segment
    const WAT_CONFIRM: &str = r#"
        (module
            (import "logos" "host_confirm" (func $cfm (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "Delete this?")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 0
                i32.const 12
                call $cfm
            )
        )
    "#;

    // WAT: calls host_prompt(ptr, len) — text from data segment
    const WAT_PROMPT: &str = r#"
        (module
            (import "logos" "host_prompt" (func $pmt (param i32 i32) (result i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "Enter name:")
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                i32.const 0
                i32.const 11
                call $pmt
            )
        )
    "#;

    // WAT: calls host_on_load() → i32
    const WAT_ON_LOAD: &str = r#"
        (module
            (import "logos" "host_on_load" (func $hook (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $hook
            )
        )
    "#;

    // WAT: calls host_on_unload() → i32
    const WAT_ON_UNLOAD: &str = r#"
        (module
            (import "logos" "host_on_unload" (func $hook (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $hook
            )
        )
    "#;

    // WAT: calls host_on_frame() → i32
    const WAT_ON_FRAME: &str = r#"
        (module
            (import "logos" "host_on_frame" (func $hook (result i32)))
            (memory (export "memory") 2)
            (func (export "logos_init") (result i32)
                i32.const 0
            )
            (func (export "logos_execute") (param i32 i32) (result i32)
                call $hook
            )
        )
    "#;

    // Helper: make a PermissionSet with notification permissions
    fn make_perms_with_notifications() -> PermissionSet {
        let mut perms = PermissionSet::document_full();
        perms.grant(PermissionKind::Notifications);
        perms
    }

    // ── Test: host_get_selection ──────────────────────────────

    #[test]
    fn test_host_get_selection_empty() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_GET_SELECTION).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "get_selection failed: {:?}", result.err());
    }

    #[test]
    fn test_host_get_selection_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_GET_SELECTION).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without DocumentRead");
    }

    // ── Test: host_get_layer_by_id ───────────────────────────

    #[test]
    fn test_host_get_layer_by_id_found() {
        let doc = make_doc();
        let layer_id;
        {
            let d = doc.read().unwrap();
            let layer = Layer::Rect(logos_core::RectLayer::new(10.0, 20.0, 100.0, 50.0));
            layer_id = layer.id();
            d.add_layer(layer).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_GET_LAYER_BY_ID).unwrap();
        rt.register_document(doc);
        // Pass the UUID string as the command (WAT reads ptr/len from params)
        let result = rt.execute(&layer_id.to_string());
        assert!(result.is_ok(), "get_layer_by_id failed: {:?}", result.err());
    }

    #[test]
    fn test_host_get_layer_by_id_not_found() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_GET_LAYER_BY_ID).unwrap();
        rt.register_document(make_doc());
        let fake_id = uuid::Uuid::new_v4().to_string();
        let result = rt.execute(&fake_id);
        assert!(result.is_ok(), "should return null, not error");
    }

    #[test]
    fn test_host_get_layer_by_id_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_GET_LAYER_BY_ID).unwrap();
        rt.register_document(make_doc());
        let fake_id = uuid::Uuid::new_v4().to_string();
        let result = rt.execute(&fake_id);
        assert!(result.is_err(), "should fail without DocumentRead");
    }

    // ── Test: host_create_text ───────────────────────────────

    #[test]
    fn test_host_create_text() {
        let doc = make_doc();
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_CREATE_TEXT).unwrap();
        rt.register_document(doc.clone());
        let result = rt.execute("test");
        assert!(result.is_ok(), "create_text failed: {:?}", result.err());
        // Verify the text layer was created
        let d = doc.read().unwrap();
        let root = d.root.read().unwrap();
        assert_eq!(root.layers.len(), 1);
        match &root.layers[0] {
            Layer::Text(t) => {
                assert_eq!(t.content, "Hello World");
            }
            other => panic!("expected Text layer, got {:?}", other),
        }
    }

    #[test]
    fn test_host_create_text_permission_denied() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_CREATE_TEXT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without DocumentWrite");
    }

    // ── Test: host_create_path ───────────────────────────────

    #[test]
    fn test_host_create_path() {
        let doc = make_doc();
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_CREATE_PATH).unwrap();
        rt.register_document(doc.clone());
        let result = rt.execute("test");
        assert!(result.is_ok(), "create_path failed: {:?}", result.err());
        // Verify the path layer was created
        let d = doc.read().unwrap();
        let root = d.root.read().unwrap();
        assert_eq!(root.layers.len(), 1);
        match &root.layers[0] {
            Layer::Path(p) => {
                assert_eq!(p.commands.len(), 3); // moveTo, lineTo, close
            }
            other => panic!("expected Path layer, got {:?}", other),
        }
    }

    #[test]
    fn test_host_create_path_permission_denied() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_CREATE_PATH).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without DocumentWrite");
    }

    // ── Test: host_set_selection ──────────────────────────────

    #[test]
    fn test_host_set_selection() {
        let doc = make_doc();
        let layer_id;
        {
            let d = doc.read().unwrap();
            let layer = Layer::Rect(logos_core::RectLayer::new(0.0, 0.0, 50.0, 50.0));
            layer_id = layer.id();
            d.add_layer(layer).unwrap();
        }
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_SET_SELECTION).unwrap();
        rt.register_document(doc.clone());
        // Pass JSON array of UUIDs as the command
        let json = format!("[\"{}\"]", layer_id);
        let result = rt.execute(&json);
        assert!(result.is_ok(), "set_selection failed: {:?}", result.err());
    }

    #[test]
    fn test_host_set_selection_permission_denied() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_SET_SELECTION).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("[]");
        assert!(result.is_err(), "should fail without DocumentWrite");
    }

    // ── Test: host_clear_selection ────────────────────────────

    #[test]
    fn test_host_clear_selection() {
        let mut rt = make_runtime(PermissionSet::document_full());
        rt.load_wat(WAT_CLEAR_SELECTION).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "clear_selection failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
    }

    #[test]
    fn test_host_clear_selection_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_CLEAR_SELECTION).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without DocumentWrite");
    }

    // ── Test: host_on_selection_changed ───────────────────────

    #[test]
    fn test_host_on_selection_changed() {
        let mut rt = make_runtime(PermissionSet::read_only());
        rt.load_wat(WAT_ON_SELECTION_CHANGED).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "on_selection_changed failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
    }

    #[test]
    fn test_host_on_selection_changed_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_ON_SELECTION_CHANGED).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without DocumentRead");
    }

    // ── Test: host_get_camera ────────────────────────────────

    #[test]
    fn test_host_get_camera_default() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_GET_CAMERA).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "get_camera failed: {:?}", result.err());
        // Default camera is (0, 0, zoom=1) — response is a buffer pointer
    }

    #[test]
    fn test_host_get_camera_after_set() {
        let mut rt = make_runtime(PermissionSet::none());
        // First set camera, then get it
        rt.set_camera(Camera::new(50.0, 75.0, 2.5));
        let cam = rt.get_camera();
        assert_eq!(cam.x, 50.0);
        assert_eq!(cam.y, 75.0);
        assert_eq!(cam.zoom, 2.5);
    }

    // ── Test: host_set_camera ────────────────────────────────

    #[test]
    fn test_host_set_camera() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_SET_CAMERA).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "set_camera failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
        // Verify camera was updated
        let cam = rt.get_camera();
        assert_eq!(cam.x, 100.0);
        assert_eq!(cam.y, 200.0);
        assert_eq!(cam.zoom, 2.0);
    }

    #[test]
    fn test_host_set_camera_invalid_zoom() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_SET_CAMERA_INVALID).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail with zoom=0.0");
    }

    // ── Test: host_screen_to_world ───────────────────────────

    #[test]
    fn test_host_screen_to_world() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_SCREEN_TO_WORLD).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "screen_to_world failed: {:?}", result.err());
        // Default camera (0,0,1.0) → screen coords = world coords
    }

    #[test]
    fn test_host_screen_to_world_with_camera() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.set_camera(Camera::new(100.0, 200.0, 2.0));
        rt.load_wat(WAT_SCREEN_TO_WORLD).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "screen_to_world with camera failed: {:?}", result.err());
    }

    // ── Test: host_show_toast ────────────────────────────────

    #[test]
    fn test_host_show_toast() {
        let mut rt = make_runtime(make_perms_with_notifications());
        rt.load_wat(WAT_SHOW_TOAST).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "show_toast failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
        // Verify notification was recorded
        let notes = rt.take_notifications();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], Notification::Toast("Layer created!".to_string()));
    }

    #[test]
    fn test_host_show_toast_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_SHOW_TOAST).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without Notifications");
    }

    // ── Test: host_confirm ───────────────────────────────────

    #[test]
    fn test_host_confirm() {
        let mut rt = make_runtime(make_perms_with_notifications());
        rt.load_wat(WAT_CONFIRM).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "confirm failed: {:?}", result.err());
        // Sandbox confirm always returns 1
        assert_eq!(result.unwrap(), PluginValue::Int(1));
        let notes = rt.take_notifications();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], Notification::Confirm("Delete this?".to_string()));
    }

    #[test]
    fn test_host_confirm_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_CONFIRM).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without Notifications");
    }

    // ── Test: host_prompt ────────────────────────────────────

    #[test]
    fn test_host_prompt() {
        let mut rt = make_runtime(make_perms_with_notifications());
        rt.load_wat(WAT_PROMPT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "prompt failed: {:?}", result.err());
        let notes = rt.take_notifications();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], Notification::Prompt("Enter name:".to_string()));
    }

    #[test]
    fn test_host_prompt_permission_denied() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_PROMPT).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_err(), "should fail without Notifications");
    }

    // ── Test: host_on_load ───────────────────────────────────

    #[test]
    fn test_host_on_load() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_ON_LOAD).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "on_load failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
    }

    // ── Test: host_on_unload ─────────────────────────────────

    #[test]
    fn test_host_on_unload() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_ON_UNLOAD).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "on_unload failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
    }

    // ── Test: host_on_frame ──────────────────────────────────

    #[test]
    fn test_host_on_frame() {
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(WAT_ON_FRAME).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok(), "on_frame failed: {:?}", result.err());
        assert_eq!(result.unwrap(), PluginValue::Int(1));
    }

    // ── Test: Notification enum ──────────────────────────────

    #[test]
    fn test_notification_variants() {
        let toast = Notification::Toast("msg".to_string());
        let confirm = Notification::Confirm("q?".to_string());
        let prompt = Notification::Prompt("name:".to_string());
        // Verify Debug and PartialEq
        assert_ne!(toast, confirm);
        assert_ne!(confirm, prompt);
        assert_eq!(toast.clone(), toast);
        assert_eq!(format!("{:?}", toast), r#"Toast("msg")"#);
    }

    // ── Test: Camera API ─────────────────────────────────────

    #[test]
    fn test_camera_default() {
        let cam = Camera::default();
        assert_eq!(cam.x, 0.0);
        assert_eq!(cam.y, 0.0);
        assert_eq!(cam.zoom, 1.0);
    }

    #[test]
    fn test_camera_screen_to_world() {
        let cam = Camera::new(100.0, 200.0, 2.0);
        let world = cam.screen_to_world(400.0, 300.0);
        // world = cam + screen / zoom
        // wx = 100 + 400 / 2 = 300
        // wy = 200 + 300 / 2 = 350
        assert!((world.x - 300.0).abs() < 0.001);
        assert!((world.y - 350.0).abs() < 0.001);
    }

    #[test]
    fn test_camera_world_to_screen() {
        let cam = Camera::new(100.0, 200.0, 2.0);
        let screen = cam.world_to_screen(300.0, 350.0);
        // screen = (world - cam) * zoom
        // sx = (300 - 100) * 2 = 400
        // sy = (350 - 200) * 2 = 300
        assert!((screen.x - 400.0).abs() < 0.001);
        assert!((screen.y - 300.0).abs() < 0.001);
    }

    #[test]
    fn test_camera_roundtrip() {
        let cam = Camera::new(50.0, 75.0, 3.0);
        let world = cam.screen_to_world(200.0, 300.0);
        let back = cam.world_to_screen(world.x, world.y);
        assert!((back.x - 200.0).abs() < 0.001);
        assert!((back.y - 300.0).abs() < 0.001);
    }

    // ── Test: parse_path_commands ─────────────────────────────

    #[test]
    fn test_parse_path_commands_valid() {
        let json = r#"[{"type":"moveTo","x":0,"y":0},{"type":"lineTo","x":100,"y":50},{"type":"close"}]"#;
        let cmds = parse_path_commands(json).unwrap();
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn test_parse_path_commands_quad() {
        let json = r#"[{"type":"quadTo","cx":50,"cy":100,"x":100,"y":0}]"#;
        let cmds = parse_path_commands(json).unwrap();
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_parse_path_commands_bezier() {
        let json = r#"[{"type":"bezierTo","cx1":1,"cy1":2,"cx2":3,"cy2":4,"x":5,"y":6}]"#;
        let cmds = parse_path_commands(json).unwrap();
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_parse_path_commands_invalid_type() {
        let json = r#"[{"type":"blah","x":0,"y":0}]"#;
        let result = parse_path_commands(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_path_commands_invalid_json() {
        let result = parse_path_commands("not json");
        assert!(result.is_err());
    }

    // ── Test: take_notifications drains ───────────────────────

    #[test]
    fn test_take_notifications_drains() {
        let mut rt = make_runtime(make_perms_with_notifications());
        rt.load_wat(WAT_SHOW_TOAST).unwrap();
        rt.register_document(make_doc());
        rt.execute("test").unwrap();
        let notes = rt.take_notifications();
        assert_eq!(notes.len(), 1);
        // Second take should be empty
        let notes2 = rt.take_notifications();
        assert_eq!(notes2.len(), 0);
    }

    // ── Test: multiple host calls accumulate across categories ─

    #[test]
    fn test_host_calls_across_categories() {
        // WAT that calls both get_camera and on_load
        let wat = r#"
            (module
                (import "logos" "host_get_camera" (func $cam (result i32)))
                (import "logos" "host_on_load" (func $load (result i32)))
                (memory (export "memory") 2)
                (func (export "logos_init") (result i32)
                    i32.const 0
                )
                (func (export "logos_execute") (param i32 i32) (result i32)
                    call $cam
                    drop
                    call $load
                )
            )
        "#;
        let mut rt = make_runtime(PermissionSet::none());
        rt.load_wat(wat).unwrap();
        rt.register_document(make_doc());
        let result = rt.execute("test");
        assert!(result.is_ok());
        assert_eq!(rt.host_calls(), 2);
    }
}
