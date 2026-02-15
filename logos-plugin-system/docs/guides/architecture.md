# Plugin System Architecture

Technical overview of the Logos plugin system's internal architecture, security model, and performance design.

---

## Layered Architecture

```
╔══════════════════════════════════════════════════════════════╗
║                     Plugin Code (JS / Sandbox)               ║
╠══════════════════════════════════════════════════════════════╣
║  Logos.* API    │  Logos.ui.*    │  Logos.on()   │  Logos.log ║
╠═════════════════╪════════════════╪═══════════════╪═══════════╣
║              Permission Guard (capability check)              ║
╠══════════════════════════════════════════════════════════════╣
║   Host API     │   UI Bridge    │  Event Bus    │  Undo Stack║
╠══════════════════════════════════════════════════════════════╣
║                    Plugin Manager                             ║
║          (lifecycle, loading, state machine)                  ║
╠══════════════════════════════════════════════════════════════╣
║  Sandbox Runtime        │     Boa JS Engine (ES2023)         ║
║  (expression evaluator) │     (full JavaScript)              ║
╠══════════════════════════════════════════════════════════════╣
║               logos-core (Document, Node, CRDT)               ║
╚══════════════════════════════════════════════════════════════╝
```

---

## Module Dependency Graph

```
logos-core
    │
    ▼
logos-plugins
    ├── manifest        (no deps)
    ├── permissions      (no deps)
    ├── signing          (no deps — pure-Rust SHA-256)
    ├── packaging        (manifest, signing)
    ├── runtime          (permissions)
    ├── host             (permissions, runtime, logos-core)
    ├── registry         (manifest, signing, packaging)
    ├── marketplace      (manifest, signing, packaging, registry)
    ├── manager          (manifest, permissions, runtime, host, engine)
    └── engine/
        ├── events       (boa_engine)
        ├── ui           (no deps)
        ├── host_api     (permissions, host, events, ui, logos-core, boa_engine)
        └── js_runtime   (permissions, runtime, events, ui, host_api, boa_engine)
```

---

## Runtime Architecture

### Dual Runtime System

Logos supports two runtimes, chosen based on the plugin's entry point:

| Runtime | File Extension | Capabilities | Performance |
|---------|---------------|--------------|-------------|
| **Sandbox** | `.lgpl`, `.expr` | Expression language, host functions | 41ns creation |
| **JavaScript** | `.js` | Full ES2023, Logos.* API, UI | 124µs first parse |

```rust
// Automatic runtime selection in PluginManager
match manifest.entry_point.as_str() {
    s if s.ends_with(".js") => PluginRuntime::JavaScript(JsEngine::new(...)),
    _ => PluginRuntime::Sandbox(Sandbox::new(...)),
}
```

### Plugin Lifecycle

```
                    load()
    ┌───────────────────────────────┐
    │                               ▼
 [Created] ──────► [Loaded] ──► [Ready] ──► [Running]
                       │           │            │
                       │           │         stop()
                       │           │            │
                       ▼           ▼            ▼
                   [Error] ◄─── [Stopped] ◄────┘
```

| State | Description |
|-------|-------------|
| `Loaded` | Manifest parsed, runtime allocated |
| `Ready` | Runtime initialized, globals set |
| `Running` | Plugin code executing |
| `Stopped` | Plugin terminated gracefully |
| `Error(msg)` | Plugin encountered a fatal error |

---

## Security Architecture

### Defense in Depth

```
Layer 1: Manifest Declaration
   │   Permissions declared at build time
   ▼
Layer 2: Permission Guard  
   │   Every API call checked at runtime (10ns)
   ▼
Layer 3: Resource Limits
   │   Memory, time, calls bounded
   ▼
Layer 4: Domain/Path Restrictions
   │   Network and file access narrowed
   ▼
Layer 5: Audit Trail
       Every denial logged for review
```

### Resource Limits

| Resource | Default Limit | Enforcement |
|----------|--------------|-------------|
| Memory | 50 MB | Checked per allocation |
| Execution time | 10 ms | Deadline-based timeout |
| Stack depth | 256 frames | Per-call check |
| Host calls | 10,000 | Counter per execution |
| Output size | 1 MB | Accumulated output check |

### Isolation Model

Each plugin runs in complete isolation:
- **No shared state** between plugins
- **No direct memory access** to host
- **No raw file system access** without permission
- **No network access** without permission + domain whitelist
- **Separate undo stack** per plugin instance

---

## Data Flow

### Document Read Path

```
Plugin calls Logos.getLayers()
    │
    ▼
PermissionGuard.check(DocumentRead)  ← 10ns
    │
    ▼
Document.read_lock()
    │
    ▼
HashMap<Uuid, Node>.values()
    │
    ▼
Node → JsObject conversion  ← per-layer serialization
    │
    ▼
Return JavaScript Array
```

### Document Write Path

```
Plugin calls Logos.createRect(x, y, w, h)
    │
    ▼
PermissionGuard.check(DocumentWrite)  ← 10ns
    │
    ▼
Document.write_lock()
    │
    ▼
Node::new(Rectangle) + set_transform()
    │
    ▼
Document.add_node(node, root_id)
    │
    ▼
CRDT sync (Yrs MapRef update)
    │
    ▼
UndoStack.push(DeleteAction(new_id))
    │
    ▼
EventBus.emit(LayerAdded { layerId })
    │
    ▼
Return UUID string
```

---

## UI Architecture

### Component Model

Logos uses a **declarative component model** — plugins describe UI as data, not DOM manipulation:

```
Plugin Code                    UI Bridge                   Renderer
    │                              │                          │
    ├─ createPanel({              │                          │
    │    components: [...]        │                          │
    │  })                         │                          │
    │                              │                          │
    │ ──── PanelSpec ────────────►│                          │
    │                              │── UiPanel struct ──────►│
    │                              │                          │
    │ ◄────── panelId ────────────│                          │
    │                              │                          │
    │ (user clicks button)        │                          │
    │                              │◄── ButtonClicked ───────│
    │ ◄── ButtonClicked ──────────│                          │
```

### Message Bus

UI communication uses a typed message protocol:

- **Plugin → Panel:** `SetComponents`, `UpdateValue`, `ShowNotification`, `SetTitle`
- **Panel → Plugin:** `ButtonClicked`, `ValueChanged`, `LayerSelected`, `PanelEvent`
- **Bidirectional:** `Custom`, `Request`/`Response`

Rate-limited to 60fps to prevent UI thrashing.

---

## Marketplace Architecture

```
┌─────────────────────────────────────────┐
│           MarketplaceClient              │
├──────────┬──────────┬───────────────────┤
│  Cache   │ Index    │ Trust System      │
│  (LRU)   │ (HashMap)│ (TrustedPubs)     │
├──────────┴──────────┴───────────────────┤
│          Package Verification            │
│  ┌──────────┬───────────┬─────────┐     │
│  │SHA-256   │ HMAC-256  │ Content │     │
│  │ Hash     │ Signature │ Verify  │     │
│  └──────────┴───────────┴─────────┘     │
├─────────────────────────────────────────┤
│      Plugin Listings (in-memory)         │
└─────────────────────────────────────────┘
```

### Performance Design

| Subsystem | Technique | Result |
|-----------|-----------|--------|
| Search | In-memory inverted index + TF-IDF | 5.32µs |
| Cache | LRU with TTL | 120ns hit |
| Trust | Bitmap + bloom filter | 19.6ns |
| Signing | Pure-Rust SHA-256/HMAC | 3.37µs |
| Package | Zero-copy binary format | 2.34µs parse |

---

## Cryptography

### Zero External Dependencies

All cryptographic primitives are implemented in pure Rust:

| Algorithm | Standard | Usage |
|-----------|----------|-------|
| SHA-256 | FIPS 180-4 | Content hashing |
| HMAC-SHA256 | RFC 2104 | Signature generation |
| FNV-1a | — | Fast hash for lookups |

This eliminates supply chain risk from external crypto crates and enables deterministic builds.

---

## Performance Summary

### Timing Budget (@ 3GHz)

```
Permission check:     10ns  =    30 cycles
Sandbox creation:     41ns  =   123 cycles
Trust check:        19.6ns  =    59 cycles
UI panel create:    191ns   =   573 cycles
Event dispatch:     200ns   =   600 cycles
UI roundtrip:       382ns   =  1,146 cycles
Host fn call:       ~3µs    =  ~9,000 cycles
JS evaluation:      1.3µs   =  3,900 cycles (cached)
Search:             5.32µs  = 15,960 cycles
Publish:            2.85µs  =  8,550 cycles
Download:           2.34µs  =  7,020 cycles
```

### Design Principles

1. **Cache everything** — LRU caches at every layer
2. **Zero allocation** — Preallocated buffers where possible
3. **O(1) lookups** — HashMap for all ID-based access
4. **Batch operations** — Events coalesced at 60fps
5. **Pure Rust** — No FFI overhead for crypto or core operations
