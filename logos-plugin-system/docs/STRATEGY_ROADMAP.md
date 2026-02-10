# LOGOS: Strategic Roadmap & Technical Analysis

## 1. Vision
**Logos** targets the intersection of performance, openness, and native-web hybridity. It aims to replace the "walled garden" of Figma with an open-source ecosystem that offers the native performance of Sketch and the prototyping power of XD, backed by Rust and WebAssembly.

---

## 2. Competitor Feature Matrix & Integration Strategy

### **Figma (The Target to Beat)**
| Competitor Strength | Logos Implementation Strategy |
| :--- | :--- |
| **WebAssembly Performance** | Use **Rust compiled to WASM**. Unlike Figma's C++, Rust offers memory safety without garbage collection pauses. |
| **Real-time Collab** | Implement **CRDTs (Yjs or Automerge)** at the core object model level. Every object (Rect, Frame, Text) is a CRDT node. |
| **Auto-Layout** | Implement a **Flexbox/Grid engine in Rust** (using `Morphorm` or `Taffy` libraries) that runs natively, not in JS. |

### **Adobe XD (The Workflow)**
| Competitor Strength | Logos Implementation Strategy |
| :--- | :--- |
| **Prototyping/Animation** | Treat transitions as first-class schemas. Use a **State Machine** in the core to handle interactive flows. |
| **Simplicity** | Minimalist UI. Keep the "Chrome" (toolbars) separate from the "Canvas" (rendering context). |

### **Sketch (The Native Feel)**
| Competitor Strength | Logos Implementation Strategy |
| :--- | :--- |
| **Native APIs** | Use **Tauri**. This allows Logos to access the local filesystem, system fonts, and hardware devices (unlike browser-sandboxed Figma). |
| **Third-party Freedom** | The **Deno-based Plugin System**. Secure by default, but capable of spawning processes, reading files, and network requests if permissions are granted. |

---

## 3. Technical Architecture Roadmap

### **Core Stack**
*   **Language:** Rust (2024 Edition)
*   **Scripting Host:** Deno Core (V8) with granular permissions.
*   **Rendering:** **Piet-GPU** (Vector optimization) + **WGPU** (WebGPU backend).
*   **Text Engine:** **Cosmic-Text** (Advanced shaping/layout).
*   **Layout Engine:** **Taffy** (Flexbox) + **Morphorm** (Constraint-based).
*   **Frontend Shell:** **Tauri** (Desktop-first) -> WebAssembly (Web port later).
*   **Data Layer:** **Delta-based CRDT** (Yjs/Automerge) for granular updates.

### **Architecture Diagram**

```text
[ Desktop App (Tauri) ]      [ Web App (WASM) ]
          \                       /
           \                     /
            [   Logos Core (Lib)   ]
            /          |          \
   [Object Store]  [Renderer]  [Plugin Host]
   (Arc<RwLock>)   (Piet/WGPU)   (Deno V8)
         |             |            |
     [CRDT/Yjs]    [GPU/Thread]  [Sandbox]
```

### **Concurrency Model**
*   **Main Thread:** UI/Input handling.
*   **Layout Thread:** Taffy/Morphorm calculations.
*   **Render Thread:** WGPU command encoding.
*   **Plugin Thread:** Isolated Deno V8 instances.

---

## 4. Current Status (Feb 2026)
*   **Repository Structure:** Established.
*   **Plugin System:** Prototype phase (`logos-plugin-system` exists).
*   **Hiring/Ops:** Reset. Ready for pure technical execution.
*   **Missing Critical Modules:**
    1.  **Vector Rendering Engine:** No active canvas code.
    2.  **Object Model:** No defined schema for Shapes/Layers.
    3.  **Frontend entry point:** No visual interface.

---

## 5. Development Phases

### Phase A: The Core (Foundation)
**Objective:** A "headless" design tool that can manipulate a scene graph via tests.
*   Define the `Node` struct (Layer, Group, Frame).
*   Implement `Taffy` for layout calculation.
*   Build the serialization format (Logos File Format - .logs).

### Phase B: The Renderer (Visualization)
**Objective:** See the graph on a screen.
*   Implement WGPU integration.
*   Render primitives (Rect, Ellipse, Path).
*   Implement Zoom/Pan mechanics (Affine Transformations).

### Phase C: Interaction & Plugins (Usability)
**Objective:** Edit the graph with mouse & code.
*   Connect the `logos-plugin-system` to the Core.
*   Implement Selection logic (Hit testing).
*   Build the Property Panel UI.

---

## 6. Challenges & Solutions
1.  **Text Rendering:** Text is notoriously hard.
    *   *Solution:* Use `Cosmic-Text` (Rust library) for shaping and layout.
2.  **Performance at Scale:** 10k nodes lag in DOM.
    *   *Solution:* GPU Instancing. Changing a color shouldn't trigger a re-layout, just a uniform buffer update.
3.  **Collaboration Latency:**
    *   *Solution:* Local-first architecture. The app works 100% offline and syncs deltas when connection restores.

---

## 7. Recommended Next Steps (Immediate Action Plan)

### **Week 1: Core Engine Foundation**
1.  **Project Initialization:**
    *   Initialize `logos-core` (Library)
    *   Initialize `logos-desktop` (Tauri)
2.  **Object Model Implementation:**
    *   Define `Node` Enum (Rect, Text, Frame, Component).
    *   Implement Delta-based CRDT structures.
3.  **CI/CD:** Setup GitHub Actions for Rust/WASM.

### **Week 2: Basic Rendering Pipeline**
1.  **Graphics Stack:** Integrate `wgpu` and `piet-gpu`.
2.  **Renderer Implementation:** Build `Renderer` struct with wgpu device/queue.
3.  **Proof of Concept:** Render 1,000 rectangles at 60fps.

### **Week 3: Basic Editor & Interaction**
1.  **Tauri Setup:** Connect Core logic to Tauri frontend.
2.  **Tooling:** Implement basic Rectangle tool and Selection logic.
3.  **Property Panel:** Basic wireframe UI.

### **Phase 1 Success Criteria**
*   **Technical:** Rust core compiles to WASM, >60fps with 1k objects, Undo/Redo functional.
*   **Usability:** Create/Move/Resize shapes, Basic Layer Panel, Export PNG.
*   **Performance:** Startup < 2s, Memory < 200MB.

## 8. Critical Risks & Mitigation
*   **Text Rendering:** Complex shaping (RTL/Emojis). *Mitigation:* Early integration of `Cosmic-Text`.
*   **Plug-in Security:** V8 Escape. *Mitigation:* Resource constrained Deno isolates (512MB RAM cap).
*   **Concurrency:** Deadlocks in `Arc<RwLock>`. *Mitigation:* Message passing (Channels) where possible.
