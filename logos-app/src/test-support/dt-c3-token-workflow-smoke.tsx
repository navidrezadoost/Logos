import React from "react";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { loadRenderWasm, type RenderWasmModule } from "../render-wasm/module";
import { Inspector } from "../components/inspector/Inspector";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import { useTokenStore } from "../stores/tokenStore";
import { useTokenWorkflowPreviewStore } from "../stores/tokenWorkflowPreviewStore";
import { loadRustTokenEngine, type RustTokenEngineLoader } from "../tokens/rustTokenEngine";
import { createTokenWorkflowProposal } from "../tokens/tokenWorkflow";
import type { ResolveRequest } from "../tokens/contract";
import type { GradientFill, Shape } from "../types/shapes";

const report = document.getElementById("dt-c3-status") as HTMLPreElement;
let stage = "setup";
let activeRendererBackend = "unknown";
report.dataset.status = "running";
report.dataset.stage = stage;

function setStage(next: string): void {
  stage = next;
  report.dataset.stage = next;
  report.textContent = `running: ${next}`;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function waitFor<T>(read: () => T | null | undefined, label: string, timeoutMs = 15000): Promise<T> {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const poll = () => {
      const value = read();
      if (value !== null && value !== undefined) return resolve(value);
      if (performance.now() - start > timeoutMs) return reject(new Error(`timeout waiting for ${label}`));
      window.setTimeout(poll, 20);
    };
    poll();
  });
}

function paint(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, 50));
}

function documentSnapshot(): string {
  const { pages, pageOrder, currentPageId, shapes } = useDocumentStore.getState();
  return JSON.stringify({ pages, pageOrder, currentPageId, shapes });
}

function pixel(canvas: HTMLCanvasElement, x: number, y: number): number[] {
  const backend = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererBackend;
  if (backend === "canvas2d") {
    const context = canvas.getContext("2d");
    assert(context, "Canvas 2D context missing");
    return Array.from(context.getImageData(x, y, 1, 1).data);
  }
  const gl = canvas.getContext("webgl2");
  assert(gl && targetFramebuffer !== undefined, "production Canvas WebGL framebuffer missing");
  const previousFramebuffer = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
  const previousReadBuffer = gl.getParameter(gl.READ_BUFFER) as number;
  const result = new Uint8Array(4);
  try {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, targetFramebuffer);
    gl.readBuffer(targetFramebuffer === null ? gl.BACK : gl.COLOR_ATTACHMENT0);
    gl.readPixels(x, canvas.height - 1 - y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, result);
  } finally {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previousFramebuffer);
    gl.readBuffer(previousReadBuffer);
  }
  return Array.from(result);
}

function assertPixel(canvas: HTMLCanvasElement, expected: number[], label: string, x = 30, y = 30): void {
  const actual = pixel(canvas, x, y);
  assert(actual.every((channel, index) => Math.abs(channel - expected[index]) <= 3), `${label}: expected ${expected}, got ${actual}`);
}

let targetFramebuffer: WebGLFramebuffer | null | undefined;

async function instrumentedRendererLoader(jsUrl: string, wasmUrl: string): Promise<RenderWasmModule | null> {
  const module = await loadRenderWasm(jsUrl, wasmUrl);
  if (!module) return null;
  const originalInit = module._init.bind(module);
  module._init = (width, height) => {
    const canvas = document.querySelector<HTMLCanvasElement>("canvas");
    const gl = canvas?.getContext("webgl2");
    assert(gl, "production Canvas did not create a WebGL2 context");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    originalInit(width, height);
  };
  return module;
}

function Workspace({ engineLoader }: { engineLoader?: RustTokenEngineLoader }): React.ReactElement {
  return <div style={{ display: "flex", width: 800, height: 500 }}>
    <main data-testid="workflow-canvas-column" style={{ flex: 1, minWidth: 0 }}><Canvas rendererLoader={instrumentedRendererLoader} /></main>
    <aside data-testid="workflow-inspector-column" style={{ width: 310, height: 500, flex: "none" }}><Inspector tokenEngineLoader={engineLoader} /></aside>
  </div>;
}

async function waitForCanvas(): Promise<HTMLCanvasElement> {
  const canvas = await waitFor(() => {
    const element = document.querySelector("canvas");
    const state = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererStatus;
    return element && state === "READY" ? element : null;
  }, "ready canvas");
  activeRendererBackend = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererBackend ?? "unknown";
  return canvas;
}

function click(selector: string): void {
  const button = document.querySelector<HTMLButtonElement>(selector);
  assert(button, `missing interactive control ${selector}`);
  button.click();
}

async function bindPreview(): Promise<HTMLElement> {
  click("[data-testid=create-bind-token]");
  return waitFor(() => {
    const status = document.querySelector<HTMLElement>("[data-testid=token-workflow-status]");
    return status?.textContent?.startsWith("Bound in preview") ? status : null;
  }, "Rust-created token binding preview");
}

async function run(): Promise<void> {
  useUiStore.setState({ zoom: 1, panX: 0, panY: 0, layersPanelOpen: false, activeTool: "select" });
  const shapeId = useDocumentStore.getState().addRect({ x: 20, y: 20, w: 52, h: 42 }, "#11aaee");
  const otherShapeId = useDocumentStore.getState().addRect({ x: 120, y: 20, w: 52, h: 42 }, "#eeaa11");
  useSelectionStore.getState().select(shapeId);
  let root: Root = createRoot(document.getElementById("app")!);
  root.render(<Workspace />);

  const canvas = await waitForCanvas();
  const pixelEvidence: Record<string, number[]> = {};
  assert(document.querySelector("[data-renderer-backend='canvas2d'], [data-renderer-backend='wasm']"), "smoke must exercise the production Canvas backend");
  assertPixel(canvas, [17, 170, 238, 255], "initial direct fill");
  pixelEvidence.initialDirect = pixel(canvas, 30, 30);

  setStage("ordinary-direct-fill-edit");
  assert(document.querySelector("input[type=color]"), "ordinary fill color control missing");
  const shape = useDocumentStore.getState().shapes[shapeId];
  useDocumentStore.getState().updateShape(shapeId, {
    fills: shape.fills.map((fill) => fill.type === "solid" ? { ...fill, color: "#226644" } : fill),
  });
  await waitFor(() => {
    const fill = useDocumentStore.getState().shapes[shapeId].fills[0];
    return fill?.type === "solid" && fill.color === "#226644" ? true : null;
  }, "direct fill edit");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "ordinary direct fill edit");
  pixelEvidence.directEdit = pixel(canvas, 30, 30);
  assert(!("tokenBinding" in useDocumentStore.getState().shapes[shapeId]), "direct fill edit unexpectedly added token metadata");

  const beforeWorkflow = documentSnapshot();
  const documentMutations: string[] = [];
  const unsubscribeDocumentStore = useDocumentStore.subscribe((state) => documentMutations.push(state.currentPageId));
  const tokenRegistryBefore = localStorage.getItem("logos-token-store");
  const importedTokenStateBefore = JSON.stringify({ sets: useTokenStore.getState().sets, themes: useTokenStore.getState().themes });

  setStage("rust-adapter-invalid-reference-and-stale-prestate");
  const current = useDocumentStore.getState();
  const proposal = createTokenWorkflowProposal(shapeId, "#226644", "Primary Color", {
    documentId: current.documentId,
    pageId: current.currentPageId,
  });
  assert(proposal.directProperty.kind === "literal", "proposal did not retain its direct color");
  const directColor = proposal.directProperty.value;
  const engine = await loadRustTokenEngine("/js/logos_tokens_wasm.wasm");
  const missingTarget = { namespace: "document", tokenId: crypto.randomUUID() };
  const missingReferenceRequest: ResolveRequest = {
    contractVersion: "0.1.0", snapshotRevision: proposal.snapshot.revision, propertyType: "color",
    property: { kind: "binding", target: missingTarget, modeStrategy: { kind: "inherit" } },
    context: { explicit: {}, ancestorsNearestFirst: [], page: {}, document: {} },
  };
  const missingReference = engine.resolve(proposal.snapshot, missingReferenceRequest);
  assert(missingReference.status === "ERROR" && missingReference.diagnostics.some((item) => item.code === "MISSING_TOKEN"), "invalid reference did not return a controlled Rust diagnostic");
  const fallback = engine.resolve(proposal.snapshot, {
    ...missingReferenceRequest,
    property: { kind: "binding", target: missingTarget, modeStrategy: { kind: "inherit" }, fallback: directColor },
  });
  assert(fallback.status === "RESOLVED" && fallback.source === "FALLBACK" && fallback.diagnostics.some((item) => item.code === "MISSING_TOKEN" && item.severity === "warning"), "fallback resolution did not preserve its warning and distinct source");
  const stale = engine.validatePlan({ ...proposal.snapshot, revision: `${proposal.snapshot.revision}-stale` }, proposal.intent, proposal.context, proposal.plan);
  assert(stale.status === "REJECTED" && stale.diagnostics.some((item) => item.code === "STALE_REVISION"), "stale revision was not rejected by Rust");
  assert(documentSnapshot() === beforeWorkflow, "invalid Rust proposals mutated the document");

  setStage("create-token-bind-with-rust");
  const boundStatus = await bindPreview();
  assert(boundStatus.textContent?.includes("TOKEN"), "preview did not report token resolution source");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "token binding initial resolution");
  pixelEvidence.tokenBound = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "create and bind preview mutated the document store");
  assert(localStorage.getItem("logos-token-store") === tokenRegistryBefore, "preview wrote to the persisted local-storage token registry");
  assert(JSON.stringify({ sets: useTokenStore.getState().sets, themes: useTokenStore.getState().themes }) === importedTokenStateBefore, "preview changed the imported token store");
  assert(documentMutations.length === 0, "preview triggered document-store writes or shadow-evaluator input");

  const wireResult = engine.validatePlan(proposal.snapshot, proposal.intent, proposal.context, proposal.plan);
  const wireJson = JSON.stringify(wireResult);
  assert(wireResult.status === "READY" && wireResult.impact.propertyPreviews[0]?.resolution.source === "TOKEN", "Rust validation did not return a token-resolved property preview");
  assert(wireJson.includes("propertyPreviews") && wireJson.includes("planDigest") && !wireJson.includes("property_previews"), "Rust/TypeScript serialized contract fields do not match camelCase DTOs");
  assert(wireResult.impact.createdTokens === 1 && wireResult.impact.boundProperties === 1, "Rust impact summary omitted create/bind operations");

  setStage("preview-token-value-change");
  click("[data-testid=preview-token-value]");
  await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.includes("Token value preview · #EF476F") ? true : null, "updated Rust token preview");
  await paint();
  assertPixel(canvas, [239, 71, 111, 255], "updated token value on canvas");
  pixelEvidence.tokenValuePreview = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "token value preview mutated the document store");

  setStage("canvas-resize-keeps-preview-and-clears-stale-pixels");
  const canvasCssWidthBeforeResize = canvas.getBoundingClientRect().width;
  const workspaceElement = document.querySelector<HTMLElement>("#app > div")!;
  workspaceElement.style.width = "740px";
  await waitFor(() => canvas.getBoundingClientRect().width !== canvasCssWidthBeforeResize ? true : null, "Canvas CSS resize");
  await paint();
  assertPixel(canvas, [239, 71, 111, 255], "token preview survives Canvas resize");
  assert(documentSnapshot() === beforeWorkflow, "Canvas resize mutated the document");

  setStage("revert-preview");
  click("[data-testid=revert-token-preview]");
  await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.includes("Preview reverted") ? true : null, "preview revert");
  await paint();
  assertPixel(canvas, [34, 102, 68, 255], "reverted original fill");
  pixelEvidence.reverted = pixel(canvas, 30, 30);
  assert(documentSnapshot() === beforeWorkflow, "revert did not leave the document exactly unchanged");
  assert(localStorage.getItem("logos-token-store") === tokenRegistryBefore, "revert changed the persistent token registry");
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "revert left a transient canvas override behind");

  setStage("selection-change-disposes-preview");
  await bindPreview();
  useSelectionStore.getState().select(otherShapeId);
  await waitFor(() => document.querySelector("[data-testid=inspector]")?.textContent?.includes("Rectangle 2") ? true : null, "new Inspector selection");
  await paint();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "selection change left an owned preview behind");
  assertPixel(canvas, [34, 102, 68, 255], "selected shape preview disposal");
  assert(documentSnapshot() === beforeWorkflow, "selection change mutated document data");

  setStage("delayed-wasm-result-is-discarded-after-selection-change");
  let releaseDelayedEngine: (() => void) | null = null;
  const delayedEngineLoader: RustTokenEngineLoader = () => new Promise((resolve) => {
    releaseDelayedEngine = () => resolve(engine);
  });
  flushSync(() => root.render(<Workspace engineLoader={delayedEngineLoader} />));
  await waitFor(() => document.querySelector("[data-testid=create-bind-token]"), "new token proposal control");
  click("[data-testid=create-bind-token]");
  await waitFor(() => releaseDelayedEngine ? true : null, "delayed Rust engine request");
  useSelectionStore.getState().select(shapeId);
  await paint();
  releaseDelayedEngine!();
  await paint();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "late Rust response restored a stale selection preview");
  assert(documentSnapshot() === beforeWorkflow, "late Rust response mutated the document");

  setStage("page-switch-disposes-preview");
  flushSync(() => root.render(<Workspace />));
  await waitFor(() => document.querySelector("[data-testid=create-bind-token]"), "selected shape token control");
  await bindPreview();
  const originalPageId = useDocumentStore.getState().currentPageId;
  const secondPageId = useDocumentStore.getState().addPage("Token preview lifecycle");
  useDocumentStore.getState().setCurrentPage(secondPageId);
  await waitFor(() => document.querySelector("[data-testid=inspector]")?.textContent?.includes("No selection") ? true : null, "selection cleared on page switch");
  await paint();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "page switch left the old page preview active");
  useDocumentStore.getState().setCurrentPage(originalPageId);
  useSelectionStore.getState().select(shapeId);

  setStage("unsupported-fill-and-shape-types");
  await waitFor(() => document.querySelector("[data-testid=token-workflow]"), "token control after returning to original page");
  const originalShape = useDocumentStore.getState().shapes[shapeId];
  const gradientFill: GradientFill = {
    type: "gradient",
    gradient: { type: "linear", startX: 20, startY: 20, endX: 72, endY: 20, width: 0,
      stops: [{ position: 0, opacity: 1, color: "#000000" }, { position: 1, opacity: 1, color: "#FFFFFF" }] },
    opacity: 1,
  };
  useDocumentStore.getState().updateShape(shapeId, { fills: [gradientFill] });
  await waitFor(() => !document.querySelector("[data-testid=token-workflow]") ? true : null, "token control absent for gradient fill");
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "unsupported gradient fill left a preview active");
  useDocumentStore.getState().updateShape(shapeId, { fills: originalShape.fills });
  await waitFor(() => document.querySelector("[data-testid=token-workflow]"), "token control after fill restoration");
  await bindPreview();
  useDocumentStore.getState().updateShape(shapeId, { type: "unsupported-imported-shape" as Shape["type"] });
  await waitFor(() => {
    const status = document.querySelector<HTMLElement>("[data-renderer-status]");
    const supportedDiagnostic = status?.dataset.rendererDiagnostic === "LOGOS_RENDER_UNSUPPORTED_SHAPE" ||
      status?.dataset.rendererDiagnostic === "LOGOS_SCENE_UNSUPPORTED_SHAPE";
    return supportedDiagnostic && (status?.dataset.rendererStatus === "UNSUPPORTED_SHAPE" || status?.dataset.rendererStatus === "SCENE_REJECTED")
      ? true
      : null;
  }, "unsupported type renderer diagnostic");
  assert(!document.querySelector("[data-testid=token-workflow]"), "unsupported renderer type offered token preview");
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "renderer failure left a token preview active");
  useDocumentStore.getState().updateShape(shapeId, { type: "rect" });
  await waitFor(() => document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererStatus === "READY" ? true : null, "renderer recovers after supported type restoration");
  assertPixel(canvas, [34, 102, 68, 255], "renderer recovery shows the unchanged direct fill");

  setStage("Rust-engine-failure-clears-preview");
  const failingLoader: RustTokenEngineLoader = async () => { throw new Error("injected token WASM failure"); };
  flushSync(() => root.render(<Workspace engineLoader={failingLoader} />));
  useSelectionStore.getState().select(otherShapeId);
  await waitFor(() => document.querySelector("[data-testid=create-bind-token]"), "failure-test token control");
  click("[data-testid=create-bind-token]");
  await waitFor(() => document.querySelector<HTMLElement>("[data-testid=token-workflow-status]")?.textContent?.includes("injected token WASM failure") ? true : null, "visible Rust engine failure");
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "Rust engine failure left a preview active");
  assert(documentSnapshot() !== beforeWorkflow, "lifecycle fixtures unexpectedly failed to record their documented page addition");
  const afterLifecycle = documentSnapshot();

  setStage("delete-disposes-preview-and-reconciles-selection");
  flushSync(() => root.render(<Workspace />));
  useSelectionStore.getState().select(shapeId);
  await waitFor(() => document.querySelector("[data-testid=create-bind-token]"), "deletion-test token control");
  await bindPreview();
  const deletion = useDocumentStore.getState().removeShape(shapeId);
  assert(deletion.ok, "supported subtree deletion failed during preview lifecycle test");
  await waitFor(() => document.querySelector("[data-testid=inspector]")?.textContent?.includes("No selection") ? true : null, "selection reconciled after deletion");
  await paint();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "deleting the previewed shape left a preview active");
  assert(!useDocumentStore.getState().shapes[shapeId], "deleted shape remained in the document");
  const afterDeletion = documentSnapshot();

  setStage("repeated-mount-unmount-cleanup");
  useSelectionStore.getState().select(otherShapeId);
  flushSync(() => root.render(<Workspace />));
  await waitFor(() => document.querySelector("[data-testid=create-bind-token]"), "token control before active-preview unmount");
  await bindPreview();
  assert(useTokenWorkflowPreviewStore.getState().preview !== null, "unmount test did not establish an active preview");
  root.unmount();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "unmount failed to dispose token preview");
  root = createRoot(document.getElementById("app")!);
  root.render(<Workspace />);
  await waitForCanvas();
  await paint();
  assert(useTokenWorkflowPreviewStore.getState().preview === null, "remount restored an unpersisted preview");
  assert(documentSnapshot() === afterDeletion, "mount/unmount wrote document state");
  assert(localStorage.getItem("logos-token-store") === tokenRegistryBefore, "lifecycle operations changed persisted token registry");
  unsubscribeDocumentStore();

  setStage("complete");
  report.dataset.status = "passed";
  report.textContent = JSON.stringify({ status: "passed", shapeId, otherShapeId, rendererBackend: activeRendererBackend, pixelEvidence, checks: [
    "ordinary direct fill editing without token metadata",
    "typed ResolveRequest and create/bind/value plans invoke the compiled Rust WASM engine",
    "Rust returns typed camelCase impact and resolved fill preview",
    "missing references, fallback diagnostics, and stale revisions fail or resolve explicitly",
    "actual Canvas pixels change and revert exactly through the transient projection",
    "document-store, imported-token registry, local storage, and shadow-operation inputs remain unchanged during preview",
    "Canvas CSS resize preserves preview color; the existing A7 regression verifies backing-store resize and stale-frame clearing",
    "selection changes, page switches, shape deletion, and unmount dispose the active preview",
    "late Rust results after selection changes are discarded",
    "unsupported fill and shape type do not expose or retain a preview",
    "Rust WASM failure is visible and leaves no invalid preview",
    "renderer reports an unsupported-shape failure and recovers after supported content returns",
    "repeated mount/unmount does not restore preview state",
  ] }, null, 2);
  root.unmount();
}

void run().catch((error: unknown) => {
  report.dataset.status = "failed";
  report.dataset.stage = stage;
  report.textContent = JSON.stringify({ status: "failed", stage, error: String(error) }, null, 2);
});
