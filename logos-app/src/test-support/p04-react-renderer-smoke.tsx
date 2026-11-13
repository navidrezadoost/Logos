import React from "react";
import { createRoot } from "react-dom/client";
import sourceFixture from "../../../test/fixtures/p0-document-compat/source-document.json";
import { Canvas } from "../components/canvas/Canvas";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import { loadRenderWasm, SHAPE_TYPE, type RenderWasmModule } from "../render-wasm/module";
import { uuidToU32x4 } from "../types/shapes";
import { projectP04RendererFixture } from "./p04-renderer-fixture";

const status = document.getElementById("p04-status") as HTMLPreElement;
const browserErrors: string[] = [];
const diagnostics: Record<string, unknown> = {};
let stage = "setup";
window.addEventListener("error", (event) => browserErrors.push(event.error?.stack ?? event.message));
window.addEventListener("unhandledrejection", (event) => browserErrors.push(String(event.reason)));
const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  browserErrors.push(args.map((arg) => String(arg)).join(" "));
  originalConsoleError(...args);
};

function fail(error: unknown): void {
  status.dataset.status = "failed";
  status.textContent = JSON.stringify({ stage, error: String(error), diagnostics, browserErrors }, null, 2);
}

function waitFor<T>(read: () => T | null | undefined, label: string, timeout = 15000): Promise<T> {
  return new Promise((resolve, reject) => {
    const start = performance.now();
    const poll = () => {
      const value = read();
      if (value !== null && value !== undefined) return resolve(value);
      if (performance.now() - start > timeout) return reject(new Error(`timeout waiting for ${label}`));
      window.setTimeout(poll, 25);
    };
    poll();
  });
}

function sample(gl: WebGL2RenderingContext, target: WebGLFramebuffer | null, x: number, y: number): number[] {
  gl.finish();
  gl.bindFramebuffer(gl.FRAMEBUFFER, target);
  if (target === null) gl.readBuffer(gl.BACK);
  if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
    throw new Error("renderer target framebuffer is incomplete");
  }
  const pixel = new Uint8Array(4);
  gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
  const errors: number[] = [];
  for (let i = 0; i < 8; i++) {
    const error = gl.getError();
    if (error === gl.NO_ERROR) break;
    errors.push(error);
  }
  if (errors.length) throw new Error(`WebGL errors after pixel read: ${errors.join(",")}`);
  return Array.from(pixel);
}

function expectColor(actual: number[], expected: number[], label: string): void {
  if (actual.some((channel, index) => Math.abs(channel - expected[index]) > 2)) {
    throw new Error(`${label}: expected ${expected}, got ${actual}`);
  }
}

async function run(): Promise<void> {
  stage = "project fixture";
  const projection = projectP04RendererFixture(sourceFixture);
  const shapeId = projection.pages[projection.currentPageId].rootShapeIds[0];
  const shape = projection.shapes[shapeId];
  const unknown = projection.unknownFieldsById[shapeId]["extension-p0"] as { sentinel?: string };
  if (unknown?.sentinel !== "preserve-me") throw new Error("fixture extension sidecar did not preserve sentinel");
  diagnostics.adapterVersion = projection.adapterVersion;
  diagnostics.fileId = projection.documentId;
  diagnostics.shapeId = shapeId;
  diagnostics.unknownExtension = unknown;

  useDocumentStore.setState({
    pages: projection.pages,
    pageOrder: projection.pageOrder,
    currentPageId: projection.currentPageId,
    shapes: projection.shapes,
  });
  useUiStore.setState({ activeTool: "scale", zoom: 1, panX: 0, panY: 0 });
  useSelectionStore.setState({ selectedIds: [] });

  stage = "load Rust/WASM module";
  const loaded = await loadRenderWasm("/js/render-wasm.js", "/js/render-wasm.wasm");
  if (!loaded) throw new Error("React canvas could not load the committed Rust/WASM renderer");
  diagnostics.wasmLoaded = true;

  const renderer = loaded as RenderWasmModule & {
    _set_children_1: (a: number, b: number, c: number, d: number) => void;
    _get_selection_rect?: () => number;
  };
  if (typeof renderer._set_children_1 !== "function") {
    throw new Error("Rust renderer does not export _set_children_1 for the test-only root adapter");
  }
  let lastBasePropsPointer: number | null = null;
  const uploadedSelrects: number[][] = [];
  const rendererBoundsHistory: Array<Record<string, number>> = [];
  const allocBytes = renderer._alloc_bytes.bind(renderer);
  renderer._alloc_bytes = (size) => {
    const pointer = allocBytes(size);
    if (size === 104) lastBasePropsPointer = pointer;
    return pointer;
  };
  const setShapeBaseProps = renderer._set_shape_base_props.bind(renderer);
  renderer._set_shape_base_props = () => {
    if (lastBasePropsPointer !== null) {
      const view = new DataView(renderer.HEAPU8.buffer, lastBasePropsPointer, 104);
      uploadedSelrects.push([
        view.getFloat32(72, true),
        view.getFloat32(76, true),
        view.getFloat32(80, true),
        view.getFloat32(84, true),
      ]);
      diagnostics.uploadedSelrects = [...uploadedSelrects];
    }
    setShapeBaseProps();
  };
  let renderCount = 0;

  // Test-only hierarchy adapter. Production scene.ts currently uploads the page
  // shapes but does not create the nil root/ordered children list needed by
  // full-scene _render_sync. Build that minimal one-child hierarchy here, then
  // call the real full-scene renderer entrypoint; this is not product code.
  const renderSync = renderer._render_sync.bind(renderer);
  renderer._render_sync = () => {
    renderCount++;
    diagnostics.renderCount = renderCount;
    const store = useDocumentStore.getState();
    const currentPage = store.pages[store.currentPageId];
    if (!currentPage || currentPage.rootShapeIds.length !== 1) {
      throw new Error("P04 slice requires exactly one nil-root child");
    }
    const [rootA, rootB, rootC, rootD] = uuidToU32x4("00000000-0000-0000-0000-000000000000");
    renderer._use_shape(rootA, rootB, rootC, rootD);
    renderer._set_shape_type(SHAPE_TYPE.frame);
    const [childA, childB, childC, childD] = uuidToU32x4(currentPage.rootShapeIds[0]);
    renderer._set_children_1(childA, childB, childC, childD);
    renderSync();
    if (typeof renderer._get_selection_rect === "function") {
      const inputPointer = renderer._alloc_bytes(16);
      const input = new DataView(renderer.HEAPU8.buffer, inputPointer, 16);
      [childA, childB, childC, childD].forEach((word, index) => input.setUint32(index * 4, word, true));
      const outputPointer = renderer._get_selection_rect();
      const output = new DataView(renderer.HEAPU8.buffer, outputPointer, 40);
      const width = output.getFloat32(0, true);
      const height = output.getFloat32(4, true);
      const centerX = output.getFloat32(8, true);
      const centerY = output.getFloat32(12, true);
      renderer._free_bytes();
      const rustBounds = { x: centerX - width / 2, y: centerY - height / 2, w: width, h: height };
      rendererBoundsHistory.push(rustBounds);
      diagnostics.rendererBoundsHistory = [...rendererBoundsHistory];
    } else {
      diagnostics.rendererBoundsProbe = "get_selection_rect export unavailable";
    }
  };

  let targetFramebuffer: WebGLFramebuffer | null = null;
  const init = renderer._init.bind(renderer);
  renderer._init = (width: number, height: number) => {
    const canvas = document.querySelector("canvas");
    const gl = canvas?.getContext("webgl2") as WebGL2RenderingContext | null;
    if (!gl) throw new Error("React canvas did not acquire WebGL2 before Rust init");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING);
    diagnostics.canvasSizeAtInit = [width, height];
    init(width, height);
  };

  stage = "mount existing Canvas";
  createRoot(document.getElementById("app")!).render(
    <div style={{ width: 320, height: 240, display: "flex" }}><Canvas /></div>,
  );

  const canvas = await waitFor(() => document.querySelector("canvas"), "React Canvas mount");
  await waitFor(
    () => Array.from(document.querySelectorAll("span")).find((item) => item.textContent?.includes("render-wasm / Skia")),
    "Rust/WASM mode badge",
  );
  stage = "wait for Canvas dimensions";
  if (document.querySelector('span[title^="Phase 5: WebGPU"]')) {
    throw new Error("WebGPU renderer activated; this smoke must exercise the existing Skia/WebGL path");
  }
  await waitFor(() => canvas.width === 320 && canvas.height === 240 ? true : null, "fixture canvas dimensions");
  await new Promise((resolve) => window.setTimeout(resolve, 50));

  const gl = canvas.getContext("webgl2") as WebGL2RenderingContext | null;
  if (!gl) throw new Error("React Canvas WebGL2 context is missing");
  if (targetFramebuffer === undefined) throw new Error("React Canvas did not capture the renderer target framebuffer");
  const expectedColor = [51, 102, 153, 255];
  const readbackY = (logicalY: number) => canvas.height - 1 - logicalY;
  const initialInside = sample(gl, targetFramebuffer, 50, readbackY(40));
  const initialOutside = sample(gl, targetFramebuffer, 5, readbackY(5));
  expectColor(initialInside, expectedColor, "initial fixture interior");
  expectColor(initialOutside, [0, 0, 0, 0], "initial fixture transparent background");
  diagnostics.initialInside = initialInside;
  diagnostics.initialOutside = initialOutside;

  const host = canvas.parentElement as HTMLDivElement;
  const bounds = host.getBoundingClientRect();
  const mouse = (type: string, x: number, y: number) => host.dispatchEvent(new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: bounds.left + x,
    clientY: bounds.top + y,
    buttons: type === "mouseup" ? 0 : 1,
  }));

  stage = "select fixture in scale tool";
  mouse("mousedown", 90, 60);
  const selected = useSelectionStore.getState().selectedIds;
  if (selected.length !== 1 || selected[0] !== shapeId) {
    throw new Error(`Canvas scale-tool hit test did not select fixture UUID: ${JSON.stringify(selected)}`);
  }
  mouse("mousemove", 80, 60);
  mouse("mouseup", 80, 60);
  stage = "wait for React geometry update";
  const updatedShape = await waitFor(() => {
    const next = useDocumentStore.getState().shapes[shapeId];
    return next?.bounds.x > 10 ? next : null;
  }, "one React geometry update");
  diagnostics.updatedBounds = updatedShape.bounds;
  const { oldArea, newArea } = await waitFor(() => {
    try {
      const oldPixel = sample(gl, targetFramebuffer, 15, readbackY(40));
      const newPixel = sample(gl, targetFramebuffer, 30, readbackY(40));
      diagnostics.postEditSamples = { oldPixel, newPixel };
      const oldClear = oldPixel.every((channel) => channel === 0);
      const newColor = newPixel.every((channel, index) => Math.abs(channel - expectedColor[index]) <= 2);
      return oldClear && newColor ? { oldArea: oldPixel, newArea: newPixel } : null;
    } catch (error) {
      diagnostics.postEditReadError = String(error);
      return null;
    }
  }, "renderer to apply geometry update");
  expectColor(oldArea, [0, 0, 0, 0], "post-edit old rectangle edge");
  expectColor(newArea, expectedColor, "post-edit rectangle interior");
  if (browserErrors.length) throw new Error(`browser errors: ${browserErrors.join(" | ")}`);
  diagnostics.selectedIds = selected;
  diagnostics.updatedBounds = updatedShape.bounds;
  diagnostics.oldAreaAfterEdit = oldArea;
  diagnostics.newAreaAfterEdit = newArea;

  stage = "complete";
  status.dataset.status = "passed";
  status.textContent = JSON.stringify({
    runtime: "existing React Canvas + current Rust/WASM renderer",
    adapterVersion: projection.adapterVersion,
    loadedResources: performance.getEntriesByType("resource").map((entry) => entry.name),
    unknownExtension: unknown,
    uuid: shapeId,
    rootOrder: projection.pages[projection.currentPageId].rootShapeIds,
    selectedIds: selected,
    initialBounds: shape.bounds,
    updatedBounds: updatedShape.bounds,
    initialInside,
    initialOutside,
    oldAreaAfterEdit: oldArea,
    newAreaAfterEdit: newArea,
    browserErrors,
    testOnlyNilRootBridge: true,
  }, null, 2);
}

void run().catch(fail);
