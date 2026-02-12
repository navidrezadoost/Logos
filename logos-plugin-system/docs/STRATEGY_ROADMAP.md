# LOGOS: Executive Technical Roadmap & Engineering Plan

## 📋 Executive Summary

As your CTO, I will transform Penpot into Logos—a **next-generation design platform** that combines Figma's collaboration, Sketch's native performance, and XD's prototyping, while eliminating their collective weaknesses. This is not a fork; this is a **strategic rearchitecture** with Penpot as the foundation.

---

## 🔍 Phase 0: Technical Audit & Rebranding (Week 1)

### 0.1 Repository Rebranding Protocol

```bash
# Strict adherence to git history preservation
git clone https://github.com/penpot/penpot.git logos
cd logos

# Create comprehensive rename mapping document
cat > RENAME_MAPPING.md << EOF
# Logos Migration Mapping
Penpot Component -> Logos Component
penpot-core      -> logos-core
penpot-frontend  -> logos-studio
penpot-exporter  -> logos-export
penpot-plugins   -> logos-extensions
EOF
```

**Deliverables:**
- ✅ Fork with full commit history preserved
- ✅ `CHANGELOG.md` with migration entries
- ✅ `README.md` rewritten with Logos vision
- ✅ All internal references refactored (code, docs, env vars)
- ✅ CI/CD pipelines retargeted

### 0.2 Penpot Architecture Audit

**Current State Assessment:**

| Component | Tech Stack | Logos Target | Risk Level |
|-----------|-----------|--------------|------------|
| Frontend | ClojureScript, React | **Rewrite to Rust/WASM + Tauri** | 🔴 High |
| Backend | Clojure, PostgreSQL | Keep (modified) | 🟢 Low |
| Rendering | SVG, Canvas 2D | **wgpu + piet-gpu** | 🔴 High |
| Collaboration | WebSockets | **CRDT (Yjs) migration** | 🟡 Medium |
| Plugin System | JS iframe sandbox | **Deno isolates** | 🔴 High |

**Critical Decision:** We **do not** incrementally refactor ClojureScript. We build the new Rust core **alongside** existing code (Strangler Pattern).

---

## 🏗️ Phase 1: Core Architecture Redesign (Weeks 2-6)

### 1.1 Rust Core Foundation

```rust
// logos-core/src/lib.rs
pub struct LogosDocument {
    // CRDT-based object graph
    pub root: Arc<RwLock<SceneNode>>,
    pub history: HistoryBuffer<Delta>,
    pub collaborators: HashMap<PeerId, Cursor>,
}

pub enum SceneNode {
    Frame(FrameNode),
    Shape(ShapeNode),
    Text(TextNode),
    Component(ComponentNode),
    // Figma-style instance system
    Instance { master: Uuid, overrides: Vec<PropertyOverride> },
}

// Layout engine using Taffy (Flexbox/Grid native)
pub struct LayoutEngine {
    taffy_tree: TaffyTree<f32>,
    style_map: HashMap<NodeId, Style>,
}
```

**Performance Specifications:**
- **Memory:** Bounded object graph (max 10k nodes: ~50MB)
- **CPU:** Layout computation < 2ms per operation
- **Threading:** 3 dedicated threads (Layout, Render, IO)
- **Compile-time optimizations:** LTO, codegen-units=1

### 1.2 Dual Architecture Strategy

```
┌─────────────────┐     ┌─────────────────┐
│   Legacy Mode   │     │   Native Mode   │
│  (Penpot JS)    │────▶│   (Logos Core)  │
│  - Fallback     │     │   - Production  │
│  - Comparison   │     │   - Performance │
└─────────────────┘     └─────────────────┘
```

**Migration Path:**
1. Keep Penpot frontend serving static UI
2. Replace data layer with Rust WASM module
3. Incrementally replace canvas renderer
4. Final: Tauri shell with no JS dependencies

---

## 🎯 Phase 2: Competitor Feature Implementation (Weeks 7-16)

### 2.1 Figma's Secret Sauce (Reimplemented Better)

**Problem:** Figma's C++ WASM has GC pauses
**Solution:** Rust + no-GC + linear types

```rust
// Memory-efficient vector paths
pub struct VectorPath {
    // No garbage collection, explicit memory layout
    points: Vec<Point>,
    commands: Vec<PathCommand>,
    // GPU-ready format
    vertex_buffer: wgpu::Buffer,
}

// Auto-layout that doesn't freeze UI
pub fn compute_layout(nodes: &[SceneNode]) -> LayoutResult {
    // Offload to layout thread
    rayon::spawn(|| {
        let layout = taffy.compute_layout(/* ... */);
        // Send result back via channel
        layout_tx.send(layout).unwrap();
    });
}
```

**Benchmark Target:** 60fps at 10,000 layers (Figma: 30fps)

### 2.2 XD's Prototyping (As First-Class Citizen)

```rust
// State machine for interactions
pub struct InteractionGraph {
    states: HashMap<StateId, UIState>,
    transitions: Vec<Transition>,
    // Timeline animation engine
    animations: AnimationEngine,
}

pub struct AnimationEngine {
    // Spring physics, not linear
    springs: SpringSystem,
    easing: EasingCurve,
}
```

**Innovation:** Every component has a built-in state machine, not bolted-on.

### 2.3 Sketch's Native Power

**Tauri Implementation:**
```rust
// Access local fonts (impossible in Figma)
#[tauri::command]
fn get_system_fonts() -> Vec<FontInfo> {
    font_kit::system_fonts()
        .map(|font| FontInfo {
            family: font.family_name(),
            weight: font.weight(),
            style: font.style(),
        })
        .collect()
}

// Direct file system access
#[tauri::command]
fn save_to_file(path: PathBuf, data: Vec<u8>) -> Result<()> {
    tokio::fs::write(path, data).await?;
    Ok(())
}
```

### 2.4 Canva's Template Engine (Our Secret Weapon)

**AI-Assisted Design:**
```rust
pub struct TemplateEngine {
    // On-device ML (no cloud dependency)
    model: ort::Session,  // ONNX Runtime
    component_library: ComponentLibrary,
}

impl TemplateEngine {
    pub fn suggest_layout(&self, content: &[SceneNode]) -> Vec<LayoutSuggestion> {
        // Run inference on CPU thread
        // No data leaves user's machine
    }
}
```

---

## 📊 Phase 3: Quality Gates & Performance Benchmarks

### 3.1 Performance SLA

| Metric | Minimum | Target | Measurement |
|--------|---------|--------|------------|
| Startup Time | <3s | <1.5s | `time logos` |
| Frame Rate | 30fps | 120fps | `wgpu-profiler` |
| Memory (idle) | <200MB | <120MB | `heaptrack` |
| Memory (10k layers) | <1GB | <500MB | `heaptrack` |
| Save Time | <500ms | <100ms | `tracing` |
| Plugin Execution | <100ms | <30ms | Deno isolate |

### 3.2 Competitor Gap Analysis

**Our Advantages:**
1. **Offline-first:** No cloud dependency (unlike Figma)
2. **Real files:** Actual .logos files on disk (unlike Figma's cloud-only)
3. **System integration:** Native file picker, fonts, shortcuts
4. **Privacy:** No telemetry by default
5. **Performance:** Rust vs C++/WASM (better memory safety, similar speed)

**Gaps to Address:**
1. **Ecosystem:** Figma has 1000s of plugins
   - *Strategy:* Automatic Penpot plugin compatibility layer
2. **Community:** Established user base
   - *Strategy:* Zero-cost migration from Penpot/Figma

---

## 🗓️ Detailed Timeline & Milestones

### Week 1-2: Foundation
- **D1:** Repository rebrand complete
- **D3:** Core Rust crate structure finalized
- **D7:** First commit of `logos-core` with SceneNode
- **D10:** CI pipeline with cross-compilation (macOS/Windows/Linux)
- **D14:** Layout engine (Taffy) integrated, passing test suite

### Week 3-4: Visual Feedback
- **D17:** wgpu context creation successful
- **D21:** Rectangle rendering (1000 @ 60fps)
- **D24:** Basic hit testing (selection working)
- **D28:** Transformations (move, scale, rotate)

### Week 5-6: Editing Capabilities
- **D31:** Property system (each node has editable props)
- **D35:** Undo/Redo via CRDT deltas
- **D38:** Basic toolset (rectangle, ellipse, text placeholder)
- **D42:** Export to PNG/SVG

### Week 7-10: Advanced Features
- **D49:** Text engine (Cosmic-Text) with shaping
- **D56:** Auto-layout (Flexbox/Grid)
- **D63:** Component system (instances, overrides)
- **D70:** Prototyping interactions (clicks, delays, transitions)

### Week 11-14: Integration & Plugins
- **D77:** Deno plugin system (secure, async)
- **D84:** File format (.logos) specification
- **D91:** Collaboration foundations (CRDT sync)
- **D98:** Penpot file import compatibility

### Week 15-16: Polish & Beta
- **D105:** Performance optimization pass
- **D110:** Memory leak audit
- **D112:** Beta release preparation
- **D116:** Public beta launch

---

## 🚨 Risk Management Matrix

| Risk | Probability | Impact | Mitigation |
|------|------------|--------|------------|
| **Text rendering bugs** | High | Critical | Early integration of `cosmic-text`, extensive RTL/CJK test suite |
| **Rust learning curve** | Medium | Medium | Weekly knowledge sharing, pair programming on complex areas |
| **Plugin security** | Medium | Critical | Deno with strict capability flags, no `--allow-all` |
| **Collaboration latency** | Low | High | CRDT with compression, delta-based sync |
| **Feature parity gap** | High | Medium | Release with 80% features, prioritize most-used (shapes, text, frames) |
| **Migration complexity** | Medium | High | Strangler pattern, feature flags, canary releases |

---

## 💻 Development Standards & Engineering Excellence

### Code Quality Requirements
1. **100% of core logic** has property tests (QuickCheck)
2. **No unsafe code** in core (exceptions reviewed by CTO)
3. **Documentation:** Every public API has doc tests
4. **Benchmarks:** Every performance-sensitive function has Criterion benchmarks

### Review Process
1. All PRs require:
   - Performance impact assessment
   - Memory usage delta
   - Cross-platform test (Windows/Linux/macOS)
   - WASM compatibility check

### Tooling Stack
```toml
[workspace]
members = [
    "logos-core",        # Core data structures
    "logos-render",      # wgpu/piet-gpu
    "logos-layout",      # Taffy wrapper
    "logos-text",        # Cosmic-text integration
    "logos-collab",      # CRDT sync
    "logos-plugins",     # Deno host
    "logos-desktop",     # Tauri application
    "logos-wasm",        # WebAssembly target
]
```

---

## 🎯 Immediate Next Actions (Today)

1. **12:00 UTC:** Create private fork of Penpot
2. **14:00 UTC:** Distribute rename mapping to team
3. **16:00 UTC:** Initialize `logos-core` with README and license
4. **18:00 UTC:** First PR: `SceneNode` enum definition
5. **20:00 UTC:** Setup GitHub Projects with this roadmap

---

## 📈 Success Metrics (6 Months)

- **Technical:** 1,000 concurrent shapes @ 120fps
- **Community:** 500 GitHub stars, 20 contributors
- **Product:** Support for all major design file imports
- **Business:** Proof-of-concept for Figma plugin migration

---

**CTO Sign-off:** This roadmap is aggressive but achievable. The key is **discipline**—we don't add features until the core is solid. Every line of Rust we write today pays off in performance tomorrow.

Let's build the design tool we've always wanted. 🚀
