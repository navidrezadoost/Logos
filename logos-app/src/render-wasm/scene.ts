/**
 * render-wasm/scene.ts  (M3 revision)
 *
 * Synchronise the Zustand documentStore shapes into the Rust/Skia scene graph.
 *
 * Render path:
 *   documentStore mutation
 *     → useCurrentPageShapes() returns new array
 *     → Canvas.tsx useEffect([shapes, …]) fires
 *     → syncScene(mod, shapes)  ← this file
 *     → _set_shape_base_props() per shape (104-byte batched ABI)
 *     → applySolidFill() per shape
 *     → _render_sync()
 *
 * C2 (updateShape/deleteShape → syncScene):
 *   Resolved by the Zustand subscription pattern. Any mutation to the
 *   `shapes` record triggers useCurrentPageShapes() to emit a new array,
 *   which causes the Canvas.tsx useEffect to fire — no extra wiring needed.
 *   Every syncScene call does a full re-upload of all shapes.
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
import { type Shape, hexToARGB } from "../types/shapes";

// ─────────────────────────────────────────────────────────────────────────────
// Constants — must match Rust RawShapeType enum (render-wasm/src/wasm/shapes/mod.rs)
// ─────────────────────────────────────────────────────────────────────────────

const SHAPE_TYPE_MAP: Record<string, number> = {
  frame:    0,
  group:    1,
  bool:     2,
  rect:     3,
  path:     4,
  text:     5,
  circle:   6,
  ellipse:  6, // circle variant — same u8
  "svg-raw": 7,
};

const FLAG_HIDDEN     = 0x02;
const BLEND_NORMAL    = 0x00;
const CONSTRAINT_NONE = 0xFF;
const BASE_PROPS_SIZE = 104;

/**
 * Push all shapes to the Rust scene graph and render a frame.
 * Called whenever the shape list or a shape's properties change.
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
  // 1. Resize the WASM viewbox to match the canvas
  mod._resize_viewbox(width, height);

  // 2. Allocate pool — total shape count (including nested children)
  const totalShapes = shapes.reduce((n, s) => n + 1 + s.children.length, 0);
  mod._init_shapes_pool(Math.max(totalShapes, 1));

  // 3. Upload every shape via the batched set_shape_base_props ABI
  for (const shape of shapes) {
    uploadShapeBatched(mod, shape);
  }

  // 4. Render
  mod._render_sync();
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
 *   [33]      flags        u8  (bit1=hidden)
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
function uploadShapeBatched(mod: RenderWasmModule, shape: Shape): void {
  const ptr = mod._alloc_bytes(BASE_PROPS_SIZE);
  const heap = mod.HEAPU8;
  const view = new DataView(heap.buffer, ptr, BASE_PROPS_SIZE);

  // id UUID [0..16)
  writeUUID(view, 0, shape.id);

  // parent_id UUID [16..32)
  writeUUID(view, 16, shape.parentId);

  // shape_type [32]
  view.setUint8(32, SHAPE_TYPE_MAP[shape.type] ?? 3 /* rect */);

  // flags [33]  bit1 = hidden
  view.setUint8(33, shape.hidden ? FLAG_HIDDEN : 0);

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
  mod._free_bytes();

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

export function syncScene2D(
  ctx: CanvasRenderingContext2D,
  shapes: Shape[],
  width: number,
  height: number
): void {
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
      ctx.beginPath();
      ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
      ctx.fill();
      break;
    default:
      ctx.fillRect(x, y, w, h);
  }

  ctx.globalAlpha = 1;
}
