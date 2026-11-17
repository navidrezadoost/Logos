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
  ok: boolean; diagnostic?: string; operation: G1B95OperationKind | "view-control"; storeActionMs: number;
  eventDispatchDelayMs: number; eventTimeStamp: number; uiSelectionUpdateMs: number;
  handlerStartedAt: number; handlerCompletedAt: number; sampleId: string;
  stages: Stage[]; candidate: unknown; selectionIds: string[];
};
type BrowserTiming = { sampleId: string; expectedFrameGeneration: number; interactionStartAt: number;
  handlerStartedAt: number; handlerCompletedAt: number; reactLayoutCommitAt?: number;
  rendererEffectStartedAt?: number; wasmSceneSyncCompletedAt?: number; rendererCompletionRafAt?: number;
  framePublishedAt?: number; actualFrameGeneration?: number; interactionToFrameMs?: number };
type G1B95Window = Window & {
  __g1b95?: {
    prepare: (size: G1B95Size, operation: G1B95OperationKind | "view-control", serial: number, sampleId?: string) => Promise<void>;
    prepareNext: (operation: G1B95OperationKind, serial: number, sampleId: string) => void;
    fixture: () => Omit<G1B95Fixture, "shapes">;
    state: () => { page: Page; shapes: Record<string, Shape>; currentPageId: string };
    selection: () => string[];
    lastResult: () => LastResult | null;
    timing: (sampleId: string) => BrowserTiming | null;
    discardTiming: (sampleId: string) => void;
    serializedState: () => { json: string; serializationMs: number };
    samplePixel: (x: number, y: number) => number[];
    samplePixelMeasured: (x: number, y: number) => { rgba: number[]; finishMs: number; readPixelsMs: number };
    errors: () => string[];
  };
  __logosG1B95Probe?: (stage: string, milliseconds: number) => void;
};

let fixture: G1B95Fixture | null = null;
let preparedOperation: G1B95OperationKind | "view-control" | null = null;
let preparedSerial = 0;
let preparedSampleId = "";
let activeTiming: BrowserTiming | null = null;
const completedTimings = new Map<string, BrowserTiming>();
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

function prepareOperation(operation: G1B95OperationKind | "view-control", serial: number, sampleId: string): void {
  assert(fixture, "fixture is not prepared");
  preparedOperation = operation;
  preparedSerial = serial;
  preparedSampleId = sampleId;
  activeTiming = null;
  completedTimings.delete(sampleId);
  lastResult = null;
  currentStages = [];
  if (operation === "view-control") { candidate = null; return; }
  const state = useDocumentStore.getState();
  const op = makeG1B95Operation(fixture, operation, serial);
  if (op.kind.startsWith("compat-")) throw new Error("unexpected compatibility operation");
  const supportedOperation = op as Exclude<HierarchyOperation, { readonly kind: `compat-${string}` }>;
  const conformance = evaluateHierarchyOperationConformance({
    documentId: state.documentId, pageOrder: state.pageOrder, pages: state.pages, shapes: state.shapes,
  }, supportedOperation, {
    evaluateDocumentOperationJson: (documentJson, operationJson) => evaluate_document_operation_json(documentJson, operationJson),
  });
  if (!conformance.ok) throw new Error(`Rust candidate rejected ${operation}: ${conformance.phase}/${conformance.code} ${JSON.stringify(conformance.diagnostics ?? conformance.issues ?? [])}`);
  if (!conformance.semanticEquivalent) throw new Error(`React/Rust candidate mismatch for ${operation}`);
  candidate = { ok: true, semanticEquivalent: conformance.semanticEquivalent, timingsMs: conformance.timingsMs };
}

function samplePixel(x: number, y: number): number[] {
  return samplePixelMeasured(x, y).rgba;
}

function samplePixelMeasured(x: number, y: number): { rgba: number[]; finishMs: number; readPixelsMs: number } {
  const canvas = document.querySelector<HTMLCanvasElement>("canvas");
  const gl = canvas?.getContext("webgl2");
  assert(canvas && gl && targetFramebuffer !== undefined, "production Canvas WebGL2 framebuffer is unavailable");
  const pixel = new Uint8Array(4);
  const finishStartedAt = performance.now();
  gl.finish();
  const finishMs = performance.now() - finishStartedAt;
  const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
  let readPixelsMs = 0;
  try {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, targetFramebuffer);
    gl.readBuffer(targetFramebuffer === null ? gl.BACK : gl.COLOR_ATTACHMENT0);
    const readStartedAt = performance.now();
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    readPixelsMs = performance.now() - readStartedAt;
  } finally {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous);
  }
  return { rgba: Array.from(pixel), finishMs, readPixelsMs };
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
      const handlerStartedAt = performance.now();
      const eventDispatchDelayMs = Math.max(0, performance.now() - event.timeStamp);
      const expectedFrameGeneration = Number(document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-frame-count") ?? 0) + 1;
      activeTiming = { sampleId: preparedSampleId, expectedFrameGeneration, interactionStartAt: event.timeStamp, handlerStartedAt, handlerCompletedAt: handlerStartedAt };
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
      const handlerCompletedAt = performance.now();
      activeTiming.handlerCompletedAt = handlerCompletedAt;
      lastResult = {
        ok: result.ok,
        ...(!result.ok ? { diagnostic: result.diagnostic.code } : {}),
        operation: kind, storeActionMs, eventDispatchDelayMs, eventTimeStamp: event.timeStamp, uiSelectionUpdateMs,
        handlerStartedAt, handlerCompletedAt, sampleId: preparedSampleId,
        stages: [...currentStages], candidate, selectionIds: [...useSelectionStore.getState().selectedIds],
      };
    }}>{kind}</button>)}
    <button data-g1b95-operation="view-control" onClick={(event) => {
      if (!fixture || preparedOperation !== "view-control") throw new Error("No prepared view-control workload");
      const handlerStartedAt = performance.now();
      const expectedFrameGeneration = Number(document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-frame-count") ?? 0) + 1;
      activeTiming = { sampleId: preparedSampleId, expectedFrameGeneration, interactionStartAt: event.timeStamp, handlerStartedAt, handlerCompletedAt: handlerStartedAt };
      const ui = useUiStore.getState();
      useUiStore.setState({ panX: ui.panX + 1 });
      const handlerCompletedAt = performance.now();
      activeTiming.handlerCompletedAt = handlerCompletedAt;
      lastResult = { ok: true, operation: "view-control", storeActionMs: handlerCompletedAt - handlerStartedAt,
        eventDispatchDelayMs: Math.max(0, handlerStartedAt - event.timeStamp), eventTimeStamp: event.timeStamp,
        uiSelectionUpdateMs: 0, handlerStartedAt, handlerCompletedAt, sampleId: preparedSampleId,
        stages: [], candidate: null, selectionIds: [...useSelectionStore.getState().selectedIds] };
    }}>view-control</button>
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
  prepare: async (size, operation, serial, sampleId = `${size}-${operation}-${serial}`) => {
    fixture = buildG1B95Fixture(size, serial);
    const page = fixture.page;
    const hydrated = useDocumentStore.getState().hydrateDocument({
      pages: { [page.id]: page }, pageOrder: fixture.pageOrder, currentPageId: page.id, shapes: fixture.shapes,
    });
    if (!hydrated.ok) throw new Error(`fixture hydration failed: ${hydrated.diagnostic.code}`);
    useSelectionStore.getState().select(fixture.redLeaf);
    useUiStore.setState({ zoom: 1, panX: 0, panY: 0, activeTool: "select" });
    prepareOperation(operation, serial, sampleId);
  },
  prepareNext: (operation, serial, sampleId) => prepareOperation(operation, serial, sampleId),
  fixture: () => {
    assert(fixture, "fixture is not prepared");
    const { shapes: _shapes, ...metadata } = fixture;
    return metadata;
  },
  state: () => {
    const state = useDocumentStore.getState();
    return { page: state.pages[state.currentPageId], shapes: state.shapes, currentPageId: state.currentPageId };
  },
  selection: () => [...useSelectionStore.getState().selectedIds],
  lastResult: () => lastResult,
  timing: (sampleId) => {
    const record = completedTimings.get(sampleId);
    return record ? structuredClone(record) : null;
  },
  discardTiming: (sampleId) => { completedTimings.delete(sampleId); },
  serializedState: () => {
    const started = performance.now();
    const state = useDocumentStore.getState();
    const json = JSON.stringify({ page: state.pages[state.currentPageId], shapes: state.shapes, currentPageId: state.currentPageId });
    return { json, serializationMs: performance.now() - started };
  },
  samplePixel,
  samplePixelMeasured,
  errors: () => [...errors],
  activeSample: () => activeTiming ? { sampleId: activeTiming.sampleId, expectedFrameGeneration: activeTiming.expectedFrameGeneration } : null,
  recordCanvasStage: (sampleId, stage, timestamp, frameGeneration) => {
    if (!activeTiming || activeTiming.sampleId !== sampleId || completedTimings.has(sampleId)) return;
    if (stage === "react_layout_commit_observed") activeTiming.reactLayoutCommitAt = timestamp;
    if (stage === "renderer_effect_started") activeTiming.rendererEffectStartedAt = timestamp;
    if (stage === "wasm_scene_sync_completed" && frameGeneration === activeTiming.expectedFrameGeneration) activeTiming.wasmSceneSyncCompletedAt = timestamp;
    if (stage === "renderer_completion_raf" && frameGeneration === activeTiming.expectedFrameGeneration) activeTiming.rendererCompletionRafAt = timestamp;
  },
  publishFrame: (sampleId, timestamp, frameGeneration) => {
    if (!activeTiming || activeTiming.sampleId !== sampleId || completedTimings.has(sampleId) || frameGeneration !== activeTiming.expectedFrameGeneration) return;
    const completed = { ...activeTiming, framePublishedAt: timestamp, actualFrameGeneration: frameGeneration,
      interactionToFrameMs: timestamp - activeTiming.interactionStartAt };
    completedTimings.set(sampleId, Object.freeze(completed));
    activeTiming = null;
  },
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
