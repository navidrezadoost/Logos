import * as React from "react";
import { createRoot } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { Inspector } from "../components/inspector/Inspector";
import { LayersPanel } from "../components/layers/LayersPanel";
import { Toolbar } from "../components/toolbar/Toolbar";
import { loadRenderWasm } from "../render-wasm/module";
import initTypes, { evaluate_document_operation_json } from "../document-core/rust-wasm/logos_types";
import { evaluateHierarchyOperationConformance } from "../document-core/operation-conformance";
import { buildG1B95Fixture, makeG1B95Operation, type G1B95Fixture, type G1B95OperationKind, type G1B95Size } from "./g1-b9-5-workload";
import { useDocumentStore, type Page } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import type { HierarchyOperation } from "../stores/hierarchyOperations";
import type { Shape } from "../types/shapes";

const status = document.getElementById("g1b95-status") as HTMLPreElement;
const kinds: G1B95OperationKind[] = ["create", "insert-child", "rename", "sibling-reorder", "root-reorder", "subtree-delete", "reparent"];
type Stage = { stage: string; milliseconds: number };
type LastResult = {
  ok: boolean; diagnostic?: string; operation: G1B95OperationKind; storeActionMs: number;
  eventDispatchDelayMs: number; eventTimeStamp: number; uiSelectionUpdateMs: number;
  stages: Stage[]; candidate: unknown; selectionIds: string[];
};
type G1B95Window = Window & {
  __g1b95?: {
    prepare: (size: G1B95Size, operation: G1B95OperationKind, serial: number) => Promise<void>;
    fixture: () => Omit<G1B95Fixture, "shapes">;
    state: () => { page: Page; shapes: Record<string, Shape>; currentPageId: string };
    lastResult: () => LastResult | null;
    samplePixel: (x: number, y: number) => number[];
    errors: () => string[];
  };
  __logosG1B95Probe?: (stage: string, milliseconds: number) => void;
};

let fixture: G1B95Fixture | null = null;
let preparedOperation: G1B95OperationKind | null = null;
let preparedSerial = 0;
let candidate: unknown = null;
let lastResult: LastResult | null = null;
let currentStages: Stage[] = [];
let rendererMemoryBytes: number | null = null;
const errors: string[] = [];
let rendererInstanceCount = 0;
let targetFramebuffer: WebGLFramebuffer | null | undefined;

function reportStage(stage: string, milliseconds: number): void {
  currentStages.push({ stage, milliseconds });
}
(window as G1B95Window).__logosG1B95Probe = reportStage;

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function samplePixel(x: number, y: number): number[] {
  const canvas = document.querySelector<HTMLCanvasElement>("canvas");
  const gl = canvas?.getContext("webgl2");
  assert(canvas && gl && targetFramebuffer !== undefined, "production Canvas WebGL2 framebuffer is unavailable");
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

async function instrumentedRendererLoader(jsUrl: string, wasmUrl: string) {
  const mod = await loadRenderWasm(jsUrl, wasmUrl);
  if (!mod) return null;
  rendererInstanceCount += 1;
  rendererMemoryBytes = mod.HEAPU8.buffer.byteLength;
  const initialize = mod._init.bind(mod);
  mod._init = (width, height) => {
    const canvas = document.querySelector<HTMLCanvasElement>("canvas");
    const gl = canvas?.getContext("webgl2");
    assert(gl, "production Canvas did not create WebGL2");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    initialize(width, height);
  };
  return mod;
}

function ActionButtons(): React.ReactElement {
  return <div data-testid="g1b95-controls" style={{ position: "fixed", zIndex: 1000, top: 4, left: 4, display: "flex", flexWrap: "wrap", gap: 3 }}>
    {kinds.map((kind) => <button key={kind} data-g1b95-operation={kind} onClick={(event) => {
      if (!fixture || preparedOperation !== kind) throw new Error(`No prepared ${kind} workload`);
      const eventDispatchDelayMs = Math.max(0, performance.now() - event.timeStamp);
      const actionStarted = performance.now();
      currentStages = [];
      const operation = makeG1B95Operation(fixture, kind, preparedSerial);
      const result = useDocumentStore.getState().applyHierarchyOperation(operation);
      const storeActionMs = performance.now() - actionStarted;
      let uiSelectionUpdateMs = 0;
      if (result.ok && (kind === "create" || kind === "insert-child")) {
        const shapeId = kind === "create" ? (operation.kind === "create-root" ? operation.shape.id : "")
          : (operation.kind === "insert-child" ? operation.shape.id : "");
        if (shapeId) {
          const selectionStarted = performance.now();
          useSelectionStore.getState().select(shapeId);
          uiSelectionUpdateMs = performance.now() - selectionStarted;
        }
      }
      lastResult = {
        ok: result.ok,
        ...(!result.ok ? { diagnostic: result.diagnostic.code } : {}),
        operation: kind, storeActionMs, eventDispatchDelayMs, eventTimeStamp: event.timeStamp, uiSelectionUpdateMs,
        stages: [...currentStages], candidate, selectionIds: [...useSelectionStore.getState().selectedIds],
      };
    }}>{kind}</button>)}
  </div>;
}

function App(): React.ReactElement {
  return <div style={{ width: "100vw", height: "100vh", display: "flex" }}>
    <Toolbar />
    <LayersPanel />
    <div style={{ display: "flex", flex: 1, minWidth: 0 }}>
      <div style={{ display: "flex", flex: 1, minWidth: 0 }}><Canvas rendererLoader={instrumentedRendererLoader} /></div>
      <Inspector />
    </div>
    <ActionButtons />
  </div>;
}

window.addEventListener("error", (event) => errors.push(event.message));
const root = createRoot(document.getElementById("root")!);
(window as G1B95Window).__g1b95 = {
  prepare: async (size, operation, serial) => {
    fixture = buildG1B95Fixture(size, serial);
    preparedOperation = operation;
    preparedSerial = serial;
    lastResult = null;
    currentStages = [];
    const page = fixture.page;
    const hydrated = useDocumentStore.getState().hydrateDocument({
      pages: { [page.id]: page }, pageOrder: fixture.pageOrder, currentPageId: page.id, shapes: fixture.shapes,
    });
    if (!hydrated.ok) throw new Error(`fixture hydration failed: ${hydrated.diagnostic.code}`);
    useSelectionStore.getState().select(operation === "subtree-delete" ? fixture.redLeaf : fixture.redLeaf);
    useUiStore.setState({ zoom: 1, panX: 0, panY: 0, activeTool: "select" });
    const snapshot = { documentId: "95000001-0000-4000-8000-000000000001", pageOrder: fixture.pageOrder,
      pages: { [page.id]: page }, shapes: fixture.shapes };
    const op = makeG1B95Operation(fixture, operation, serial);
    if (op.kind.startsWith("compat-")) throw new Error("unexpected compatibility operation");
    const supportedOperation = op as Exclude<HierarchyOperation, { readonly kind: `compat-${string}` }>;
    const conformance = evaluateHierarchyOperationConformance(snapshot, supportedOperation, {
      evaluateDocumentOperationJson: (documentJson, operationJson) => evaluate_document_operation_json(documentJson, operationJson),
    });
    if (!conformance.ok) throw new Error(`Rust candidate rejected ${operation}: ${conformance.phase}/${conformance.code} ${JSON.stringify(conformance.diagnostics ?? conformance.issues ?? [])}`);
    if (!conformance.semanticEquivalent) throw new Error(`React/Rust candidate mismatch for ${operation}`);
    candidate = { ok: true, semanticEquivalent: conformance.semanticEquivalent, timingsMs: conformance.timingsMs };
  },
  fixture: () => {
    assert(fixture, "fixture is not prepared");
    const { shapes: _shapes, ...metadata } = fixture;
    return metadata;
  },
  state: () => {
    const state = useDocumentStore.getState();
    return { page: state.pages[state.currentPageId], shapes: state.shapes, currentPageId: state.currentPageId };
  },
  lastResult: () => lastResult,
  samplePixel,
  errors: () => [...errors],
};

void (async () => {
  try {
    await initTypes();
    root.render(<App />);
    const start = performance.now();
    while (document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") !== "READY") {
      if (performance.now() - start > 30000) throw new Error("production Canvas did not reach READY");
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    status.dataset.status = "ready";
    status.textContent = JSON.stringify({ ready: true, userAgent: navigator.userAgent,
      rendererInstances: rendererInstanceCount, candidateWasmReservedBytes: rendererMemoryBytes });
  } catch (error) {
    status.dataset.status = "failed";
    status.textContent = error instanceof Error ? error.stack ?? error.message : String(error);
  }
})();
