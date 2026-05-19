/**
 * render-webgpu/shape-buffer.ts
 *
 * Packs React/TypeScript `Shape` objects into the flat Float32Array layout
 * that the WGSL shaders expect.
 *
 * ShapeEntry layout (12 × f32 = 48 bytes):
 *   [x, y, w, h, r, g, b, a, rotation, opacity, shape_type, flags]
 *
 * The buffer is a triple: the CPU Float32Array (CPU-writable), the GPU
 * GPUBuffer (mapped for upload), and the shape-count actually written.
 */

import type { Shape } from "../types/shapes";
import type { GradientAtlas } from "./gradient-atlas";
import { SHAPE_ENTRY_F32S, MAX_SHAPES, GRADIENT_ENTRY_F32S } from "./constants";

// ─────────────────────────────────────────────────────────────────────────────
// Fill-type encoding  (must match tile.wgsl ShapeEntry.fill_type)
// ─────────────────────────────────────────────────────────────────────────────

const FILL_TYPE_SOLID          = 0;
const FILL_TYPE_LINEAR_GRADIENT = 1;
const FILL_TYPE_RADIAL_GRADIENT = 2;

const SHAPE_TYPE_RECT    = 0;
const SHAPE_TYPE_ELLIPSE = 1;
const SHAPE_TYPE_OTHER   = 2;

function encodeShapeType(type: Shape["type"]): number {
  switch (type) {
    case "rect":
    case "frame":
    case "group":
    case "component":
    case "instance":
      return SHAPE_TYPE_RECT;
    case "circle":
    case "ellipse":
      return SHAPE_TYPE_ELLIPSE;
    default:
      return SHAPE_TYPE_OTHER;
  }
}

/** Converts a hex color string to [r, g, b] in [0, 1] range. */
function hexToLinear(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const full = h.length === 3
    ? h.split("").map((c) => c + c).join("")
    : h.slice(0, 6);
  const r = parseInt(full.slice(0, 2), 16) / 255;
  const g = parseInt(full.slice(2, 4), 16) / 255;
  const b = parseInt(full.slice(4, 6), 16) / 255;
  return [r, g, b];
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

export interface PackedShapes {
  /** Flat Float32Array; length = count × SHAPE_ENTRY_F32S */
  data:  Float32Array;
  /** Number of shapes actually packed (≤ MAX_SHAPES). */
  count: number;
}

/**
 * Pack an array of `Shape` objects into the GPU-ready Float32Array.
 *
 * Shapes are written in the order provided (index 0 = bottom of stack).
 * Hidden shapes receive flag bit 0 = 1 so shaders can skip them.
 * Locked shapes receive flag bit 1 = 1.
 */
export function packShapes(shapes: Shape[]): PackedShapes {
  const count = Math.min(shapes.length, MAX_SHAPES);
  const data  = new Float32Array(count * SHAPE_ENTRY_F32S);

  for (let i = 0; i < count; i++) {
    const s   = shapes[i];
    const off = i * SHAPE_ENTRY_F32S;

    // AABB
    data[off + 0] = s.bounds.x;
    data[off + 1] = s.bounds.y;
    data[off + 2] = s.bounds.w;
    data[off + 3] = s.bounds.h;

    // Fill — use first fill or transparent fallback
    if (s.fills.length > 0) {
      const f = s.fills[0];
      if (f.type === "solid") {
        const [r, g, b]     = hexToLinear(f.color);
        data[off + 4]  = r;
        data[off + 5]  = g;
        data[off + 6]  = b;
        data[off + 7]  = f.opacity;
        data[off + 11] = FILL_TYPE_SOLID;
      } else {
        // Gradient fill — colour channels unused by the tile shader when
        // fill_type > 0, but zero-fill for determinism.
        data[off + 4]  = 0;
        data[off + 5]  = 0;
        data[off + 6]  = 0;
        data[off + 7]  = 0;
        data[off + 11] = f.gradient.type === "linear"
          ? FILL_TYPE_LINEAR_GRADIENT
          : FILL_TYPE_RADIAL_GRADIENT;
      }
    } else {
      // No fill — transparent
      data[off + 4]  = 0;
      data[off + 5]  = 0;
      data[off + 6]  = 0;
      data[off + 7]  = 0;
      data[off + 11] = FILL_TYPE_SOLID;
    }

    // Transform
    data[off + 8]  = s.rotation;
    data[off + 9]  = s.opacity;
    data[off + 10] = encodeShapeType(s.type);
    // field 11 = fill_type — already written above.
  }

  return { data, count };
}

/**
 * Upload packed shape data to a pre-existing GPUBuffer.
 * The buffer must have been created with `GPUBufferUsage.STORAGE | COPY_DST`.
 */
export function uploadShapes(
  device: GPUDevice,
  gpuBuffer: GPUBuffer,
  packed: PackedShapes
): void {
  device.queue.writeBuffer(gpuBuffer, 0, packed.data);
}

/**
 * Create a shape storage buffer sized for MAX_SHAPES.
 */
export function createShapeBuffer(device: GPUDevice, label = "logos-shapes"): GPUBuffer {
  return device.createBuffer({
    label,
    size:  MAX_SHAPES * SHAPE_ENTRY_F32S * 4,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Gradient params buffer (P5.2)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Pack gradient positional parameters for all shapes into a Float32Array.
 *
 * The buffer is indexed by instance_index (same as the shape buffer), so
 * each slot corresponds to the shape at the same index in `shapes`.
 * For solid-fill shapes the slot is zeroed (the tile shader won't read it).
 *
 * GradientEntry layout (must match tile.wgsl, 8 × f32 = 32 bytes):
 *   [x0, y0, x1, y1, atlas_v, grad_opacity, _p1, _p2]
 *
 * @param shapes  Packed shape array (same order as `packShapes`).
 * @param atlas   GradientAtlas — provides atlas_v coordinates.
 */
export function packGradientParams(shapes: Shape[], atlas: GradientAtlas): Float32Array {
  const count = Math.min(shapes.length, MAX_SHAPES);
  const data  = new Float32Array(count * GRADIENT_ENTRY_F32S);

  for (let i = 0; i < count; i++) {
    const s = shapes[i];
    if (s.fills.length === 0 || s.fills[0].type !== "gradient") continue;

    const fill = s.fills[0];
    const g    = fill.gradient;
    const off  = i * GRADIENT_ENTRY_F32S;

    // Convert shape-local [0,1] gradient coords to canvas coords.
    const bx = s.bounds.x;
    const by = s.bounds.y;
    const bw = s.bounds.w;
    const bh = s.bounds.h;

    data[off + 0] = bx + g.startX * bw;   // x0
    data[off + 1] = by + g.startY * bh;   // y0
    data[off + 2] = bx + g.endX   * bw;   // x1
    data[off + 3] = by + g.endY   * bh;   // y1

    // Register in the atlas (idempotent) and retrieve atlas V.
    const slot   = atlas.register(fill);
    data[off + 4] = atlas.atlasV(slot);   // atlas_v
    data[off + 5] = fill.opacity;         // grad_opacity
    data[off + 6] = 0;                    // _p1
    data[off + 7] = 0;                    // _p2
  }

  return data;
}

/**
 * Create a gradient params GPU buffer sized for MAX_SHAPES.
 */
export function createGradientParamsBuffer(
  device: GPUDevice,
  label  = "logos-gradient-params",
): GPUBuffer {
  return device.createBuffer({
    label,
    size:  MAX_SHAPES * GRADIENT_ENTRY_F32S * 4,
    usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
  });
}

/**
 * Upload packed gradient params to the GPU buffer.
 */
export function uploadGradientParams(
  device:    GPUDevice,
  gpuBuffer: GPUBuffer,
  data:      Float32Array,
): void {
  device.queue.writeBuffer(gpuBuffer, 0, data);
}
