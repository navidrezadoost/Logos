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
import { SHAPE_ENTRY_F32S, MAX_SHAPES } from "./constants";

// ─────────────────────────────────────────────────────────────────────────────
// Shape-type encoding (must match WGSL shader)
// ─────────────────────────────────────────────────────────────────────────────

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
      const f             = s.fills[0];
      const [r, g, b]     = hexToLinear(f.color);
      data[off + 4]  = r;
      data[off + 5]  = g;
      data[off + 6]  = b;
      data[off + 7]  = f.opacity;
    } else {
      // No fill — transparent
      data[off + 4]  = 0;
      data[off + 5]  = 0;
      data[off + 6]  = 0;
      data[off + 7]  = 0;
    }

    // Transform
    data[off + 8]  = s.rotation;
    data[off + 9]  = s.opacity;
    data[off + 10] = encodeShapeType(s.type);

    // Flags: bit0 = hidden, bit1 = locked
    let flags = 0;
    if (s.hidden) flags |= 1;
    if (s.locked) flags |= 2;
    data[off + 11] = flags;
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
