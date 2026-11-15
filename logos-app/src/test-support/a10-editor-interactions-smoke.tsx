import * as React from "react";
import { createRoot } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { Inspector } from "../components/inspector/Inspector";
import { LayersPanel } from "../components/layers/LayersPanel";
import { Toolbar } from "../components/toolbar/Toolbar";
import { loadRenderWasm, type RenderWasmModule } from "../render-wasm/module";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import type { Shape } from "../types/shapes";
import initRust, { evaluate_document_operation_json } from "../document-core/rust-wasm/logos_types";
import { installRustShadowRuntime, type RustShadowEvidence } from "../document-core/rust-shadow-evaluation";

const status = document.getElementById("a10-status") as HTMLPreElement;
type WindowHarness = Window & {
  __a10?: {
    state: () => unknown;
    selection: () => string[];
    addPage: () => string;
    createVectorNetworkForCompatibility: () => string;
    createUnsupportedComponentForCompatibility: () => string;
    setShapeFlags: (id: string, flags: { hidden?: boolean; locked?: boolean }) => void;
    rejectInvalidChild: (parentId: string) => boolean;
    rejectLockedRename: (shapeId: string) => boolean;
    rejectHierarchyCycle: (parentId: string, descendantId: string) => boolean;
    setShapeTypeForCompatibilityRecovery: (id: string) => void;
    poisonRenderer: () => void;
    clearFailure: () => void;
    replaceTwoVectorNetworks: () => { firstId: string; secondId: string; replacementId: string; rootsBefore: string[]; rootsAfter: string[]; selection: string[] };
    samplePixel: (x: number, y: number) => number[];
    rendererInstances: () => number;
    wasmReservedBytes: () => number | null;
    stage: (name: string) => void;
    errors: () => string[];
    shadowEvidence: () => RustShadowEvidence[];
    shadowCommitCount: () => number;
    shadowWasmReservedBytes: () => number | null;
    shadowDiscardCount: () => number;
    exerciseStalePageSwitch: () => void;
    returnToFirstPage: () => void;
    unmountDuringShadow: () => void;
    runUnavailableEvaluator: () => void;
    setShadowFault: (mode: "rust-rejected" | "invalid-response" | "mismatch" | "throw" | "none") => void;
    runFaultedRename: () => void;
    runRepeatedRename: () => void;
    disposeShadowRuntime: () => void;
  };
};
let failureMode: "none" | "always-after-mutation" = "none";
let renderer: RenderWasmModule | null = null;
let targetFramebuffer: WebGLFramebuffer | null | undefined;
let rendererInstanceCount = 0;
const errors: string[] = [];
const shadowEvidence: RustShadowEvidence[] = [];
let shadowCommitCount = 0;
let shadowDiscardCount = 0;
let disposeShadowRuntime = () => {};
let shadowWasmReservedBytes: number | null = null;
let shadowFaultMode: "none" | "rust-rejected" | "invalid-response" | "mismatch" | "throw" = "none";
const shadowDocumentId = "10000000-0000-4000-8000-000000000001";
const shadowPageId = "10000000-0000-4000-8000-000000000002";

function stage(name: string): void {
  status.dataset.stage = name;
  status.textContent = `running: ${name}`;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function instrumentedLoader(jsUrl: string, wasmUrl: string): Promise<RenderWasmModule | null> {
  const mod = await loadRenderWasm(jsUrl, wasmUrl);
  if (!mod) return null;
  renderer = mod;
  rendererInstanceCount += 1;
  const originalInit = mod._init.bind(mod);
  mod._init = (width, height) => {
    const canvas = document.querySelector<HTMLCanvasElement>("canvas");
    const gl = canvas?.getContext("webgl2");
    assert(gl, "production Canvas did not create WebGL2");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    originalInit(width, height);
  };
  const original = mod._set_shape_base_props.bind(mod);
  mod._set_shape_base_props = () => {
    original();
    if (failureMode === "always-after-mutation") throw new Error("A10 injected upload failure");
  };
  return mod;
}

function samplePixel(x: number, y: number): number[] {
  const canvas = document.querySelector<HTMLCanvasElement>("canvas");
  const gl = canvas?.getContext("webgl2");
  assert(canvas && gl && targetFramebuffer !== undefined, "renderer framebuffer unavailable");
  const pixel = new Uint8Array(4);
  gl.finish();
  const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
  try {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, targetFramebuffer);
    gl.readBuffer(targetFramebuffer === null ? gl.BACK : gl.COLOR_ATTACHMENT0);
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
  } finally {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous);
  }
  return Array.from(pixel);
}

function currentPageShapes(): Shape[] {
  const state = useDocumentStore.getState();
  const pending = [...(state.pages[state.currentPageId]?.rootShapeIds ?? [])];
  const seen = new Set<string>();
  while (pending.length > 0) {
    const id = pending.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    pending.push(...(state.shapes[id]?.children ?? []));
  }
  return [...seen].flatMap((id) => state.shapes[id] ? [state.shapes[id]] : []);
}

useUiStore.setState({ zoom: 1, panX: 0, panY: 0, activeTool: "select" });
(window as WindowHarness).__a10 = {
  state: () => {
    const { pages, pageOrder, currentPageId, shapes } = useDocumentStore.getState();
    return { pages, pageOrder, currentPageId, shapes };
  },
  selection: () => useSelectionStore.getState().selectedIds,
  addPage: () => useDocumentStore.getState().addPage("A10 second page"),
  createVectorNetworkForCompatibility: () => {
    const id = useDocumentStore.getState().addVectorNetwork([{ x: 90, y: 280 }, { x: 120, y: 310 }], [{ s: 0, e: 1 }]);
    useSelectionStore.getState().select(id);
    return id;
  },
  createUnsupportedComponentForCompatibility: () => {
    const store = useDocumentStore.getState();
    const base = Object.values(store.shapes).find((shape) => shape.type === "rect");
    if (!base) throw new Error("Create a rectangle before adding the compatibility component fixture");
    const id = crypto.randomUUID();
    store.addShape({
      ...base,
      id,
      type: "component",
      name: "A10 unsupported compatibility component",
      bounds: { ...base.bounds, x: base.bounds.x + 40 },
      parentId: null,
      children: [],
      componentMeta: { properties: {} },
      vendorExtension: { sentinel: "preserved-by-compatibility-operation" },
    } as Shape);
    useSelectionStore.getState().select(id);
    return id;
  },
  setShapeFlags: (id, flags) => useDocumentStore.getState().updateShape(id, flags),
  rejectInvalidChild: (parentId) => {
    const before = useDocumentStore.getState();
    const selectedBefore = useSelectionStore.getState().selectedIds;
    const parent = before.shapes[parentId];
    if (!parent) return false;
    const invalid = { ...parent, id: crypto.randomUUID(), type: "rect", name: "Rejected child", parentId: null, children: [] } as Shape;
    const result = before.applyHierarchyOperation({ kind: "insert-child", parentId, shape: invalid, index: 999999 });
    return !result.ok && useDocumentStore.getState().shapes === before.shapes &&
      useDocumentStore.getState().pages === before.pages &&
      useSelectionStore.getState().selectedIds === selectedBefore;
  },
  rejectLockedRename: (shapeId) => {
    const state = useDocumentStore.getState();
    const selectedBefore = useSelectionStore.getState().selectedIds;
    const shape = state.shapes[shapeId];
    if (!shape) return false;
    useDocumentStore.getState().updateShape(shapeId, { locked: true });
    const lockedPages = useDocumentStore.getState().pages;
    const lockedShapes = useDocumentStore.getState().shapes;
    const selectedLocked = useSelectionStore.getState().selectedIds;
    const result = useDocumentStore.getState().applyHierarchyOperation({ kind: "rename", shapeId, name: "must reject locked" });
    const rejected = !result.ok && useDocumentStore.getState().pages === lockedPages &&
      useDocumentStore.getState().shapes === lockedShapes && useSelectionStore.getState().selectedIds === selectedLocked;
    useDocumentStore.getState().updateShape(shapeId, { locked: false });
    return rejected && selectedBefore === selectedLocked;
  },
  rejectHierarchyCycle: (parentId, descendantId) => {
    useDocumentStore.getState().updateShape(parentId, { locked: false });
    const before = useDocumentStore.getState();
    const selectedBefore = useSelectionStore.getState().selectedIds;
    const result = before.applyHierarchyOperation({ kind: "reparent", shapeId: parentId, parentId: descendantId, preserveWorldPosition: true });
    const rejectedUnchanged = !result.ok && useDocumentStore.getState().pages === before.pages &&
      useDocumentStore.getState().shapes === before.shapes && useSelectionStore.getState().selectedIds === selectedBefore;
    useDocumentStore.getState().updateShape(parentId, { locked: true });
    return rejectedUnchanged;
  },
  setShapeTypeForCompatibilityRecovery: (id) => useDocumentStore.getState().updateShape(id, { type: "rect" }),
  poisonRenderer: () => {
    failureMode = "always-after-mutation";
    const shape = currentPageShapes()[0];
    if (!shape) throw new Error("create a shape before poisoning renderer");
    useDocumentStore.getState().updateShape(shape.id, { opacity: Math.max(0.1, shape.opacity - 0.1) });
  },
  clearFailure: () => { failureMode = "none"; },
  replaceTwoVectorNetworks: () => {
    const store = useDocumentStore.getState();
    const rootsBefore = [...store.pages[store.currentPageId].rootShapeIds];
    const firstId = store.addVectorNetwork([{ x: 700, y: 300 }, { x: 725, y: 325 }], [{ s: 0, e: 1 }]);
    const secondId = useDocumentStore.getState().addVectorNetwork([{ x: 740, y: 300 }, { x: 765, y: 325 }], [{ s: 0, e: 1 }]);
    useSelectionStore.getState().select(firstId);
    useSelectionStore.getState().toggleSelect(secondId);
    const replacementId = useDocumentStore.getState().replaceWithVectorNetwork(
      [firstId, secondId], [{ x: 700, y: 300 }, { x: 765, y: 325 }], [{ s: 0, e: 1 }],
    );
    const next = useDocumentStore.getState();
    return {
      firstId,
      secondId,
      replacementId,
      rootsBefore,
      rootsAfter: [...next.pages[next.currentPageId].rootShapeIds],
      selection: [...useSelectionStore.getState().selectedIds],
    };
  },
  samplePixel,
  rendererInstances: () => rendererInstanceCount,
  wasmReservedBytes: () => renderer?.HEAPU8.buffer.byteLength ?? null,
  stage,
  errors: () => [...errors],
  shadowEvidence: () => [...shadowEvidence],
  shadowCommitCount: () => shadowCommitCount,
  shadowWasmReservedBytes: () => shadowWasmReservedBytes,
  shadowDiscardCount: () => shadowDiscardCount,
  exerciseStalePageSwitch: () => {
    const state = useDocumentStore.getState();
    const existing = Object.values(state.shapes).find((shape) => shape.type === "rect");
    if (!existing) throw new Error("stale-result probe requires a rectangle");
    const operation = { kind: "rename", shapeId: existing.id, name: `${existing.name} stale-probe` } as const;
    state.applyHierarchyOperation(operation);
    const nextPage = state.pageOrder.find((id) => id !== state.currentPageId);
    if (nextPage) useDocumentStore.getState().setCurrentPage(nextPage);
  },
  returnToFirstPage: () => {
    const state = useDocumentStore.getState();
    state.setCurrentPage(state.pageOrder[0]);
  },
  unmountDuringShadow: () => {
    const state = useDocumentStore.getState();
    const existing = currentPageShapes().find((shape) => shape.type === "rect" && shape.parentId === null);
    if (!existing) throw new Error("unmount probe requires a root Rectangle");
    state.applyHierarchyOperation({ kind: "rename", shapeId: existing.id, name: `${existing.name} unmount-probe` });
    appRoot.unmount();
  },
  runUnavailableEvaluator: () => {
    const state = useDocumentStore.getState();
    const existing = currentPageShapes().find((shape) => shape.type === "rect");
    if (!existing) throw new Error("unavailable probe requires a Rectangle");
    disposeShadowRuntime();
    disposeShadowRuntime = installRuntime(false);
    state.applyHierarchyOperation({ kind: "rename", shapeId: existing.id, name: `${existing.name} unavailable-probe` });
    disposeShadowRuntime();
    disposeShadowRuntime = installRuntime();
  },
  setShadowFault: (mode) => { shadowFaultMode = mode; },
  runFaultedRename: () => {
    const existing = currentPageShapes().find((shape) => shape.type === "rect");
    if (!existing) throw new Error("fault probe requires a Rectangle");
    useDocumentStore.getState().applyHierarchyOperation({ kind: "rename", shapeId: existing.id, name: `${existing.name} fault-probe` });
  },
  runRepeatedRename: () => {
    const existing = currentPageShapes().find((shape) => shape.type === "rect");
    if (!existing) throw new Error("repeat probe requires a Rectangle");
    const state = useDocumentStore.getState();
    state.applyHierarchyOperation({ kind: "rename", shapeId: existing.id, name: `${existing.name} repeat-a` });
    useDocumentStore.getState().applyHierarchyOperation({ kind: "rename", shapeId: existing.id, name: `${existing.name} repeat-b` });
  },
  disposeShadowRuntime: () => disposeShadowRuntime(),
};

function App(): React.ReactElement {
  React.useEffect(() => () => disposeShadowRuntime(), []);
  const [, setPageSwitchTick] = React.useState(0);
  return (
    <div style={{ width: "100%", height: "100%", display: "flex" }}>
      <Toolbar />
      <LayersPanel />
      <div style={{ display: "flex", flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", flex: 1, minWidth: 0 }}><Canvas rendererLoader={instrumentedLoader} /></div>
        <Inspector />
      </div>
      <button data-testid="a10-switch-page" style={{ position: "fixed", top: 4, right: 4, zIndex: 200 }} onClick={() => {
        const state = useDocumentStore.getState();
        const nextPage = state.pageOrder.find((id) => id !== state.currentPageId) ?? useDocumentStore.getState().addPage("A10 second page");
        useDocumentStore.getState().setCurrentPage(nextPage);
        setPageSwitchTick((tick) => tick + 1);
      }}>Switch page</button>
    </div>
  );
}

window.addEventListener("error", (event) => errors.push(event.message));
const appRoot = createRoot(document.getElementById("root")!);
useDocumentStore.subscribe((next, previous) => {
  if (next.pages !== previous.pages || next.shapes !== previous.shapes) shadowCommitCount += 1;
});
void (async () => {
  try {
    const wasmExports = await initRust();
    shadowWasmReservedBytes = wasmExports.memory.buffer.byteLength;
    const oldPage = useDocumentStore.getState().pages[useDocumentStore.getState().currentPageId];
    const hydrated = useDocumentStore.getState().hydrateDocument({
      pages: { [shadowPageId]: { ...oldPage, id: shadowPageId } },
      pageOrder: [shadowPageId], currentPageId: shadowPageId, shapes: {},
    });
    if (!hydrated.ok) throw new Error(`shadow fixture hydration failed: ${hydrated.diagnostic.code}`);
    (globalThis as typeof globalThis & { __LOGOS_G1_B9_SHADOW_OPT_IN__?: boolean }).__LOGOS_G1_B9_SHADOW_OPT_IN__ = true;
    disposeShadowRuntime = installRuntime();
    if (!(wasmExports.memory.buffer.byteLength > 0)) throw new Error("candidate Rust WASM memory was unavailable");
    appRoot.render(<App />);
    const started = performance.now();
    while (document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") !== "READY") {
      if (performance.now() - started > 20000) throw new Error("initial renderer did not reach READY");
      await new Promise((resolve) => setTimeout(resolve, 30));
    }
    status.dataset.status = "ready";
    status.textContent = JSON.stringify({ ready: true, userAgent: navigator.userAgent });
  } catch (error) {
    status.dataset.status = "failed";
    status.textContent = error instanceof Error ? error.stack ?? error.message : String(error);
  }
})();

function installRuntime(evaluatorAvailable = true): () => void {
  return installRustShadowRuntime({
    documentId: shadowDocumentId,
    ...(evaluatorAvailable ? { evaluator: { evaluateDocumentOperationJson: (documentJson: string, operationJson: string) => {
      const fault = shadowFaultMode;
      shadowFaultMode = "none";
      if (fault === "throw") throw new Error("injected Rust evaluator exception");
      if (fault === "invalid-response") return "{invalid-json";
      if (fault === "rust-rejected") return JSON.stringify({ ok: false, diagnostics: [{ code: "CYCLE", path: "/operation" }] });
      const raw = evaluate_document_operation_json(documentJson, operationJson);
      if (fault === "mismatch") {
        const response = JSON.parse(raw);
        if (response.document?.shapes?.[0]) response.document.shapes[0].name += " shadow mismatch";
        return JSON.stringify(response);
      }
      return raw;
    } } } : {}),
    report: (item) => shadowEvidence.push({
      ...item,
      jsHeapUsedBytes: (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize ?? null,
      wasmReservedBytes: shadowWasmReservedBytes,
    }),
    discard: () => { shadowDiscardCount += 1; },
  });
}
