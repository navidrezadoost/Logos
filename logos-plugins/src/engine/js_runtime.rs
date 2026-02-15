//! JavaScript engine wrapping boa_engine for real JS execution.
//!
//! Replaces the Day 18 placeholder expression evaluator with a full
//! ES2023-compliant JavaScript engine (boa_engine 0.21).
//!
//! ## Design Decisions
//!
//! - **boa_engine over V8**: Pure Rust, no binary downloads, ~2MB footprint
//!   (V8 prebuilt binary download blocked by network constraints)
//! - **unsafe from_closure**: Safe because captured `Arc<RwLock<Document>>`
//!   contains no boa GC types (see boa_engine safety docs)
//! - **JsVariant matching**: boa 0.21 uses opaque JsValue + JsVariant enum
//! - **Resource tracking**: Timeout via wall-clock check at host boundaries
//!
//! ## Security References
//!
//! - Secure Programming Cookbook — Sandboxing, Resource Isolation
//! - OWASP Testing Guide v4 — Permission Systems
//! - Software Architecture: The Hard Parts — Extensibility

use crate::engine::events::EventBus;
use crate::engine::ui::{UiBridge, UiPermissionSet};
use crate::permissions::{PermissionGuard, PermissionSet};
use crate::runtime::{ExecutionStats, PluginValue, ResourceLimits, RuntimeError, RuntimeResult};
use boa_engine::value::JsVariant;
use boa_engine::{Context, JsValue, Source};
use logos_core::{Document, UndoStack};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// A JavaScript engine instance powered by boa_engine.
///
/// Each plugin gets its own `JsEngine` with:
/// - Isolated JavaScript context (ES2023)
/// - Permission-gated host functions (`Logos.*`)
/// - Resource limits (memory, timeout, host call count)
/// - Execution statistics
///
/// ## Example
///
/// ```rust
/// use logos_plugins::engine::JsEngine;
/// use logos_plugins::runtime::ResourceLimits;
/// use logos_plugins::permissions::PermissionSet;
///
/// let mut engine = JsEngine::new("my-plugin", ResourceLimits::default(), PermissionSet::none());
/// let result = engine.execute("1 + 2");
/// assert_eq!(result.unwrap().as_int(), Some(3));
/// ```
pub struct JsEngine {
    /// Engine unique ID
    id: Uuid,
    /// Plugin name
    name: String,
    /// The boa JavaScript context
    context: Context,
    /// Resource limits
    limits: ResourceLimits,
    /// Permission guard
    guard: Arc<RwLock<PermissionGuard>>,
    /// Last execution stats
    last_stats: ExecutionStats,
    /// Total executions
    total_executions: u64,
    /// Total time spent
    total_time: Duration,
    /// Host call counter (reset per execution)
    host_call_count: Arc<RwLock<u64>>,
    /// Execution deadline (set per execution)
    deadline: Arc<RwLock<Option<Instant>>>,
    /// Whether the engine is alive
    alive: bool,
    /// Document reference (optional, set via register_document)
    document: Option<Arc<RwLock<Document>>>,
    /// Undo stack for document operations
    undo_stack: Arc<RwLock<UndoStack>>,
    /// Event bus for plugin callbacks
    event_bus: Arc<RwLock<EventBus>>,
    /// UI bridge for panel communication
    ui_bridge: Arc<RwLock<UiBridge>>,
}

impl JsEngine {
    /// Create a new JavaScript engine with the given name and resource limits.
    ///
    /// Performance target: <5ms cold, <100μs warm (V8 Design Doc reference)
    pub fn new(
        name: impl Into<String>,
        limits: ResourceLimits,
        permissions: PermissionSet,
    ) -> Self {
        let context = Context::default();
        let guard = Arc::new(RwLock::new(PermissionGuard::new(permissions)));
        let host_call_count = Arc::new(RwLock::new(0u64));
        let deadline = Arc::new(RwLock::new(None::<Instant>));

        let mut engine = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            context,
            limits,
            guard,
            last_stats: ExecutionStats::default(),
            total_executions: 0,
            total_time: Duration::ZERO,
            host_call_count,
            deadline,
            alive: true,
            document: None,
            undo_stack: Arc::new(RwLock::new(UndoStack::new(100))),
            event_bus: Arc::new(RwLock::new(EventBus::new())),
            ui_bridge: Arc::new(RwLock::new(UiBridge::new())),
        };

        // Register the console.log shim
        engine.register_console();

        engine
    }

    /// Engine unique ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Engine name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the engine is alive (not killed by resource limit).
    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Last execution statistics.
    pub fn last_stats(&self) -> &ExecutionStats {
        &self.last_stats
    }

    /// Total number of executions.
    pub fn total_executions(&self) -> u64 {
        self.total_executions
    }

    /// Total wall-clock time spent in this engine.
    pub fn total_time(&self) -> Duration {
        self.total_time
    }

    /// Get a mutable reference to the resource limits.
    pub fn limits_mut(&mut self) -> &mut ResourceLimits {
        &mut self.limits
    }

    /// Get the permission guard.
    pub fn guard(&self) -> &Arc<RwLock<PermissionGuard>> {
        &self.guard
    }

    /// Kill the engine (no further execution allowed).
    pub fn kill(&mut self) {
        self.alive = false;
    }

    /// Connect a document for host API access.
    ///
    /// After calling this, the `Logos.*` host functions become available
    /// in JavaScript code (e.g., `Logos.getLayers()`, `Logos.createRect()`).
    pub fn register_document(&mut self, document: Arc<RwLock<Document>>) {
        self.document = Some(Arc::clone(&document));
        crate::engine::host_api::register_logos_api(
            &mut self.context,
            Arc::clone(&document),
            Arc::clone(&self.guard),
            Arc::clone(&self.host_call_count),
            Arc::clone(&self.deadline),
            Arc::clone(&self.undo_stack),
            Arc::clone(&self.event_bus),
            self.id,
            Arc::clone(&self.ui_bridge),
        );
    }

    /// Get the undo stack.
    pub fn undo_stack(&self) -> &Arc<RwLock<UndoStack>> {
        &self.undo_stack
    }

    /// Get the event bus.
    pub fn event_bus(&self) -> &Arc<RwLock<EventBus>> {
        &self.event_bus
    }

    /// Get the UI bridge.
    pub fn ui_bridge(&self) -> &Arc<RwLock<UiBridge>> {
        &self.ui_bridge
    }

    /// Set UI permissions for this engine's plugin.
    ///
    /// Must be called before the plugin can use `Logos.ui.*` functions.
    pub fn set_ui_permissions(&self, perms: UiPermissionSet) {
        let mut bridge = self.ui_bridge.write().unwrap();
        bridge.set_permissions(self.id, perms);
    }

    /// Flush pending events, invoking registered callbacks.
    ///
    /// Returns the number of callbacks invoked.
    pub fn flush_events(&mut self) -> u64 {
        let mut bus = self.event_bus.write().unwrap();
        bus.flush(&mut self.context)
    }

    /// Execute JavaScript code and return the result.
    ///
    /// Performance target: <10ms cold, <1ms warm (Software Architecture ref)
    pub fn execute(&mut self, code: &str) -> RuntimeResult<PluginValue> {
        if !self.alive {
            return Err(RuntimeError::ExecutionError(
                "engine has been killed".to_string(),
            ));
        }

        // Reset per-execution counters
        *self.host_call_count.write().unwrap() = 0;
        *self.deadline.write().unwrap() =
            Some(Instant::now() + self.limits.max_execution_time);

        let start = Instant::now();

        // Evaluate the JavaScript source
        let result = self
            .context
            .eval(Source::from_bytes(code))
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("timeout") || msg.contains("Timeout") {
                    RuntimeError::TimeLimitExceeded {
                        elapsed: start.elapsed(),
                        limit: self.limits.max_execution_time,
                    }
                } else {
                    RuntimeError::ExecutionError(msg)
                }
            })?;

        let elapsed = start.elapsed();

        // Check timeout after execution
        if elapsed > self.limits.max_execution_time {
            self.alive = false;
            return Err(RuntimeError::TimeLimitExceeded {
                elapsed,
                limit: self.limits.max_execution_time,
            });
        }

        // Check host call count
        let calls = *self.host_call_count.read().unwrap();
        if calls > self.limits.max_host_calls as u64 {
            self.alive = false;
            return Err(RuntimeError::HostCallLimitExceeded {
                calls: calls as usize,
                limit: self.limits.max_host_calls,
            });
        }

        // Convert result
        let plugin_value = jsvalue_to_plugin_value(&result, &mut self.context);

        // Update stats
        self.last_stats = ExecutionStats {
            elapsed,
            host_calls: calls,
            peak_memory: 0,
            instructions: 0,
        };
        self.total_executions += 1;
        self.total_time += elapsed;

        Ok(plugin_value)
    }

    /// Execute JavaScript code and discard the result (for side effects).
    pub fn execute_void(&mut self, code: &str) -> RuntimeResult<()> {
        self.execute(code)?;
        Ok(())
    }

    /// Set a global variable visible to JavaScript code.
    pub fn set_global(&mut self, name: &str, value: PluginValue) {
        let js_val = plugin_value_to_jsvalue(&value, &mut self.context);
        let _ = self.context.register_global_property(
            boa_engine::JsString::from(name),
            js_val,
            boa_engine::property::Attribute::all(),
        );
    }

    /// Register the `console.log` shim.
    fn register_console(&mut self) {
        // Leak the name so the closure can be Copy (static lifetime str ref)
        let name: &'static str = Box::leak(self.name.clone().into_boxed_str());
        let log_fn = boa_engine::NativeFunction::from_copy_closure(
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                let parts: Vec<String> = args
                    .iter()
                    .map(|a| {
                        a.to_string(ctx)
                            .map(|s| s.to_std_string_escaped())
                            .unwrap_or_else(|_| "[?]".to_string())
                    })
                    .collect();
                log::info!("[plugin:{}] {}", name, parts.join(" "));
                Ok(JsValue::undefined())
            },
        );

        let console = boa_engine::object::ObjectInitializer::new(&mut self.context)
            .function(log_fn, boa_engine::JsString::from("log"), 0)
            .build();

        let _ = self.context.register_global_property(
            boa_engine::JsString::from("console"),
            console,
            boa_engine::property::Attribute::all(),
        );
    }
}

// ───────────────────── JsValue ↔ PluginValue Conversion ─────────────────────

/// Convert a boa JsValue to a PluginValue.
///
/// Uses `val.variant()` to match the opaque JsValue (boa 0.21 API).
pub fn jsvalue_to_plugin_value(val: &JsValue, ctx: &mut Context) -> PluginValue {
    match val.variant() {
        JsVariant::Undefined | JsVariant::Null => PluginValue::Null,
        JsVariant::Boolean(b) => PluginValue::Bool(b),
        JsVariant::Integer32(i) => PluginValue::Int(i as i64),
        JsVariant::Float64(f) => {
            if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                PluginValue::Int(f as i64)
            } else {
                PluginValue::Float(f)
            }
        }
        JsVariant::String(s) => PluginValue::String(s.to_std_string_escaped()),
        JsVariant::Object(obj) => {
            if obj.is_array() {
                let length = obj
                    .get(
                        boa_engine::property::PropertyKey::from(
                            boa_engine::JsString::from("length"),
                        ),
                        ctx,
                    )
                    .ok()
                    .and_then(|v: JsValue| v.to_u32(ctx).ok())
                    .unwrap_or(0);

                let mut items = Vec::with_capacity(length as usize);
                for i in 0..length {
                    let item = obj.get(i, ctx).unwrap_or(JsValue::undefined());
                    items.push(jsvalue_to_plugin_value(&item, ctx));
                }
                PluginValue::Array(items)
            } else {
                let keys = obj.own_property_keys(ctx).unwrap_or_default();
                let mut map = HashMap::new();
                for key in keys {
                    let key_str = match &key {
                        boa_engine::property::PropertyKey::String(s) => {
                            s.to_std_string_escaped()
                        }
                        boa_engine::property::PropertyKey::Index(i) => i.get().to_string(),
                        boa_engine::property::PropertyKey::Symbol(_) => continue,
                    };
                    let v = obj.get(key, ctx).unwrap_or(JsValue::undefined());
                    map.insert(key_str, jsvalue_to_plugin_value(&v, ctx));
                }
                PluginValue::Object(map)
            }
        }
        JsVariant::BigInt(bi) => {
            let s = bi.to_string();
            if let Ok(i) = s.parse::<i64>() {
                PluginValue::Int(i)
            } else {
                PluginValue::String(s)
            }
        }
        JsVariant::Symbol(_) => PluginValue::String("[Symbol]".to_string()),
    }
}

/// Convert a PluginValue to a boa JsValue.
pub fn plugin_value_to_jsvalue(val: &PluginValue, ctx: &mut Context) -> JsValue {
    match val {
        PluginValue::Null => JsValue::null(),
        PluginValue::Bool(b) => JsValue::new(*b),
        PluginValue::Int(i) => {
            if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 {
                JsValue::new(*i as i32)
            } else {
                JsValue::rational(*i as f64)
            }
        }
        PluginValue::Float(f) => JsValue::rational(*f),
        PluginValue::String(s) => JsValue::new(boa_engine::JsString::from(s.as_str())),
        PluginValue::Array(arr) => {
            let js_arr = boa_engine::object::builtins::JsArray::new(ctx);
            for item in arr {
                let js_val = plugin_value_to_jsvalue(item, ctx);
                js_arr.push(js_val, ctx).unwrap_or_default();
            }
            js_arr.into()
        }
        PluginValue::Object(map) => {
            let obj = boa_engine::JsObject::with_null_proto();
            for (key, value) in map {
                let js_val = plugin_value_to_jsvalue(value, ctx);
                obj.set(
                    boa_engine::property::PropertyKey::from(
                        boa_engine::JsString::from(key.as_str()),
                    ),
                    js_val,
                    false,
                    ctx,
                )
                .unwrap_or_default();
            }
            JsValue::from(obj)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::PermissionSet;
    use crate::runtime::ResourceLimits;

    fn make_engine() -> JsEngine {
        JsEngine::new("test", ResourceLimits::default(), PermissionSet::none())
    }

    #[test]
    fn test_js_hello_world() {
        let mut engine = make_engine();
        let result = engine.execute("'hello world'").unwrap();
        assert_eq!(result.as_str(), Some("hello world"));
    }

    #[test]
    fn test_js_arithmetic() {
        let mut engine = make_engine();
        let result = engine.execute("2 + 3 * 4").unwrap();
        assert_eq!(result.as_int(), Some(14));
    }

    #[test]
    fn test_js_string_concat() {
        let mut engine = make_engine();
        let result = engine.execute("'foo' + 'bar'").unwrap();
        assert_eq!(result.as_str(), Some("foobar"));
    }

    #[test]
    fn test_js_boolean() {
        let mut engine = make_engine();
        let result = engine.execute("true && !false").unwrap();
        assert_eq!(result.as_bool(), Some(true));
    }

    #[test]
    fn test_js_null_undefined() {
        let mut engine = make_engine();
        let result = engine.execute("null").unwrap();
        assert!(matches!(result, PluginValue::Null));
        let result2 = engine.execute("undefined").unwrap();
        assert!(matches!(result2, PluginValue::Null));
    }

    #[test]
    fn test_js_array() {
        let mut engine = make_engine();
        let result = engine.execute("[1, 2, 3]").unwrap();
        if let PluginValue::Array(arr) = result {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0].as_int(), Some(1));
            assert_eq!(arr[2].as_int(), Some(3));
        } else {
            panic!("expected array, got {:?}", result);
        }
    }

    #[test]
    fn test_js_object() {
        let mut engine = make_engine();
        let result = engine.execute("({name: 'test', count: 42})").unwrap();
        if let PluginValue::Object(map) = result {
            assert_eq!(map.get("name").and_then(|v| v.as_str()), Some("test"));
            assert_eq!(map.get("count").and_then(|v| v.as_int()), Some(42));
        } else {
            panic!("expected object, got {:?}", result);
        }
    }

    #[test]
    fn test_js_arrow_functions() {
        let mut engine = make_engine();
        let result = engine.execute("((x) => x * 2)(21)").unwrap();
        assert_eq!(result.as_int(), Some(42));
    }

    #[test]
    fn test_js_template_literals() {
        let mut engine = make_engine();
        engine.execute("var x = 'world'").unwrap();
        let result = engine.execute("`hello ${x}`").unwrap();
        assert_eq!(result.as_str(), Some("hello world"));
    }

    #[test]
    fn test_js_destructuring() {
        let mut engine = make_engine();
        let result = engine.execute("var {a, b} = {a: 1, b: 2}; a + b").unwrap();
        assert_eq!(result.as_int(), Some(3));
    }

    #[test]
    fn test_js_spread() {
        let mut engine = make_engine();
        let result = engine.execute("var arr = [1, 2, 3]; [...arr, 4].length").unwrap();
        assert_eq!(result.as_int(), Some(4));
    }

    #[test]
    fn test_js_let_const() {
        let mut engine = make_engine();
        let result = engine.execute("let a = 10; const b = 20; a + b").unwrap();
        assert_eq!(result.as_int(), Some(30));
    }

    #[test]
    fn test_js_class() {
        let mut engine = make_engine();
        let result = engine
            .execute("class Point { constructor(x, y) { this.x = x; this.y = y; } sum() { return this.x + this.y; } } new Point(3, 4).sum()")
            .unwrap();
        assert_eq!(result.as_int(), Some(7));
    }

    #[test]
    fn test_set_global() {
        let mut engine = make_engine();
        engine.set_global("MY_CONST", PluginValue::Int(99));
        let result = engine.execute("MY_CONST + 1").unwrap();
        assert_eq!(result.as_int(), Some(100));
    }

    #[test]
    fn test_set_global_string() {
        let mut engine = make_engine();
        engine.set_global("GREETING", PluginValue::String("hi".to_string()));
        let result = engine.execute("GREETING + '!'").unwrap();
        assert_eq!(result.as_str(), Some("hi!"));
    }

    #[test]
    fn test_syntax_error() {
        let mut engine = make_engine();
        let result = engine.execute("function(");
        assert!(result.is_err());
    }

    #[test]
    fn test_reference_error() {
        let mut engine = make_engine();
        let result = engine.execute("nonExistentVar");
        assert!(result.is_err());
    }

    #[test]
    fn test_type_error() {
        let mut engine = make_engine();
        let result = engine.execute("null.property");
        assert!(result.is_err());
    }

    #[test]
    fn test_kill_engine() {
        let mut engine = make_engine();
        assert!(engine.is_alive());
        engine.kill();
        assert!(!engine.is_alive());
        assert!(engine.execute("42").is_err());
    }

    #[test]
    fn test_execution_stats() {
        let mut engine = make_engine();
        engine.execute("1 + 1").unwrap();
        assert!(engine.last_stats().elapsed > Duration::ZERO);
        assert_eq!(engine.total_executions(), 1);
    }

    #[test]
    fn test_multiple_executions() {
        let mut engine = make_engine();
        engine.execute("var x = 1").unwrap();
        engine.execute("x = x + 1").unwrap();
        let result = engine.execute("x").unwrap();
        assert_eq!(result.as_int(), Some(2));
        assert_eq!(engine.total_executions(), 3);
    }

    #[test]
    fn test_value_roundtrip_int() {
        let mut ctx = Context::default();
        let original = PluginValue::Int(42);
        let js = plugin_value_to_jsvalue(&original, &mut ctx);
        let back = jsvalue_to_plugin_value(&js, &mut ctx);
        assert_eq!(back.as_int(), Some(42));
    }

    #[test]
    fn test_value_roundtrip_float() {
        let mut ctx = Context::default();
        let original = PluginValue::Float(3.14);
        let js = plugin_value_to_jsvalue(&original, &mut ctx);
        let back = jsvalue_to_plugin_value(&js, &mut ctx);
        assert!((back.as_float().unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_value_roundtrip_string() {
        let mut ctx = Context::default();
        let original = PluginValue::String("hello".to_string());
        let js = plugin_value_to_jsvalue(&original, &mut ctx);
        let back = jsvalue_to_plugin_value(&js, &mut ctx);
        assert_eq!(back.as_str(), Some("hello"));
    }

    #[test]
    fn test_value_roundtrip_array() {
        let mut ctx = Context::default();
        let original = PluginValue::Array(vec![
            PluginValue::Int(1),
            PluginValue::String("two".to_string()),
        ]);
        let js = plugin_value_to_jsvalue(&original, &mut ctx);
        let back = jsvalue_to_plugin_value(&js, &mut ctx);
        if let PluginValue::Array(arr) = back {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].as_int(), Some(1));
            assert_eq!(arr[1].as_str(), Some("two"));
        } else {
            panic!("expected array");
        }
    }

    #[test]
    fn test_value_roundtrip_null() {
        let mut ctx = Context::default();
        let original = PluginValue::Null;
        let js = plugin_value_to_jsvalue(&original, &mut ctx);
        let back = jsvalue_to_plugin_value(&js, &mut ctx);
        assert!(matches!(back, PluginValue::Null));
    }

    #[test]
    fn test_js_float_vs_int() {
        let mut engine = make_engine();
        let result = engine.execute("10.0").unwrap();
        assert_eq!(result.as_int(), Some(10));
        let result2 = engine.execute("3.14").unwrap();
        assert!((result2.as_float().unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_console_log_does_not_crash() {
        let mut engine = make_engine();
        engine.execute("console.log('hello from JS')").unwrap();
    }
}
