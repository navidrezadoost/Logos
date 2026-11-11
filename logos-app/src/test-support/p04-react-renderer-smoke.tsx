import React from "react";
import { createRoot } from "react-dom/client";
import sourceFixture from "../../../test/fixtures/p0-document-compat/source-document.json";
import { Canvas } from "../components/canvas/Canvas";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import { loadRenderWasm, type RenderWasmModule } from "../render-wasm/module";
import { uuidToU32x4 } from "../types/shapes";
import { projectP04RendererFixture } from "./p04-renderer-fixture";

const status = document.getElementById("p04-status") as HTMLPreElement;
const browserErrors: string[] = [];
window.addEventListener("error", (event) => browserErrors.push(event.error?.stack ?? event.message));
window.addEventListener("unhandledrejection", (event) => browserErrors.push(String(event.reason)));
const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  browserErrors.push(args.map((arg) => String(arg)).join(" "));
  originalConsoleError(...args);
};

function fail(error: unknown): void {
  status.dataset.status = "failed";
  status.textContent = JSON.stringify({ error: String(error), browserErrors }, null, 2);
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
  const projection = projectP04RendererFixture(sourceFixture);
  const shapeId = projection.pages[projection.currentPageId].rootShapeIds[0];
  const shape = projection.shapes[shapeId];
  const unknown = projection.unknownFieldsById[shapeId]["extension-p0"] as { sentinel?: string };
  if (unknown?.sentinel !== "preserve-me") throw new Error("fixture extension sidecar did not preserve sentinel");

  useDocumentStore.setState({
    pages: projection.pages,
    pageOrder: projection.pageOrder,
    currentPageId: projection.currentPageId,
    shapes: projection.shapes,
  });
  useUiStore.setState({ activeTool: "scale", zoom: 1, panX: 0, panY: 0 });
  useSelectionStore.setState({ selectedIds: [] });

  const loaded = await loadRenderWasm("/js/render-wasm.js", "/js/render-wasm.wasm");
  if (!loaded) throw new Error("React canvas could not load the committed Rust/WASM renderer");

  const renderer = loaded as RenderWasmModule & {
    _set_children_1: (a: number, b: number, c: number, d: number) => void;
  };
  if (typeof renderer._set_children_1 !== "function") {
    throw new Error("Rust renderer does not export _set_children_1 for the nil-root bridge");
  }

  // Test-only nil-root bridge. Production syncScene currently reinitializes the
  // pool with only React shapes and does not attach the root's ordered children.
  // This harness supplies the existing ABI root relationship without changing
  // Canvas, scene.ts, or renderer algorithms.
  const initPool = renderer._init_shapes_pool.bind(renderer);
  renderer._init_shapes_pool = (count: number) => {
    initPool(count + 1);
    renderer._use_shape(0, 0, 0, 0);
  };

  const renderSync = renderer._render_sync.bind(renderer);
  renderer._render_sync = () => {
    const store = useDocumentStore.getState();
    const currentPage = store.pages[store.currentPageId];
    if (!currentPage || currentPage.rootShapeIds.length !== 1) {
      throw new Error("P04 slice requires exactly one nil-root child");
    }
    const [a, b, c, d] = uuidToU32x4(currentPage.rootShapeIds[0]);
    renderer._use_shape(0, 0, 0, 0);
    renderer._set_children_1(a, b, c, d);
    renderSync();
  };

  let targetFramebuffer: WebGLFramebuffer | null = null;
  const init = renderer._init.bind(renderer);
  renderer._init = (width: number, height: number) => {
    const canvas = document.querySelector("canvas");
    const gl = canvas?.getContext("webgl2") as WebGL2RenderingContext | null;
    if (!gl) throw new Error("React canvas did not acquire WebGL2 before Rust init");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING);
    init(width, height);
  };

  createRoot(document.getElementById("app")!).render(
    <div style={{ width: 320, height: 240, display: "flex" }}><Canvas /></div>,
  );

  const canvas = await waitFor(() => document.querySelector("canvas"), "React Canvas mount");
  await waitFor(
    () => Array.from(document.querySelectorAll("span")).find((item) => item.textContent?.includes("render-wasm / Skia")),
    "Rust/WASM mode badge",
  );
  if (document.querySelector('span[title^="Phase 5: WebGPU"]')) {
    throw new Error("WebGPU renderer activated; this smoke must exercise the existing Skia/WebGL path");
  }
  await waitFor(() => canvas.width === 320 && canvas.height === 240 ? true : null, "fixture canvas dimensions");
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

  const gl = canvas.getContext("webgl2") as WebGL2RenderingContext | null;
  if (!gl) throw new Error("React Canvas WebGL2 context is missing");
  if (targetFramebuffer === undefined) throw new Error("React Canvas did not capture the renderer target framebuffer");
  const expectedColor = [51, 102, 153, 255];
  const initialInside = sample(gl, targetFramebuffer, 50, 40);
  const initialOutside = sample(gl, targetFramebuffer, 5, 5);
  expectColor(initialInside, expectedColor, "initial fixture interior");
  expectColor(initialOutside, [0, 0, 0, 0], "initial fixture transparent background");

  const host = document.querySelector("#app > div") as HTMLDivElement;
  const bounds = host.getBoundingClientRect();
  const mouse = (type: string, x: number, y: number) => host.dispatchEvent(new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: bounds.left + x,
    clientY: bounds.top + y,
    buttons: type === "mouseup" ? 0 : 1,
  }));

  mouse("mousedown", 90, 60);
  const selected = useSelectionStore.getState().selectedIds;
  if (selected.length !== 1 || selected[0] !== shapeId) {
    throw new Error(`Canvas scale-tool hit test did not select fixture UUID: ${JSON.stringify(selected)}`);
  }
  mouse("mousemove", 80, 60);
  mouse("mouseup", 80, 60);
  const updatedShape = await waitFor(() => {
    const next = useDocumentStore.getState().shapes[shapeId];
    return next?.bounds.x > 10 ? next : null;
  }, "one React geometry update");
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

  const oldArea = sample(gl, targetFramebuffer, 15, 40);
  const newArea = sample(gl, targetFramebuffer, 30, 40);
  expectColor(oldArea, [0, 0, 0, 0], "post-edit old rectangle edge");
  expectColor(newArea, expectedColor, "post-edit rectangle interior");
  if (browserErrors.length) throw new Error(`browser errors: ${browserErrors.join(" | ")}`);

  status.dataset.status = "passed";
  status.textContent = JSON.stringify({
    runtime: "existing React Canvas + current Rust/WASM renderer",
    adapterVersion: projection.adapterVersion,
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
