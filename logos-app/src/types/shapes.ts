/**
 * types/shapes.ts
 *
 * Thin projection of shape data used by the React shell + Zustand stores.
 * The full geometry lives in the Rust scene graph; this is only what the
 * UI needs to render controls, layers panel, inspector, etc.
 */

export type ShapeType = "frame" | "rect" | "circle" | "ellipse" | "path" | "text" | "group" | "bool" | "svg-raw" | "vector-network";

// ─────────────────────────────────────────────────────────────────────────────
// Vector network geometry (mirrors rust/logos-vector-wasm JSON schema)
// ─────────────────────────────────────────────────────────────────────────────

/** One anchor point in a vector network, with optional Bézier handles. */
export interface VNAnchor {
  x: number;
  y: number;
  /** Handle-in control point [dx, dy] relative to anchor. */
  hi?: [number, number] | null;
  /** Handle-out control point [dx, dy] relative to anchor. */
  ho?: [number, number] | null;
}

/** One segment in a vector network (cubic Bézier or straight line). */
export interface VNSegment {
  /** Start anchor index. */
  s: number;
  /** End anchor index. */
  e: number;
  /** First Bézier control point (absolute coords), or omit for straight line. */
  c1?: [number, number] | null;
  /** Second Bézier control point (absolute coords). */
  c2?: [number, number] | null;
}

/**
 * A closed region produced by `logos_vn_find_regions` —
 * an ordered list of segment indices that bound the region.
 */
export type VNRegion = number[];

// ---------------------------------------------------------------------------
// Variable font support
// ---------------------------------------------------------------------------

/**
 * A single OpenType variable font axis override.
 *
 * `tag` is the 4-character ASCII axis identifier from the OpenType spec
 * (e.g. `"wght"`, `"wdth"`, `"slnt"`, `"opsz"`, `"ital"`).
 *
 * `value` is a number within the axis range declared by the font.
 * The render-wasm receives these and converts them to
 * `SkFontArguments::VariationPosition::Coordinate` entries.
 */
export interface FontVariationAxis {
  /** 4-character OpenType axis tag, e.g. "wght" */
  tag: string;
  /** Axis value within the font-defined range, e.g. 750 for weight */
  value: number;
  /** Human-readable axis name from the font's fvar table, e.g. "Weight" */
  name?: string;
  /** Minimum value declared by the font for this axis */
  min?: number;
  /** Maximum value declared by the font for this axis */
  max?: number;
  /** Default value declared by the font for this axis */
  default?: number;
}

/**
 * Serialised font-variation-settings for a text shape or typography style.
 * Matches the CSS `font-variation-settings` property format and the
 * ClojureScript `schema:font-variation-settings` Malli schema.
 *
 * Key = 4-char axis tag, value = axis value.
 * Example: `{ wght: 750, wdth: 100 }`
 */
export type FontVariationSettings = Record<string, number>;

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
  /**
   * Variable font axis overrides for text shapes.
   * Undefined / absent for non-text shapes and static fonts.
   * Transmitted to render-wasm as SkFontArguments variation coordinates.
   */
  fontVariationSettings?: FontVariationSettings;

  // ── Vector network fields (present only when type === "vector-network") ────
  /** Anchor points of the vector network. */
  vnAnchors?: VNAnchor[];
  /** Segments connecting anchors. */
  vnSegments?: VNSegment[];
  /** Closed regions (each is an ordered list of segment indices). */
  vnRegions?: VNRegion[];
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

// ─────────────────────────────────────────────────────────────────────────────
// Vector network factory
// ─────────────────────────────────────────────────────────────────────────────

/** Build a bounding box from a set of anchor points. */
function anchorsBounds(anchors: VNAnchor[]): Rect {
  if (anchors.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const a of anchors) {
    if (a.x < minX) minX = a.x;
    if (a.y < minY) minY = a.y;
    if (a.x > maxX) maxX = a.x;
    if (a.y > maxY) maxY = a.y;
  }
  return { x: minX, y: minY, w: Math.max(1, maxX - minX), h: Math.max(1, maxY - minY) };
}

/** Minimal vector-network shape factory. */
export function createVectorNetwork(
  id: string,
  name: string,
  anchors: VNAnchor[],
  segments: VNSegment[],
  regions: VNRegion[] = [],
  fill = "#6c9ef8"
): Shape {
  return {
    id,
    type: "vector-network",
    name,
    bounds: anchorsBounds(anchors),
    transform: IDENTITY_TRANSFORM,
    rotation: 0,
    fills: [{ type: "solid", color: fill, opacity: 1 }],
    opacity: 1,
    hidden: false,
    locked: false,
    parentId: null,
    children: [],
    vnAnchors: anchors,
    vnSegments: segments,
    vnRegions: regions,
  };
}
