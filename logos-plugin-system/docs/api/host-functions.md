# Host Functions Reference (Sandbox API)

This document covers the low-level sandbox host function API. Most plugin developers should use the [JavaScript API](javascript-api.md) instead. This reference is for developers building custom runtimes or extending the host API.

---

## Overview

Host functions are registered on a `Sandbox` instance and callable from plugin code via `host.function_name(args)` syntax in the sandbox expression language, or via the `Logos.*` global in the JavaScript engine.

---

## Registered Host Functions

### `get_document_info()`

**Permission:** `DocumentRead`

**Returns:** `PluginValue::Object`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "version": "1",
  "page_name": "Page 1",
  "layer_count": 5
}
```

---

### `get_layers()`

**Permission:** `DocumentRead`

**Returns:** `PluginValue::Array` of layer objects

Each layer:
```json
{
  "id": "...",
  "name": "Rectangle 1",
  "node_type": "Rectangle",
  "x": 100.0,
  "y": 200.0,
  "width": 300.0,
  "height": 150.0,
  "rotation": 0.0
}
```

---

### `get_layer_count()`

**Permission:** `DocumentRead`

**Returns:** `PluginValue::Int(count)`

---

### `get_layer(id)`

**Permission:** `DocumentRead`

**Parameters:**
- `id` — `PluginValue::String` with UUID

**Returns:** Layer object or `PluginValue::Null`

---

### `create_rect(x, y, width, height)`

**Permission:** `DocumentWrite`

**Parameters:**
- `x` — `PluginValue::Float`
- `y` — `PluginValue::Float`
- `width` — `PluginValue::Float`
- `height` — `PluginValue::Float`

**Returns:** `PluginValue::String` with UUID of created rectangle

---

### `delete_layer(id)`

**Permission:** `DocumentWrite`

**Parameters:**
- `id` — `PluginValue::String` with UUID

**Returns:** `PluginValue::Bool(true)` on success

---

### `log(message)`

**Permission:** None

**Parameters:**
- `message` — `PluginValue::String`

**Returns:** `PluginValue::Null`

Logs the message via `log::info!`.

---

## PluginValue Type System

The `PluginValue` enum is the universal type for all data crossing the host/plugin boundary.

| Variant | Rust Type | JavaScript Equivalent |
|---------|-----------|----------------------|
| `Null` | — | `null` |
| `Bool(bool)` | `bool` | `boolean` |
| `Int(i64)` | `i64` | `number` |
| `Float(f64)` | `f64` | `number` |
| `String(String)` | `String` | `string` |
| `Array(Vec<PluginValue>)` | `Vec` | `Array` |
| `Object(HashMap<String, PluginValue>)` | `HashMap` | `Object` |

### Conversion Methods

```rust
let val = PluginValue::String("hello".to_string());

// Type-safe accessors
val.as_str();    // Some("hello")
val.as_int();    // None
val.as_float();  // None
val.as_bool();   // None

// JSON conversion
let json = val.to_json(); // serde_json::Value
```

---

## Sandbox Configuration

### Resource Limits

```rust
use logos_plugins::ResourceLimits;

let limits = ResourceLimits {
    max_memory_bytes: 50 * 1024 * 1024,  // 50MB
    max_execution_time: Duration::from_millis(10),
    max_stack_depth: 256,
    max_host_calls: 10_000,
    max_output_bytes: 1024 * 1024,       // 1MB
};

let sandbox = Sandbox::new("my-plugin", limits);
```

### Custom Host Functions

```rust
use logos_plugins::{Sandbox, PluginValue, HostFn};
use std::sync::Arc;

let mut sandbox = Sandbox::new("test", ResourceLimits::default());

// Register a custom host function
sandbox.register_host_fn(
    "my_function",
    Arc::new(|args: &[PluginValue]| {
        let name = args.get(0)
            .and_then(|v| v.as_str())
            .unwrap_or("world");
        Ok(PluginValue::String(format!("Hello, {}!", name)))
    })
);

// Call from sandbox
let result = sandbox.execute("host.my_function(\"Logos\")")?;
assert_eq!(result.as_str(), Some("Hello, Logos!"));
```

### Global Variables

```rust
sandbox.set_global("version", PluginValue::String("1.0.0".into()));
sandbox.set_global("count", PluginValue::Int(42));

let result = sandbox.execute("return version")?;
```

---

## Execution Statistics

```rust
sandbox.execute("host.log(\"hello\")")?;

let stats = sandbox.last_stats().unwrap();
println!("Elapsed: {:?}", stats.elapsed);
println!("Host calls: {}", stats.host_calls);
println!("Peak memory: {} bytes", stats.peak_memory);
println!("Instructions: {}", stats.instructions);

// Aggregate stats
println!("Total executions: {}", sandbox.total_executions());
println!("Total time: {:?}", sandbox.total_time());
println!("Memory used: {} bytes", sandbox.memory_used());
```

---

## Runtime Errors

| Error | Cause |
|-------|-------|
| `CompileError(String)` | Code parsing failure |
| `ExecutionError(String)` | Runtime execution error |
| `MemoryLimitExceeded` | Exceeded `max_memory_bytes` |
| `TimeLimitExceeded` | Exceeded `max_execution_time` |
| `StackOverflow` | Exceeded `max_stack_depth` |
| `HostCallLimitExceeded` | Exceeded `max_host_calls` |
| `OutputLimitExceeded` | Exceeded `max_output_bytes` |
| `PermissionDenied(String)` | Missing required permission |
| `HostError(String)` | Host function returned error |
| `NotFound(String)` | Resource not found |

```rust
use logos_plugins::RuntimeError;

match sandbox.execute("invalid code") {
    Err(RuntimeError::CompileError(msg)) => {
        eprintln!("Compilation error: {}", msg);
    }
    Err(RuntimeError::TimeLimitExceeded) => {
        eprintln!("Plugin took too long!");
    }
    _ => {}
}
```
