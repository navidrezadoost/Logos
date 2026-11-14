import { createRoot, type Root } from "react-dom/client";
import { Canvas } from "../components/canvas/Canvas";
import { LayersPanel } from "../components/layers/LayersPanel";
import { loadRenderWasm, type RenderWasmModule } from "../render-wasm/module";
import { useDocumentStore, type Page } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import { useUiStore } from "../stores/uiStore";
import type { Shape } from "../types/shapes";
import fixtureJson from "../../../test/fixtures/renderer-scene-projection/nested-overlap.json";

const report = document.getElementById("a9-status") as HTMLPreElement;
type FailureMode = "none" | "once-after-mutation" | "always-after-mutation";
let failureMode: FailureMode = "none";
let renderer: RenderWasmModule | null = null;
let targetFramebuffer: WebGLFramebuffer | null | undefined;
let uploads = 0;
const performanceRuns: Array<Record<string, number>> = [];

function stage(value: string): void {
  report.dataset.stage = value;
  report.textContent = `running: ${value}`;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function colorAt(pixels: Uint8Array, width: number, height: number, x: number, y: number): number[] {
  return Array.from(pixels.subarray(((height - 1 - y) * width + x) * 4, ((height - 1 - y) * width + x) * 4 + 4));
}

function expectColor(pixels: Uint8Array, width: number, height: number, x: number, y: number, expected: number[], label: string): void {
  const observed = colorAt(pixels, width, height, x, y);
  assert(observed.every((channel, index) => Math.abs(channel - expected[index]) <= 3), `${label}: expected ${expected}, got ${observed}`);
}

function samePixels(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function pixels(): Uint8Array {
  const canvas = document.querySelector<HTMLCanvasElement>("canvas");
  const gl = canvas?.getContext("webgl2");
  assert(canvas && gl && targetFramebuffer !== undefined, "production Canvas WebGL framebuffer unavailable");
  const result = new Uint8Array(canvas.width * canvas.height * 4);
  gl.finish();
  const previous = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
  const previousBuffer = gl.getParameter(gl.READ_BUFFER) as number;
  try {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, targetFramebuffer);
    gl.readBuffer(targetFramebuffer === null ? gl.BACK : gl.COLOR_ATTACHMENT0);
    gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, result);
  } finally {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previous);
    gl.readBuffer(previousBuffer);
  }
  return result;
}

async function waitFor(predicate: () => boolean, label: string, timeoutMs = 12000): Promise<void> {
  const started = performance.now();
  while (!predicate()) {
    if (performance.now() - started > timeoutMs) throw new Error(`timeout waiting for ${label}; status=${document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status")}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

async function waitReady(label: string): Promise<void> {
  await waitFor(() => {
    const root = document.querySelector("[data-renderer-status]");
    const canvas = document.querySelector<HTMLCanvasElement>("canvas");
    if (root?.getAttribute("data-renderer-status") !== "READY" || !canvas) return false;
    const raw = root.getAttribute("data-renderer-metrics");
    if (!raw) return false;
    const metrics = JSON.parse(raw) as { rendererWidth?: number; rendererHeight?: number };
    return metrics.rendererWidth === canvas.width && metrics.rendererHeight === canvas.height &&
      canvas.width === Math.floor(canvas.getBoundingClientRect().width) &&
      canvas.height === Math.floor(canvas.getBoundingClientRect().height);
  }, `${label} resized READY frame and metrics`);
  const raw = document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-metrics");
  if (raw) performanceRuns.push(JSON.parse(raw) as Record<string, number>);
}

async function waitRejected(code: string, label: string): Promise<void> {
  await waitFor(() => {
    const root = document.querySelector("[data-renderer-status]");
    return root?.getAttribute("data-renderer-status") === "SCENE_REJECTED" && root.getAttribute("data-renderer-diagnostic") === code;
  }, `${label} ${code}`);
}

const base = structuredClone(fixtureJson) as unknown as {
  page: Page;
  shapes: Record<string, Shape>;
};
const outerFrameId = "00000000-0000-4000-8000-000000000020";
const redId = "00000000-0000-4000-8000-000000000021";
const blueId = "00000000-0000-4000-8000-000000000022";
const nestedFrameId = "00000000-0000-4000-8000-000000000023";
const greenId = "00000000-0000-4000-8000-000000000024";
const yellowId = "00000000-0000-4000-8000-000000000030";
const groupId = "00000000-0000-4000-8000-000000000025";
const group: Shape = {
  ...base.shapes[outerFrameId],
  id: groupId,
  type: "group",
  name: "Nested group",
  parentId: outerFrameId,
  children: [redId],
};
base.shapes[outerFrameId].children = [groupId, blueId, nestedFrameId];
base.shapes[redId].parentId = groupId;
base.shapes[groupId] = group;

function installScene(page: Page, shapes: Record<string, Shape>): void {
  useDocumentStore.setState((state) => ({
    currentPageId: page.id,
    pages: { ...state.pages, [page.id]: page },
    shapes,
  }));
}

async function instrumentedLoader(jsUrl: string, wasmUrl: string): Promise<RenderWasmModule | null> {
  const mod = await loadRenderWasm(jsUrl, wasmUrl);
  if (!mod) return null;
  renderer = mod;
  const originalInit = mod._init.bind(mod);
  mod._init = (width, height) => {
    const canvas = document.querySelector<HTMLCanvasElement>("canvas");
    const gl = canvas?.getContext("webgl2");
    assert(gl, "production Canvas did not create a WebGL2 context");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    originalInit(width, height);
  };
  const originalBegin = mod._begin_scene_upload.bind(mod);
  mod._begin_scene_upload = (count) => {
    uploads += 1;
    originalBegin(count);
  };
  const originalBaseProps = mod._set_shape_base_props.bind(mod);
  mod._set_shape_base_props = () => {
    originalBaseProps();
    if (failureMode === "once-after-mutation") {
      failureMode = "none";
      throw new Error("A9 test-only one-shot upload interruption");
    }
    if (failureMode === "always-after-mutation") throw new Error("A9 test-only recovery interruption");
  };
  return mod;
}

function renderCanvas(root: Root): void {
  root.render(<div style={{ width: 540, height: 240, display: "flex" }}><LayersPanel /><div style={{ flex: 1 }}><Canvas rendererLoader={instrumentedLoader} /></div></div>);
}

async function run(): Promise<void> {
  report.dataset.status = "running";
  const root = createRoot(document.getElementById("root")!);
  useUiStore.setState({ zoom: 1, panX: 0, panY: 0 });
  installScene(base.page, base.shapes);
  renderCanvas(root);

  stage("production Canvas initial hierarchy and layer selection");
  await waitReady("initial production frame");
  const canvas = document.querySelector<HTMLCanvasElement>("canvas");
  assert(canvas && canvas.width > 1 && canvas.height > 1, "production Canvas was not sized by ResizeObserver");
  const initial = pixels();
  expectColor(initial, canvas.width, canvas.height, 43, 43, [255, 0, 0, 255], "Frame → Group → Rectangle over sibling");
  expectColor(initial, canvas.width, canvas.height, 50, 50, [255, 255, 0, 255], "top overlapping root order");
  expectColor(initial, canvas.width, canvas.height, 95, 80, [0, 255, 0, 255], "nested Frame descendant");
  assert(document.querySelector("#root")?.textContent?.includes("Nested group"), "nested group is missing from production LayersPanel");
  useSelectionStore.getState().select(redId);
  assert(useSelectionStore.getState().selectedIds[0] === redId, "nested child cannot be selected");

  stage("zoom and pan use the renderer viewport");
  useUiStore.setState({ zoom: 1.5, panX: 10, panY: 15 });
  await waitReady("zoom and pan frame");
  const transformed = pixels();
  assert(!samePixels(initial, transformed), "zoom and pan did not change the production renderer framebuffer");
  expectColor(transformed, canvas.width, canvas.height, 75, 80, [255, 0, 0, 255], "zoomed and panned nested child");
  useUiStore.setState({ zoom: 1, panX: 0, panY: 0 });
  await waitReady("reset view frame");

  stage("solid fill and visibility mutations invalidate the scene");
  const changed = structuredClone(base.shapes);
  changed[yellowId].fills = [{ type: "solid", color: "#00ffff", opacity: 1 }];
  changed[blueId].hidden = true;
  installScene(base.page, changed);
  await waitReady("fill and visibility frame");
  let frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 50, 50, [0, 255, 255, 255], "solid-fill update");
  expectColor(frame, canvas.width, canvas.height, 43, 43, [255, 0, 0, 255], "hidden sibling reveals nested rectangle");

  stage("fixture-injected reparent preserves IDs and changes hierarchy");
  const targetGroupId = "00000000-0000-4000-8000-000000000041";
  const moved = structuredClone(changed);
  moved[outerFrameId].children = [blueId, nestedFrameId];
  moved[groupId].children = [];
  moved[redId].parentId = targetGroupId;
  moved[redId].bounds = { x: 165, y: 40, w: 40, h: 40 };
  moved[targetGroupId] = { ...group, id: targetGroupId, name: "Reparent target group", parentId: null, children: [redId], bounds: { x: 150, y: 20, w: 80, h: 80 } };
  const reparentPage = { ...base.page, rootShapeIds: [targetGroupId, ...base.page.rootShapeIds] };
  installScene(reparentPage, moved);
  await waitReady("reparent frame");
  frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 180, 50, [255, 0, 0, 255], "reparented rectangle");

  stage("child insertion and deletion, then subtree deletion");
  const insertedId = "00000000-0000-4000-8000-000000000042";
  const inserted = structuredClone(base.shapes);
  inserted[groupId].children.push(insertedId);
  inserted[insertedId] = { ...inserted[redId], id: insertedId, name: "Inserted rectangle", parentId: groupId, bounds: { x: 100, y: 35, w: 12, h: 12 }, fills: [{ type: "solid", color: "#ff00ff", opacity: 1 }] };
  installScene(base.page, inserted);
  await waitReady("child insertion frame");
  frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 105, 40, [255, 0, 255, 255], "inserted child");
  useDocumentStore.getState().removeShape(insertedId);
  await waitReady("child deletion frame");
  assert(!useDocumentStore.getState().shapes[insertedId], "deleted child remains in document store");
  frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 105, 40, [0, 0, 0, 0], "deleted child clears old pixels");
  useDocumentStore.getState().removeShape(outerFrameId);
  await waitReady("subtree deletion frame");
  for (const id of [outerFrameId, groupId, redId, blueId, nestedFrameId, greenId]) {
    assert(!useDocumentStore.getState().shapes[id], `subtree deletion left ${id}`);
  }
  frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 95, 80, [0, 0, 0, 0], "subtree deletion clears descendants");

  stage("page switch uploads only the selected page");
  const page2: Page = { id: "a9-page-two", name: "A9 second page", rootShapeIds: [yellowId] };
  installScene(page2, { ...base.shapes, [yellowId]: { ...base.shapes[yellowId], bounds: { x: 200, y: 120, w: 25, h: 25 } } });
  await waitReady("page switch frame");
  frame = pixels();
  expectColor(frame, canvas.width, canvas.height, 210, 130, [255, 255, 0, 255], "second page root");
  expectColor(frame, canvas.width, canvas.height, 50, 50, [0, 0, 0, 0], "old page content removed");

  stage("invalid graph rejects before native mutation and preserves frame");
  const beforeInvalid = pixels();
  const invalid = structuredClone(base.shapes);
  invalid[outerFrameId].children.push("00000000-0000-4000-8000-000000000099");
  installScene(base.page, invalid);
  const uploadsBeforeInvalid = uploads;
  await waitRejected("LOGOS_SCENE_MISSING_CHILD", "invalid graph");
  assert(uploads === uploadsBeforeInvalid, "invalid graph mutated native scene upload");
  assert(samePixels(beforeInvalid, pixels()), "invalid graph changed last committed framebuffer");

  stage("unsupported transformed parent is rejected before upload");
  const transformedParent = structuredClone(base.shapes);
  transformedParent[outerFrameId].transform = [1, 0, 0, 1, 5, 0];
  const uploadsBeforeTransform = uploads;
  installScene(base.page, transformedParent);
  await waitRejected("LOGOS_SCENE_UNSUPPORTED_CAPABILITY", "transformed parent");
  assert(uploads === uploadsBeforeTransform, "unsupported transform mutated native scene upload");

  stage("out-of-bounds frame clipping behavior is rejected before upload");
  const unclipped = structuredClone(base.shapes);
  unclipped[redId].bounds = { x: 130, y: 30, w: 40, h: 40 };
  const uploadsBeforeClip = uploads;
  installScene(base.page, unclipped);
  await waitRejected("LOGOS_SCENE_UNSUPPORTED_CAPABILITY", "unqualified frame clipping");
  assert(uploads === uploadsBeforeClip, "unqualified clipping behavior mutated native scene upload");

  stage("native upload interruption restores the committed frame");
  installScene(base.page, base.shapes);
  await waitReady("restore valid frame");
  const beforeInterrupt = pixels();
  failureMode = "once-after-mutation";
  const mutate = structuredClone(base.shapes);
  mutate[yellowId].fills = [{ type: "solid", color: "#00ff00", opacity: 1 }];
  installScene(base.page, mutate);
  await waitRejected("LOGOS_SCENE_UPLOAD_INTERRUPTED", "interrupted upload");
  assert(samePixels(beforeInterrupt, pixels()), "recovery did not restore the committed framebuffer");

  stage("repeat mount and unmount lifecycle");
  root.unmount();
  await new Promise((resolve) => setTimeout(resolve, 100));
  const secondRoot = createRoot(document.getElementById("root")!);
  installScene(base.page, base.shapes);
  renderCanvas(secondRoot);
  await waitReady("remounted production Canvas");

  stage("recovery failure reports an unverified frame");
  failureMode = "always-after-mutation";
  const secondMutation = structuredClone(base.shapes);
  secondMutation[yellowId].fills = [{ type: "solid", color: "#00ff00", opacity: 1 }];
  installScene(base.page, secondMutation);
  await waitRejected("LOGOS_SCENE_RECOVERY_FAILED", "failed recovery");
  assert(document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-frame-disposition") === "unverified", "failed recovery was reported as a verified prior frame");
  failureMode = "none";
  secondRoot.unmount();

  const environment = {
    browser: navigator.userAgent,
    hardware: {
      platform: navigator.platform,
      logicalProcessors: navigator.hardwareConcurrency,
      deviceMemoryGiB: (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? null,
      webglVendor: (() => {
        const gl = canvas.getContext("webgl2");
        const ext = gl?.getExtension("WEBGL_debug_renderer_info");
        return ext && gl ? {
          vendor: gl.getParameter(ext.UNMASKED_VENDOR_WEBGL),
          renderer: gl.getParameter(ext.UNMASKED_RENDERER_WEBGL),
        } : null;
      })(),
    },
    viewport: { width: canvas.width, height: canvas.height, devicePixelRatio: window.devicePixelRatio },
    wasmReservedBytes: renderer?.HEAPU8.buffer.byteLength ?? null,
    jsHeapUsedBytes: (performance as Performance & { memory?: { usedJSHeapSize?: number } }).memory?.usedJSHeapSize ?? null,
    measurementCount: performanceRuns.length,
    measurements: performanceRuns,
    notes: [
      "Production Canvas component and actual Rust/WASM renderer; all document mutations after initial mount are fixture/store injected except page switching through the live store.",
      "synchronousMainThreadMs is the synchronous projection, validation, upload, and native render call duration; browserFrameLatencyMs is time from return to the next animation frame.",
      "WASM reserved bytes are the WebAssembly memory buffer size. Chromium JS heap is an observed point-in-time value, not a retention study or leak claim.",
    ],
  };
  report.dataset.status = "passed";
  report.textContent = JSON.stringify({ status: "passed", environment, checks: [
    "initial nested Frame→Group→Rectangle and overlapping roots pixel output",
    "nested layer listing and selection ID",
    "zoom/pan, solid fill, visibility, fixture reparenting",
    "child insertion/deletion, subtree deletion, stale-pixel removal, page switch",
    "invalid graph rejection before native upload; transformed parent fail-closed",
    "interrupted upload recovery, recovery-failure reporting, repeat mount/unmount",
  ] }, null, 2);
}

run().catch((error: unknown) => {
  report.dataset.status = "failed";
  report.textContent = `FAILED at ${report.dataset.stage}: ${error instanceof Error ? error.message : String(error)}`;
});
