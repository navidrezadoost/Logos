import { loadRenderWasm, initCanvasContext, type RenderWasmModule } from "../render-wasm/module";
import { projectAndSyncScene } from "../render-wasm/scene";
import { buildSceneProjection, NIL_RENDERER_ROOT_ID } from "../render-wasm/scene-projection";
import type { Shape } from "../types/shapes";
import nestedFixture from "../../../test/fixtures/renderer-scene-projection/nested-overlap.json";

const report = document.getElementById("a8-status") as HTMLPreElement;
let stage = "setup";
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

function uuidFromView(view: DataView, offset: number): string {
  const hex = [0, 4, 8, 12]
    .map((part) => view.getUint32(offset + part, true).toString(16).padStart(8, "0"))
    .join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function captureFrame(
  gl: WebGL2RenderingContext,
  target: WebGLFramebuffer | null,
  width: number,
  height: number,
): Uint8Array {
  const previousReadFramebuffer = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
  const previousReadBuffer = gl.getParameter(gl.READ_BUFFER) as number;
  const pixels = new Uint8Array(width * height * 4);
  gl.finish();
  try {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, target);
    gl.readBuffer(target === null ? gl.BACK : gl.COLOR_ATTACHMENT0);
    if (gl.checkFramebufferStatus(gl.READ_FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
      throw new Error("renderer target framebuffer is incomplete");
    }
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
  } finally {
    gl.bindFramebuffer(gl.READ_FRAMEBUFFER, previousReadFramebuffer);
    gl.readBuffer(previousReadBuffer);
  }
  return pixels;
}

function pixelAt(pixels: Uint8Array, width: number, height: number, x: number, y: number): number[] {
  const offset = ((height - 1 - y) * width + x) * 4;
  return Array.from(pixels.subarray(offset, offset + 4));
}

const expectedObserved: Array<{ label: string; expected: number[]; observed: number[] }> = [];

function expectPixel(
  pixels: Uint8Array,
  width: number,
  height: number,
  x: number,
  y: number,
  expected: number[],
  label: string,
): void {
  const observed = pixelAt(pixels, width, height, x, y);
  expectedObserved.push({ label, expected, observed });
  if (observed.some((channel, index) => Math.abs(channel - expected[index]) > 2)) {
    throw new Error(`${label}: expected ${expected}, observed ${observed}`);
  }
}

function assertFramesEqual(before: Uint8Array, after: Uint8Array, label: string): void {
  assert(before.length === after.length, `${label}: framebuffer dimensions changed`);
  for (let index = 0; index < before.length; index += 1) {
    if (before[index] !== after[index]) throw new Error(`${label}: framebuffer changed at byte ${index}`);
  }
}

function scopeRecords(shapes: Record<string, Shape>, ids: string[]): Record<string, Shape> {
  return Object.fromEntries(ids.map((id) => [id, shapes[id]]));
}

async function run(): Promise<void> {
  const canvas = document.getElementById("renderer") as HTMLCanvasElement;
  canvas.width = 320;
  canvas.height = 240;

  setStage("load actual Rust/WASM renderer");
  const loaded = await loadRenderWasm("/js/render-wasm.js", "/js/render-wasm.wasm");
  assert(loaded, "Rust/WASM renderer did not load");
  const mod = loaded as RenderWasmModule;
  assert(typeof mod._begin_scene_upload === "function", "renderer is missing resettable scene-upload boundary");
  assert(typeof mod._set_children === "function", "renderer is missing ordered child upload");
  assert(typeof mod._set_children_0 === "function", "renderer is missing empty child upload");

  let targetFramebuffer: WebGLFramebuffer | null | undefined;
  const init = mod._init.bind(mod);
  mod._init = (width, height) => {
    const gl = canvas.getContext("webgl2");
    assert(gl, "WebGL2 context unavailable");
    targetFramebuffer = gl.getParameter(gl.FRAMEBUFFER_BINDING) as WebGLFramebuffer | null;
    init(width, height);
  };
  assert(initCanvasContext(mod, canvas), "renderer could not initialize the test canvas");

  const basePropsById = new Map<string, { parentId: string; rendererType: number }>();
  let lastBasePropsPointer = -1;
  let resizeCalls = 0;
  let basePropsCalls = 0;
  const originalAlloc = mod._alloc_bytes.bind(mod);
  mod._alloc_bytes = (size) => {
    const pointer = originalAlloc(size);
    if (size === 104) lastBasePropsPointer = pointer;
    return pointer;
  };
  const originalBeginSceneUpload = mod._begin_scene_upload.bind(mod);
  mod._begin_scene_upload = (capacity) => {
    basePropsById.clear();
    originalBeginSceneUpload(capacity);
  };
  const originalResizeViewbox = mod._resize_viewbox.bind(mod);
  mod._resize_viewbox = (width, height) => {
    resizeCalls++;
    originalResizeViewbox(width, height);
  };
  const originalSetBaseProps = mod._set_shape_base_props.bind(mod);
  let interruptAfterNextBaseProps = false;
  mod._set_shape_base_props = () => {
    basePropsCalls++;
    assert(lastBasePropsPointer >= 0, "base properties were not staged in the WASM heap");
    const view = new DataView(mod.HEAPU8.buffer, lastBasePropsPointer, 104);
    basePropsById.set(uuidFromView(view, 0), {
      parentId: uuidFromView(view, 16),
      rendererType: view.getUint8(32),
    });
    originalSetBaseProps();
    if (interruptAfterNextBaseProps) {
      interruptAfterNextBaseProps = false;
      throw new Error("injected interruption after a native shape mutation");
    }
  };

  const firstScene = structuredClone(nestedFixture) as unknown as {
    page: { id: string; rootShapeIds: string[] };
    shapes: Record<string, Shape>;
  };
  setStage("pure scene projection and deterministic serialization order");
  const firstProjection = buildSceneProjection(firstScene.page, firstScene.shapes, firstScene.page.rootShapeIds);
  if (!firstProjection.ok) throw new Error(`valid nested fixture rejected: ${firstProjection.diagnostic.code}`);
  assert(firstProjection.plan.rootId === NIL_RENDERER_ROOT_ID, "synthetic root is not renderer-owned nil UUID");
  assert(
    firstProjection.plan.rendererRootChildrenIds.join(",") === [...firstScene.page.rootShapeIds].reverse().join(","),
    "synthetic root children are not serialized back-to-front",
  );
  const outerFrameId = "00000000-0000-4000-8000-000000000020";
  const outerFrame = firstProjection.plan.nodes.find((node) => node.id === outerFrameId);
  assert(outerFrame, "outer frame missing from projection preorder");
  assert(
    outerFrame.rendererChildrenIds.join(",") === [
      "00000000-0000-4000-8000-000000000023",
      "00000000-0000-4000-8000-000000000022",
      "00000000-0000-4000-8000-000000000021",
    ].join(","),
    "children are not serialized in deterministic renderer painter order",
  );

  setStage("render actual overlapping roots and nested frames");
  const firstSync = projectAndSyncScene(
    mod,
    firstScene.page,
    firstScene.shapes,
    firstScene.page.rootShapeIds,
    canvas.width,
    canvas.height,
  );
  if (!firstSync.ok) throw new Error(`valid scene upload failed: ${firstSync.diagnostic.code}`);
  assert(targetFramebuffer !== undefined, "target framebuffer was not captured at renderer init");
  const gl = canvas.getContext("webgl2");
  assert(gl, "WebGL2 context disappeared");
  const initialPixels = captureFrame(gl, targetFramebuffer, canvas.width, canvas.height);
  expectPixel(initialPixels, canvas.width, canvas.height, 43, 43, [255, 0, 0, 255], "top child over lower sibling");
  expectPixel(initialPixels, canvas.width, canvas.height, 50, 50, [255, 255, 0, 255], "top page root over nested content");
  expectPixel(initialPixels, canvas.width, canvas.height, 95, 80, [0, 255, 0, 255], "nested frame descendant");
  assert(
    basePropsById.get("00000000-0000-4000-8000-000000000021")?.parentId === outerFrameId,
    "nested child parent UUID did not reach the Rust shape pool",
  );

  setStage("invalid projection leaves actual renderer untouched");
  const invalidScene = structuredClone(firstScene);
  invalidScene.shapes[outerFrameId].children.push("00000000-0000-4000-8000-000000000099");
  const beginSceneUpload = mod._begin_scene_upload.bind(mod);
  const renderSync = mod._render_sync.bind(mod);
  let beginCount = 0;
  let renderCount = 0;
  mod._begin_scene_upload = (capacity) => { beginCount++; beginSceneUpload(capacity); };
  mod._render_sync = () => { renderCount++; renderSync(); };
  const beforeInvalid = { beginCount, renderCount, resizeCalls, basePropsCalls };
  const invalidResult = projectAndSyncScene(
    mod,
    invalidScene.page,
    invalidScene.shapes,
    invalidScene.page.rootShapeIds,
    canvas.width,
    canvas.height,
  );
  assert(!invalidResult.ok && invalidResult.diagnostic.code === "LOGOS_SCENE_MISSING_CHILD", "invalid scene lacks typed projection diagnostic");
  assert(
    beginCount === beforeInvalid.beginCount &&
      renderCount === beforeInvalid.renderCount &&
      resizeCalls === beforeInvalid.resizeCalls &&
      basePropsCalls === beforeInvalid.basePropsCalls,
    "invalid projection mutated WASM state",
  );
  assertFramesEqual(initialPixels, captureFrame(gl, targetFramebuffer, canvas.width, canvas.height), "invalid projection rejection");

  setStage("interrupted upload restores previous committed framebuffer");
  const frameId = outerFrameId;
  const blueId = "00000000-0000-4000-8000-000000000022";
  const deletedScene: { page: { id: string; rootShapeIds: string[] }; shapes: Record<string, Shape> } = {
    page: { id: firstScene.page.id, rootShapeIds: [frameId] },
    shapes: {
      [frameId]: { ...firstScene.shapes[frameId], children: [blueId] },
      [blueId]: firstScene.shapes[blueId],
    },
  };
  interruptAfterNextBaseProps = true;
  const interrupted = projectAndSyncScene(mod, deletedScene.page, deletedScene.shapes, deletedScene.page.rootShapeIds, canvas.width, canvas.height);
  assert(!interrupted.ok, "native setter interruption was not detected");
  assert(interrupted.diagnostic.code === "LOGOS_SCENE_UPLOAD_INTERRUPTED" && interrupted.diagnostic.recovered, "compensating upload did not restore committed scene");
  assertFramesEqual(initialPixels, captureFrame(gl, targetFramebuffer, canvas.width, canvas.height), "WASM upload recovery");

  setStage("subtree deletion clears old framebuffer pixels");
  const deletion = projectAndSyncScene(mod, deletedScene.page, deletedScene.shapes, deletedScene.page.rootShapeIds, canvas.width, canvas.height);
  if (!deletion.ok) throw new Error(`deletion upload failed: ${deletion.diagnostic.code}`);
  const afterDelete = captureFrame(gl, targetFramebuffer, canvas.width, canvas.height);
  expectPixel(afterDelete, canvas.width, canvas.height, 50, 50, [0, 0, 255, 255], "remaining blue child");
  expectPixel(afterDelete, canvas.width, canvas.height, 35, 35, [0, 0, 0, 0], "removed red child cleared");
  expectPixel(afterDelete, canvas.width, canvas.height, 95, 80, [0, 0, 0, 0], "deleted nested subtree cleared");

  setStage("reparent and move child between frames");
  const newParentId = "00000000-0000-4000-8000-000000000040";
  const movedScene: { page: { id: string; rootShapeIds: string[] }; shapes: Record<string, Shape> } = {
    page: { id: deletedScene.page.id, rootShapeIds: [newParentId, frameId] },
    shapes: {
      [frameId]: { ...deletedScene.shapes[frameId], children: [] },
      [newParentId]: {
        ...deletedScene.shapes[frameId],
        id: newParentId,
        name: "Reparent target frame",
        children: [blueId],
        bounds: { x: 140, y: 20, w: 100, h: 80 },
      },
      [blueId]: {
        ...deletedScene.shapes[blueId],
        parentId: newParentId,
        bounds: { x: 150, y: 35, w: 30, h: 30 },
      },
    },
  };
  const moved = projectAndSyncScene(mod, movedScene.page, movedScene.shapes, movedScene.page.rootShapeIds, canvas.width, canvas.height);
  if (!moved.ok) throw new Error(`reparent upload failed: ${moved.diagnostic.code}`);
  const afterReparent = captureFrame(gl, targetFramebuffer, canvas.width, canvas.height);
  expectPixel(afterReparent, canvas.width, canvas.height, 50, 50, [0, 0, 0, 0], "old child location cleared after reparent");
  expectPixel(afterReparent, canvas.width, canvas.height, 160, 45, [0, 0, 255, 255], "reparented child at new location");
  assert(basePropsById.get(blueId)?.parentId === newParentId, "new parent UUID did not reach the Rust base-props ABI");

  setStage("repeated identical projected sync is deterministic");
  const repeated = projectAndSyncScene(mod, movedScene.page, movedScene.shapes, movedScene.page.rootShapeIds, canvas.width, canvas.height);
  if (!repeated.ok) throw new Error(`repeated scene sync failed: ${repeated.diagnostic.code}`);
  assertFramesEqual(afterReparent, captureFrame(gl, targetFramebuffer, canvas.width, canvas.height), "identical repeated sync");

  setStage("complete");
  report.dataset.status = "passed";
  report.textContent = JSON.stringify({
    status: "passed",
    actualRenderer: "render-wasm/Skia in headless Chromium",
    expectedObserved,
    checks: [
      "synthetic nil-root and deterministic painter-order serialization",
      "overlapping roots and nested-frame descendants render at expected pixels",
      "invalid graph rejected before WASM mutation",
      "interrupted native upload compensates to the last committed pixels",
      "subtree deletion clears removed pixels",
      "reparenting updates uploaded parent ID, old pixels, and new pixels",
      "repeated identical synchronization has byte-identical framebuffer output",
    ],
  }, null, 2);
}

void run().catch((error: unknown) => {
  report.dataset.status = "failed";
  report.textContent = JSON.stringify({ stage, error: String(error), expectedObserved }, null, 2);
});
