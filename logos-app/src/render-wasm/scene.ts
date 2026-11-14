/**
 * render-wasm/scene.ts  (M3 revision)
 *
 * Synchronise React scene data into the Rust/Skia renderer scene graph.
 *
 * Render path:
 *   documentStore mutation → current page scene selector → Canvas effect
 *     → projectAndSyncScene() → A8 graph validation and ordered child upload
 *     → _render_sync()
 *
 * `syncScene()` below is retained for legacy flat adapters. Production Canvas
 * uses only the hierarchical A8 path.
 *
 * C3 (binary format):
 *   Uses _set_shape_base_props() which reads 104 bytes from WASM heap,
 *   matching render-wasm/src/wasm/shapes/base_props.rs BASE_PROPS_SIZE=104
 *   exactly. Format documented inline below.
 */

import {
  type RenderWasmModule,
  applySolidFill,
} from "./module";
import { type Shape, hexToARGB, uuidToU32x4 } from "../types/shapes";
import { UnsupportedShapeRenderError } from "./renderer-status";
import { WASM_SHAPE_TYPE_MAP } from "./shape-capabilities";
import {
  buildSceneProjection,
  NIL_RENDERER_ROOT_ID,
  type SceneProjectionDiagnostic,
  type SceneProjectionPage,
  type SceneProjectionPlan,
  type SceneShapeRecords,
} from "./scene-projection";

// ─────────────────────────────────────────────────────────────────────────────
// Constants — must match Rust RawShapeType enum (render-wasm/src/wasm/shapes/mod.rs)
// ─────────────────────────────────────────────────────────────────────────────

export class UnsupportedRendererShapeError extends UnsupportedShapeRenderError {
  readonly discriminator: string;

  constructor(shapeId: string, discriminator: string) {
    super(shapeId, discriminator, "wasm");
    this.name = "UnsupportedRendererShapeError";
    this.discriminator = discriminator;
  }
}

function rustShapeDiscriminator(shape: Shape): number {
  if (!Object.prototype.hasOwnProperty.call(WASM_SHAPE_TYPE_MAP, shape.type)) {
    throw new UnsupportedRendererShapeError(shape.id, String(shape.type));
  }
  return WASM_SHAPE_TYPE_MAP[shape.type];
}

const FLAG_HIDDEN     = 0x02;
const FLAG_CLIP_CONTENT = 0x01;
const BLEND_NORMAL    = 0x00;
const CONSTRAINT_NONE = 0xFF;
const BASE_PROPS_SIZE = 104;

/**
 * Legacy flat uploader. Production Canvas must use `projectAndSyncScene` so
 * nested membership is validated and uploaded through the A8 contract.
 *
 * @param mod    Loaded Emscripten module (null = Canvas 2D, handled by caller).
 * @param shapes Current page shapes in layers order (top shape first).
 * @param width  Canvas logical width (points, not pixels).
 * @param height Canvas logical height (points, not pixels).
 */
export function syncScene(
  mod: RenderWasmModule,
  shapes: Shape[],
  width: number,
  height: number
): void {
  // Validate the whole upload before changing viewbox or pool state. In
  // particular, never silently reinterpret unsupported content as a rectangle.
  const uploadPlan = shapes.map((shape) => ({
    shape,
    rustType: rustShapeDiscriminator(shape),
  }));

  // 1. Resize the WASM viewbox to match the canvas
  mod._resize_viewbox(width, height);

  // 2. Allocate pool — total shape count (including nested children)
  const totalShapes = uploadPlan.reduce((n, { shape }) => n + 1 + shape.children.length, 0);
  mod._init_shapes_pool(Math.max(totalShapes, 1));

  // 3. Upload every shape via the batched set_shape_base_props ABI
  for (const { shape, rustType } of uploadPlan) {
    uploadShapeBatched(mod, shape, rustType);
  }

  // 4. Render
  mod._render_sync();
}

export type SceneUploadFailureCode =
  | "LOGOS_SCENE_UPLOAD_INTERRUPTED"
  | "LOGOS_SCENE_RECOVERY_FAILED"
  | "LOGOS_SCENE_MODULE_POISONED";

export class SceneUploadRecoveryError extends Error {
  constructor(
    readonly code: SceneUploadFailureCode,
    readonly pageId: string,
    readonly recovered: boolean,
  ) {
    super(`${code} for page ${pageId}`);
    this.name = "SceneUploadRecoveryError";
  }
}

export type ProjectedSceneSyncResult =
  | { readonly ok: true; readonly plan: SceneProjectionPlan; readonly metrics: ProjectedSceneMetrics }
  | {
      readonly ok: false;
      readonly diagnostic: SceneProjectionDiagnostic | {
        readonly code: SceneUploadFailureCode;
        readonly pageId: string;
        readonly backend: "wasm";
        readonly recovered: boolean;
      readonly shapeId?: string;
      readonly shapeType?: string;
      };
    };

interface CommittedProjectedScene {
  readonly plan: SceneProjectionPlan;
  readonly width: number;
  readonly height: number;
  readonly view: ProjectedSceneView;
}

const lastCommittedProjectedScene = new WeakMap<RenderWasmModule, CommittedProjectedScene>();
const poisonedRenderModules = new WeakSet<RenderWasmModule>();

export interface ProjectedSceneView {
  readonly zoom: number;
  readonly panX: number;
  readonly panY: number;
}

export interface ProjectedSceneMetrics {
  readonly projectionValidationMs: number;
  readonly wasmUploadMs: number;
  readonly nativeRendererMs: number;
  readonly synchronousMainThreadMs: number;
  readonly wasmReservedBytes: number;
}

interface MutableUploadMetrics { uploadMs: number; nativeMs: number; }

function validateA9Capabilities(plan: SceneProjectionPlan): SceneProjectionDiagnostic | null {
  const byId = new Map(plan.nodes.filter((node) => !node.synthetic).map((node) => [node.id, node.shape]));
  for (const node of plan.nodes) {
    if (node.synthetic) continue;
    const shape = node.shape;
    const reject = (reason: string): SceneProjectionDiagnostic => ({
      code: "LOGOS_SCENE_UNSUPPORTED_CAPABILITY",
      pageId: plan.pageId,
      shapeId: shape.id,
      shapeType: `${shape.type}: ${reason}`,
      backend: "wasm",
    });
    if (shape.type !== "rect" && shape.type !== "frame" && shape.type !== "group") {
      return reject("A9 supports only rectangles, frames, and groups");
    }
    if (!Array.isArray(shape.transform) || shape.transform.length !== 6 || !shape.transform.every(Number.isFinite)) {
      return reject("affine transform is malformed");
    }
    if (shape.transform.some((value, index) => value !== [1, 0, 0, 1, 0, 0][index]) || shape.rotation !== 0) {
      return reject("non-identity transforms and rotation are not verified");
    }
    if (!shape.bounds || typeof shape.bounds !== "object") return reject("bounds are malformed");
    const { x, y, w, h } = shape.bounds;
    if (![x, y, w, h, shape.opacity].every(Number.isFinite) || w < 0 || h < 0 || shape.opacity < 0 || shape.opacity > 1) {
      return reject("invalid bounds or opacity");
    }
    if (!Array.isArray(shape.fills) || shape.fills.length > 1 || shape.fills.some((fill) => !fill || fill.type !== "solid")) {
      return reject("only zero or one solid fill is verified");
    }
    const solidFill = shape.fills[0];
    if (solidFill?.type === "solid" && (typeof solidFill.color !== "string" || !/^#[0-9a-f]{6}$/i.test(solidFill.color) ||
        !Number.isFinite(solidFill.opacity) || solidFill.opacity < 0 || solidFill.opacity > 1)) {
      return reject("solid fill color or opacity is invalid");
    }
    if (shape.type === "frame") {
      // The renderer clips Frame children by default. Its current React shape
      // contract has no clip switch, so require all descendants to be inside
      // every Frame; this makes the enabled clip behavior visually equivalent.
      const stack = [...shape.children];
      const seen = new Set<string>();
      while (stack.length) {
        const childId = stack.pop()!;
        if (seen.has(childId)) continue;
        seen.add(childId);
        const child = byId.get(childId);
        if (!child) continue; // Structural diagnostics are returned by A8 first.
        const b = child.bounds;
        if (b.x < x || b.y < y || b.x + b.w > x + w || b.y + b.h > y + h) {
          return reject("Frame clipping of out-of-bounds descendants is not qualified");
        }
        stack.push(...child.children);
      }
    }
  }
  return null;
}

/** Validate the whole candidate graph before invoking any WASM mutator. */
export function projectAndSyncScene(
  mod: RenderWasmModule,
  page: SceneProjectionPage,
  pageShapeRecords: SceneShapeRecords,
  orderedRootIds: readonly string[],
  width: number,
  height: number,
  view: ProjectedSceneView = { zoom: 1, panX: 0, panY: 0 },
): ProjectedSceneSyncResult {
  const startedAt = performance.now();
  const projection = buildSceneProjection(page, pageShapeRecords, orderedRootIds);
  if (!projection.ok) return projection;
  const capabilityDiagnostic = validateA9Capabilities(projection.plan);
  if (capabilityDiagnostic) return { ok: false, diagnostic: capabilityDiagnostic };
  const projectionValidationMs = performance.now() - startedAt;
  const uploadMetrics: MutableUploadMetrics = { uploadMs: 0, nativeMs: 0 };

  try {
    syncProjectedScene(mod, projection.plan, width, height, view, uploadMetrics);
    return {
      ok: true,
      plan: projection.plan,
      metrics: {
        projectionValidationMs,
        wasmUploadMs: uploadMetrics.uploadMs,
        nativeRendererMs: uploadMetrics.nativeMs,
        synchronousMainThreadMs: performance.now() - startedAt,
        wasmReservedBytes: mod.HEAPU8.buffer.byteLength,
      },
    };
  } catch (error) {
    if (!(error instanceof SceneUploadRecoveryError)) throw error;
    return {
      ok: false,
      diagnostic: {
        code: error.code,
        pageId: error.pageId,
        backend: "wasm",
        recovered: error.recovered,
      },
    };
  }
}

/**
 * Upload a previously validated plan. WASM uses a live singleton pool rather
 * than a staging transaction: on interruption, reset the pool and re-upload
 * the last committed plan before returning. If compensation fails, poison this
 * module and make no further calls; the owner must recreate the renderer.
 */
function syncProjectedScene(
  mod: RenderWasmModule,
  plan: SceneProjectionPlan,
  width: number,
  height: number,
  view: ProjectedSceneView,
  metrics: MutableUploadMetrics,
): void {
  if (poisonedRenderModules.has(mod)) {
    throw new SceneUploadRecoveryError("LOGOS_SCENE_MODULE_POISONED", plan.pageId, false);
  }

  const committed = lastCommittedProjectedScene.get(mod);
  try {
    uploadProjectedPlan(mod, plan, width, height, view, metrics);
    lastCommittedProjectedScene.set(mod, {
      plan: cloneProjectionPlan(plan), width, height, view: { ...view },
    });
  } catch {
    const recoveryPlan = committed?.plan ?? emptyProjectionPlan(plan.pageId);
    const recoveryWidth = committed?.width ?? width;
    const recoveryHeight = committed?.height ?? height;
    const recoveryView = committed?.view ?? view;
    try {
      uploadProjectedPlan(mod, recoveryPlan, recoveryWidth, recoveryHeight, recoveryView, metrics);
      lastCommittedProjectedScene.set(mod, {
        plan: recoveryPlan, width: recoveryWidth, height: recoveryHeight, view: { ...recoveryView },
      });
    } catch {
      poisonedRenderModules.add(mod);
      throw new SceneUploadRecoveryError("LOGOS_SCENE_RECOVERY_FAILED", plan.pageId, false);
    }
    throw new SceneUploadRecoveryError("LOGOS_SCENE_UPLOAD_INTERRUPTED", plan.pageId, true);
  }
}

function uploadProjectedPlan(
  mod: RenderWasmModule,
  plan: SceneProjectionPlan,
  width: number,
  height: number,
  view: ProjectedSceneView,
  metrics: MutableUploadMetrics,
): void {
  const uploadStartedAt = performance.now();
  mod._resize_viewbox(width, height);
  mod._set_view(view.zoom, view.panX, view.panY);
  mod._begin_scene_upload(plan.nodes.length);

  for (const node of plan.nodes) {
    uploadShapeBatched(mod, node.shape, node.rendererType);
  }

  for (const node of plan.nodes) {
    uploadChildren(mod, node.id, node.rendererChildrenIds);
  }

  metrics.uploadMs += performance.now() - uploadStartedAt;
  const rendererStartedAt = performance.now();
  mod._render_sync();
  metrics.nativeMs += performance.now() - rendererStartedAt;
}

function uploadChildren(mod: RenderWasmModule, parentId: string, childIds: readonly string[]): void {
  const [a, b, c, d] = uuidToU32x4(parentId);
  mod._use_shape(a, b, c, d);
  if (childIds.length === 0) {
    mod._set_children_0();
    return;
  }

  const byteLength = childIds.length * 16;
  const pointer = mod._alloc_bytes(byteLength);
  try {
    const view = new DataView(mod.HEAPU8.buffer, pointer, byteLength);
    childIds.forEach((id, index) => writeUUID(view, index * 16, id));
    // Rust's set_children consumes the allocated byte buffer.
    mod._set_children();
  } finally {
    // If the Rust setter traps before consuming the buffer, release it so the
    // compensating full upload can allocate its own records.
    mod._free_bytes();
  }
}

function emptyProjectionPlan(pageId: string): SceneProjectionPlan {
  const result = buildSceneProjection({ id: pageId, rootShapeIds: [] }, {}, []);
  if (!result.ok) throw new Error("Unable to create the empty renderer recovery plan");
  return result.plan;
}

function cloneProjectionPlan(plan: SceneProjectionPlan): SceneProjectionPlan {
  return structuredClone(plan);
}

// ─────────────────────────────────────────────────────────────────────────────
// Batched shape uploader — matches shapes.cljs / set_shape_base_props Rust ABI
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Write one shape's 104-byte base-props record into the WASM heap and call
 * _set_shape_base_props(), then write fills separately.
 *
 * Binary layout:
 *   [0..16)   id UUID      4×u32 LE
 *   [16..32)  parent_id    4×u32 LE (zeros if null)
 *   [32]      shape_type   u8
 *   [33]      flags        u8  (bit0=clip, bit1=hidden)
 *   [34]      blend_mode   u8  (0=Normal)
 *   [35]      constraint_h u8  (0xFF=None)
 *   [36]      constraint_v u8  (0xFF=None)
 *   [37..40)  padding
 *   [40..44)  opacity      f32 LE
 *   [44..48)  rotation     f32 LE  (degrees)
 *   [48..72)  transform    6×f32 LE (a,b,c,d,e,f)
 *   [72..88)  selrect      4×f32 LE (x1,y1,x2,y2)
 *   [88..104) corners      4×f32 LE (r1,r2,r3,r4)
 */
function uploadShapeBatched(mod: RenderWasmModule, shape: Shape, rustType: number): void {
  const ptr = mod._alloc_bytes(BASE_PROPS_SIZE);
  try {
    const heap = mod.HEAPU8;
    const view = new DataView(heap.buffer, ptr, BASE_PROPS_SIZE);

    // id UUID [0..16)
    writeUUID(view, 0, shape.id);

    // parent_id UUID [16..32)
    writeUUID(view, 16, shape.parentId);

    // shape_type [32]
    view.setUint8(32, rustType);

    // flags [33]: frame clipping (renderer default) and visibility.
    view.setUint8(33, (shape.hidden ? FLAG_HIDDEN : 0) | (shape.type === "frame" ? FLAG_CLIP_CONTENT : 0));

    // blend_mode [34]
    view.setUint8(34, BLEND_NORMAL);

    // constraint_h [35], constraint_v [36]
    view.setUint8(35, CONSTRAINT_NONE);
    view.setUint8(36, CONSTRAINT_NONE);

    // padding [37..40) — DataView on zeroed ArrayBuffer, no need to write

    // opacity [40]
    view.setFloat32(40, shape.opacity, true);

    // rotation [44] — Rust expects degrees
    view.setFloat32(44, shape.rotation, true);

    // transform [48..72)
    const [a, b, c, d, e, f] = shape.transform;
    view.setFloat32(48, a, true);
    view.setFloat32(52, b, true);
    view.setFloat32(56, c, true);
    view.setFloat32(60, d, true);
    view.setFloat32(64, e, true);
    view.setFloat32(68, f, true);

    // selrect [72..88)
    const { x, y, w, h } = shape.bounds;
    view.setFloat32(72, x,     true);
    view.setFloat32(76, y,     true);
    view.setFloat32(80, x + w, true);
    view.setFloat32(84, y + h, true);

    // corners [88..104) — zero for non-frames (already zero from alloc)
    view.setFloat32(88,  0, true);
    view.setFloat32(92,  0, true);
    view.setFloat32(96,  0, true);
    view.setFloat32(100, 0, true);

    // Commit — Rust reads from the last alloc
    mod._set_shape_base_props();
  } finally {
    // Rust's bytes() consumes this allocation on success; this is a no-op then
    // and frees the pending buffer if a setter throws before consuming it.
    mod._free_bytes();
  }

  // Fills (separate alloc per the fill ABI)
  if (shape.fills.length > 0) {
    const fill = shape.fills[0];
    if (fill.type === "solid") {
      applySolidFill(mod, hexToARGB(fill.color, fill.opacity * shape.opacity));
    }
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// UUID helper
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Write a UUID string (or null → zeros) as 4×u32 LE into a DataView.
 * Matches uuid/get-u32 in ClojureScript and uuid_from_u32_quartet() in Rust.
 */
function writeUUID(view: DataView, offset: number, uuid: string | null): void {
  if (!uuid) {
    view.setUint32(offset + 0,  0, true);
    view.setUint32(offset + 4,  0, true);
    view.setUint32(offset + 8,  0, true);
    view.setUint32(offset + 12, 0, true);
    return;
  }
  const hex = uuid.replace(/-/g, "");
  view.setUint32(offset + 0,  parseInt(hex.slice( 0,  8), 16), true);
  view.setUint32(offset + 4,  parseInt(hex.slice( 8, 16), 16), true);
  view.setUint32(offset + 8,  parseInt(hex.slice(16, 24), 16), true);
  view.setUint32(offset + 12, parseInt(hex.slice(24, 32), 16), true);
}

// ─────────────────────────────────────────────────────────────────────────────
// Canvas 2D fallback renderer  (active when Skia not available)
// ─────────────────────────────────────────────────────────────────────────────

const CANVAS_2D_SHAPE_TYPES = new Set<string>([
  "rect",
  "frame",
  "circle",
  "ellipse",
  "vector-network",
]);

export class UnsupportedCanvas2DShapeError extends UnsupportedShapeRenderError {
  readonly discriminator: string;

  constructor(shapeId: string, discriminator: string) {
    super(shapeId, discriminator, "canvas2d");
    this.name = "UnsupportedCanvas2DShapeError";
    this.discriminator = discriminator;
  }
}

export function syncScene2D(
  ctx: CanvasRenderingContext2D,
  shapes: Shape[],
  width: number,
  height: number
): void {
  // Validate the entire frame before clearRect/fillRect or any shape draw. A
  // rejected fallback must not leave a misleading partial rectangle frame.
  for (const shape of shapes) {
    if (!CANVAS_2D_SHAPE_TYPES.has(shape.type)) {
      throw new UnsupportedCanvas2DShapeError(shape.id, String(shape.type));
    }
  }

  ctx.clearRect(0, 0, width, height);
  ctx.fillStyle = "#1e1e2e";
  ctx.fillRect(0, 0, width, height);

  // Draw back-to-front (shapes array is top-first → reverse for painter's order)
  for (const shape of [...shapes].reverse()) {
    draw2DShape(ctx, shape);
  }
}

function draw2DShape(ctx: CanvasRenderingContext2D, shape: Shape): void {
  if (shape.hidden) return;

  ctx.globalAlpha = shape.opacity;

  const fill = shape.fills[0];
  const color = fill?.type === "solid" ? fill.color : "#888888";
  ctx.fillStyle = color;

  const { x, y, w, h } = shape.bounds;

  switch (shape.type) {
    case "rect":
    case "frame":
      ctx.fillRect(x, y, w, h);
      break;
    case "circle":
    case "ellipse":
      ctx.beginPath();
      ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
      ctx.fill();
      break;
    case "vector-network": {
      const anchors = shape.vnAnchors;
      const segments = shape.vnSegments;
      if (anchors && segments && anchors.length > 0) {
        ctx.beginPath();
        for (const seg of segments) {
          const a = anchors[seg.s];
          const b = anchors[seg.e];
          if (!a || !b) continue;
          ctx.moveTo(a.x, a.y);
          if (seg.c1 && seg.c2) {
            ctx.bezierCurveTo(seg.c1[0], seg.c1[1], seg.c2[0], seg.c2[1], b.x, b.y);
          } else {
            ctx.lineTo(b.x, b.y);
          }
        }
        ctx.fillStyle = color;
        ctx.fill("evenodd");
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
      break;
    }
    default:
      throw new UnsupportedCanvas2DShapeError(shape.id, String(shape.type));
  }

  ctx.globalAlpha = 1;
}
