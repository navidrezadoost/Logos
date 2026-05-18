/**
 * types/shapes.ts
 *
 * Thin projection of shape data used by the React shell + Zustand stores.
 * The full geometry lives in the Rust scene graph; this is only what the
 * UI needs to render controls, layers panel, inspector, etc.
 */

export type ShapeType = "frame" | "rect" | "circle" | "ellipse" | "path" | "text" | "group" | "bool" | "svg-raw";

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A 6-element affine transform matrix [a, b, c, d, e, f] (CSS matrix order). */
export type Transform = [number, number, number, number, number, number];

export const IDENTITY_TRANSFORM: Transform = [1, 0, 0, 1, 0, 0];

/** Solid fill. */
export interface SolidFill {
  type: "solid";
  /** Hex color, e.g. "#0000ff" */
  color: string;
  opacity: number;
}

export type Fill = SolidFill;

export interface Shape {
  /** UUID string (maps to the u32×4 the Rust engine uses). */
  id: string;
  type: ShapeType;
  name: string;
  /** Bounding box in canvas-local coordinates. */
  bounds: Rect;
  transform: Transform;
  rotation: number;
  fills: Fill[];
  opacity: number;
  hidden: boolean;
  locked: boolean;
  /** ID of parent shape/frame, or null for top-level shapes. */
  parentId: string | null;
  /** Ordered child IDs (for frames/groups). */
  children: string[];
}

/** Minimal shape factory. */
export function createRect(
  id: string,
  name: string,
  bounds: Rect,
  fill = "#0000ff"
): Shape {
  return {
    id,
    type: "rect",
    name,
    bounds,
    transform: IDENTITY_TRANSFORM,
    rotation: 0,
    fills: [{ type: "solid", color: fill, opacity: 1 }],
    opacity: 1,
    hidden: false,
    locked: false,
    parentId: null,
    children: [],
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// UUID helpers — map string UUIDs to the four u32 words the Rust engine expects
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Parse a UUID string (with or without dashes) into four little-endian u32s.
 * The Rust engine receives shape IDs as (a, b, c, d).
 */
export function uuidToU32x4(uuid: string): [number, number, number, number] {
  const hex = uuid.replace(/-/g, "");
  const a = parseInt(hex.slice(0, 8), 16) >>> 0;
  const b = parseInt(hex.slice(8, 16), 16) >>> 0;
  const c = parseInt(hex.slice(16, 24), 16) >>> 0;
  const d = parseInt(hex.slice(24, 32), 16) >>> 0;
  return [a, b, c, d];
}

/** Convert a hex colour string (#rrggbb) + opacity [0,1] to 0xAARRGGBB u32. */
export function hexToARGB(hex: string, opacity = 1): number {
  const rgb = parseInt(hex.replace("#", ""), 16);
  const a = Math.round(opacity * 255) & 0xff;
  return ((a << 24) | (rgb & 0xffffff)) >>> 0;
}
