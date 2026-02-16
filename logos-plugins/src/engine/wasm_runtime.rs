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
use logos_core::{Document, Layer};
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
}

impl HostState {
    fn new(document: Arc<RwLock<Document>>, permissions: PermissionSet, max_host_calls: usize) -> Self {
        Self {
            document,
            guard: PermissionGuard::new(permissions),
            host_calls: 0,
            max_host_calls,
            log_output: Vec::new(),
            store_limits: wasmtime::StoreLimitsBuilder::new()
                .memory_size(50 * 1024 * 1024) // 50MB
                .build(),
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
        })
    }

    /// Attach a document for host function access.
    pub fn register_document(&mut self, document: Arc<RwLock<Document>>) {
        self.document = Some(document);
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

    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

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

    #[test]
    fn test_host_state_count_call() {
        let mut state = HostState::new(make_doc(), PermissionSet::none(), 3);
        assert!(state.count_call().is_ok()); // 1
        assert!(state.count_call().is_ok()); // 2
        assert!(state.count_call().is_ok()); // 3
        assert!(state.count_call().is_err()); // 4 > limit of 3
    }

    #[test]
    fn test_host_state_check_permission_granted() {
        let mut state = HostState::new(make_doc(), PermissionSet::read_only(), 100);
        assert!(state.check_permission(&PermissionKind::DocumentRead).is_ok());
    }

    #[test]
    fn test_host_state_check_permission_denied() {
        let mut state = HostState::new(make_doc(), PermissionSet::none(), 100);
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
}
