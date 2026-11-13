import React from "react";
import { createRoot } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { Inspector } from "../components/inspector/Inspector";
import { LayersPanel } from "../components/layers/LayersPanel";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import { createRect } from "../types/shapes";

const report = document.getElementById("a7-status") as HTMLPreElement;
let stage = "setup";
report.dataset.status = "running";
report.dataset.stage = stage;
report.textContent = `running: ${stage}`;

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

function nextPaint(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, 50));
}

function documentSnapshot(): string {
  const { pages, pageOrder, currentPageId, shapes } = useDocumentStore.getState();
  return JSON.stringify({ pages, pageOrder, currentPageId, shapes });
}

function captureCanvas(canvas: HTMLCanvasElement): Uint8ClampedArray {
  const context = canvas.getContext("2d");
  assert(context, "Canvas 2D context missing");
  return context.getImageData(0, 0, canvas.width, canvas.height).data.slice();
}

function assertPixelsEqual(before: Uint8ClampedArray, after: Uint8ClampedArray, label: string): void {
  assert(before.length === after.length, `${label}: framebuffer size changed`);
  for (let offset = 0; offset < before.length; offset += 1) {
    if (before[offset] !== after[offset]) throw new Error(`${label}: pixel buffer differs at channel ${offset}`);
  }
}

function assertPixel(canvas: HTMLCanvasElement, x: number, y: number, expected: number[], label: string): void {
  const context = canvas.getContext("2d");
  assert(context, "Canvas 2D context missing");
  const actual = Array.from(context.getImageData(x, y, 1, 1).data);
  assert(actual.every((channel, index) => channel === expected[index]), `${label}: expected ${expected}, got ${actual}`);
}

function assertContextRestored(canvas: HTMLCanvasElement): void {
  const transform = canvas.getContext("2d")?.getTransform();
  assert(transform, "Canvas transform unavailable");
  assert(
    transform.a === 1 && transform.b === 0 && transform.c === 0 && transform.d === 1 && transform.e === 0 && transform.f === 0,
    `Canvas transform leaked after failed render: ${JSON.stringify(transform)}`,
  );
}

function CompatibilitySmokeWorkspace({ onNavigate }: { onNavigate: () => void }): React.ReactElement {
  const layersPanelOpen = useUiStore((state) => state.layersPanelOpen);
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: `56px ${layersPanelOpen ? "220px " : ""}minmax(0, 1fr) 220px`,
      width: 800,
      height: 500,
    }}>
      <nav data-testid="test-page-navigation"><button onClick={onNavigate}>Pages</button></nav>
      {layersPanelOpen && <div data-testid="test-layers-panel"><LayersPanel /></div>}
      <main style={{ display: "flex", minWidth: 0, height: 500 }}><Canvas /></main>
      <aside data-testid="test-inspector" style={{ height: 500 }}><Inspector /></aside>
    </div>
  );
}

async function run(): Promise<void> {
  const { setZoom, setPan } = useUiStore.getState();
  useUiStore.setState({ zoom: 1, panX: 0, panY: 0, layersPanelOpen: false });
  const supportedId = useDocumentStore.getState().addRect(
    { x: 20, y: 20, w: 52, h: 42 },
    "#11aaee",
  );

  let navigationClicks = 0;
  createRoot(document.getElementById("app")!).render(
    <CompatibilitySmokeWorkspace onNavigate={() => navigationClicks++} />,
  );

  setStage("supported-scene-renders");
  const canvas = await waitFor(() => {
    const element = document.querySelector("canvas");
    const state = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererStatus;
    return element && state === "READY" ? element : null;
  }, "supported scene READY");
  assert(document.querySelector("[data-renderer-backend='canvas2d']"), "test did not use Canvas 2D fallback");
  assertPixel(canvas, 30, 30, [17, 170, 238, 255], "supported rectangle");

  setStage("transform-and-frame-capture");
  setZoom(1.25);
  setPan(8, 5);
  await nextPaint();
  assertContextRestored(canvas);
  const priorPixels = captureCanvas(canvas);

  setStage("unsupported-scene-diagnostic");
  const unsupportedId = crypto.randomUUID();
  const unsupportedShape = {
    ...createRect(unsupportedId, "Legacy component", { x: 100, y: 50, w: 32, h: 22 }, "#f38ba8"),
    type: "component" as const,
  };
  useDocumentStore.getState().addShape(unsupportedShape);
  const unchangedDocument = documentSnapshot();
  const overlay = await waitFor(() => {
    const element = document.querySelector<HTMLElement>("[data-testid='renderer-compatibility-overlay']");
    return element?.dataset.state === "UNSUPPORTED_SHAPE" ? element : null;
  }, "unsupported-shape diagnostic");

  assert(overlay.dataset.frameDisposition === "previous", "previous frame was not identified as stale");
  assert(overlay.innerText.includes("component"), "diagnostic omitted unsupported type");
  assert(overlay.innerText.includes(unsupportedId), "diagnostic omitted shape ID");
  assert(overlay.innerText.includes("Canvas 2D fallback"), "diagnostic omitted backend");
  assert(overlay.innerText.includes("LOGOS_RENDER_UNSUPPORTED_SHAPE"), "diagnostic omitted stable code");
  assert(overlay.innerText.includes("not been deleted or changed"), "diagnostic did not reassure the user that document data is intact");
  assert(overlay.innerText.includes("last successful frame"), "stale-frame warning is not user-visible");
  assert(getComputedStyle(overlay).pointerEvents === "auto", "stale canvas is still interactive");
  const overlayBounds = overlay.getBoundingClientRect();
  const canvasBounds = canvas.getBoundingClientRect();
  assert(getComputedStyle(overlay).visibility === "visible", "compatibility diagnostic is not visible");
  assert(overlayBounds.width >= canvasBounds.width, "warning veil does not cover the stale canvas width");
  assert(overlayBounds.height >= canvasBounds.height, "warning veil does not cover the stale canvas height");
  assert(getComputedStyle(overlay).backgroundColor.endsWith(", 0.62)"), "stale pixels are not visually dimmed");
  assertPixelsEqual(priorPixels, captureCanvas(canvas), "fail-closed unsupported render");
  assertContextRestored(canvas);
  assert(documentSnapshot() === unchangedDocument, "failed render mutated document state");

  setStage("single-persistent-diagnostic-and-editor-access");
  const alertNode = overlay.querySelector<HTMLElement>("[role='alert']");
  assert(alertNode, "compatibility message is not an accessible alert");
  assert(document.querySelectorAll("[data-testid='renderer-compatibility-overlay']").length === 1, "duplicate notices were created");
  setStage("repeated-failure-does-not-loop-notices");
  setPan(9, 6);
  await nextPaint();
  assert(document.querySelectorAll("[data-testid='renderer-compatibility-overlay']").length === 1, "repeated failure duplicated the compatibility notice");
  assert(documentSnapshot() === unchangedDocument, "repeated failed render mutated document state");
  assertPixelsEqual(priorPixels, captureCanvas(canvas), "repeated fail-closed render");

  setStage("single-persistent-diagnostic-and-editor-access");
  document.querySelector<HTMLButtonElement>("[data-testid='test-page-navigation'] button")!.click();
  assert(navigationClicks === 1, "canvas notice blocked document navigation");
  document.querySelector<HTMLButtonElement>("[data-testid='renderer-compatibility-overlay'] button")!.click();
  assert(useSelectionStore.getState().selectedIds.includes(unsupportedId), "recovery action did not select the incompatible shape");
  assert(useUiStore.getState().layersPanelOpen, "recovery action did not open Layers");
  const reflowedOverlay = await waitFor(() => {
    const element = document.querySelector<HTMLElement>("[data-testid='renderer-compatibility-overlay']");
    return element?.dataset.frameDisposition === "none" ? element : null;
  }, "resized canvas to clear stale-frame status");
  assert(reflowedOverlay.innerText.includes("No current frame is available"), "cleared pixels were not identified to the user");
  assert(document.querySelector("[data-testid='test-layers-panel']")?.textContent?.includes("Legacy component"), "Layers did not reveal the selected shape");
  const inspector = document.querySelector<HTMLElement>("[data-testid='test-inspector']");
  assert(inspector, "Inspector panel is unavailable");
  assert(inspector.textContent?.includes("Legacy component"), "Inspector did not update for the selected shape");
  assert(inspector.querySelector("input"), "Inspector controls are unavailable during the diagnostic");

  setStage("compatible-update-recovers");
  useDocumentStore.getState().updateShape(unsupportedId, { type: "rect" });
  await waitFor(() => {
    const state = document.querySelector<HTMLElement>("[data-renderer-status]")?.dataset.rendererStatus;
    return state === "READY" && !document.querySelector("[data-testid='renderer-compatibility-overlay']") ? true : null;
  }, "successful recovery clears diagnostic");
  assertPixel(canvas, 145, 75, [243, 139, 168, 255], "corrected shape after recovery");
  assertContextRestored(canvas);

  setStage("complete");
  report.dataset.status = "passed";
  report.textContent = JSON.stringify({ status: "passed", supportedId, unsupportedId, checks: [
    "supported Canvas 2D pixel output",
    "visible unsupported-shape status with backend, type, ID, and code",
    "stale frame dimmed and labelled as previous",
    "document unchanged during failed render",
    "surrounding navigation and inspector controls remain usable",
    "context transform restored after thrown render",
    "single persistent notice across repeated failures",
    "compatible shape correction clears error and renders",
  ] }, null, 2);
}

void run().catch((error: unknown) => {
  report.dataset.status = "failed";
  report.textContent = JSON.stringify({ stage, error: String(error) }, null, 2);
});
