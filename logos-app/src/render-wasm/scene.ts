/**
 * render-wasm/scene.ts
 *
 * Synchronise the Zustand documentStore shapes into the Rust/Skia scene graph.
 *
 * This is the M2 bridge: React state → Rust WASM renderer.
 * The flow is:
 *   documentStore changes → syncScene(mod, shapes) → module._render_sync()
 *
 * For Phase M3 this will move to a Worker; for now it runs on the main thread
 * to keep M2 simple and verifiable.
 */

import {
  type RenderWasmModule,
  SHAPE_TYPE,
  applySolidFill,
} from "./module";
import { type Shape, uuidToU32x4, hexToARGB } from "../types/shapes";

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

  // 2. Allocate pool — pass total shape count including children
  const totalShapes = countShapesRecursive(shapes);
  mod._init_shapes_pool(Math.max(totalShapes, 1));

  // 3. Upload every shape
  for (const shape of shapes) {
    uploadShape(mod, shape);
  }

  // 4. Render
  mod._render_sync();
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

function countShapesRecursive(shapes: Shape[]): number {
  return shapes.reduce((count, s) => count + 1 + s.children.length, 0);
}

function uploadShape(mod: RenderWasmModule, shape: Shape): void {
  const [a, b, c, d] = uuidToU32x4(shape.id);

  mod._use_shape(a, b, c, d);
  mod._set_shape_type(shapeTypeNum(shape.type));

  const { x, y, w, h } = shape.bounds;
  mod._set_shape_selrect(x, y, x + w, y + h);

  // Transform matrix [a,b,c,d,e,f]
  const [ta, tb, tc, td, te, tf] = shape.transform;
  mod._set_shape_transform(ta, tb, tc, td, te, tf);

  mod._set_shape_rotation(shape.rotation);
  mod._set_shape_clip_content(false);

  // Fills
  if (shape.fills.length > 0) {
    const fill = shape.fills[0];
    if (fill.type === "solid") {
      applySolidFill(mod, hexToARGB(fill.color, fill.opacity * shape.opacity));
    }
  }
}

const SHAPE_TYPE_MAP: Record<Shape["type"], number> = {
  frame:    SHAPE_TYPE.frame,
  rect:     SHAPE_TYPE.rect,
  circle:   SHAPE_TYPE.circle,
  path:     SHAPE_TYPE.path,
  text:     SHAPE_TYPE.text,
  group:    SHAPE_TYPE.group,
  bool:     SHAPE_TYPE.bool,
  "svg-raw": SHAPE_TYPE.svgRaw,
};

function shapeTypeNum(type: Shape["type"]): number {
  return SHAPE_TYPE_MAP[type] ?? SHAPE_TYPE.rect;
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
