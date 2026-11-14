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
    setShapeTypeForCompatibilityRecovery: (id: string) => void;
    poisonRenderer: () => void;
    clearFailure: () => void;
    replaceTwoVectorNetworks: () => { firstId: string; secondId: string; replacementId: string; rootsBefore: string[]; rootsAfter: string[]; selection: string[] };
    samplePixel: (x: number, y: number) => number[];
    rendererInstances: () => number;
    wasmReservedBytes: () => number | null;
    stage: (name: string) => void;
    errors: () => string[];
  };
};
let failureMode: "none" | "always-after-mutation" = "none";
let renderer: RenderWasmModule | null = null;
let targetFramebuffer: WebGLFramebuffer | null | undefined;
let rendererInstanceCount = 0;
const errors: string[] = [];

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
};

function App(): React.ReactElement {
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
createRoot(document.getElementById("root")!).render(<App />);
void (async () => {
  try {
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
