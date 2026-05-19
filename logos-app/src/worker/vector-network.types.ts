/**
 * worker/vector-network.types.ts
 *
 * TypeScript-side types reflecting the JSON protocol of logos-vector-wasm.
 * All shapes match the Rust structs in logos-vector-wasm/src/lib.rs exactly.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Core geometry types
// ─────────────────────────────────────────────────────────────────────────────

/** An anchor (control point) in a VectorNetwork. */
export interface VNAnchor {
  x: number;
  y: number;
  /** handle_in — incoming Bézier control point, absolute coords. */
  hi?: [number, number] | null;
  /** handle_out — outgoing Bézier control point, absolute coords. */
  ho?: [number, number] | null;
}

/** A directed segment (edge) between two anchors. */
export interface VNSegment {
  /** Index of start anchor. */
  s: number;
  /** Index of end anchor. */
  e: number;
  /** First cubic Bézier control point. */
  c1?: [number, number] | null;
  /** Second cubic Bézier control point. */
  c2?: [number, number] | null;
}

/** A serialised VectorNetwork (anchors + segments, no regions). */
export interface VNNetwork {
  anchors: VNAnchor[];
  segments: VNSegment[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Boolean operation
// ─────────────────────────────────────────────────────────────────────────────

/** The four standard set operations on closed regions. */
export type BoolOp = "union" | "intersect" | "subtract" | "exclude";

/** Input for `logos_vn_boolean_op`. */
export interface BoolOpRequest {
  net_a:    VNNetwork;
  net_b:    VNNetwork;
  /** Ordered segment indices (boundary) of region A. */
  region_a: number[];
  /** Ordered segment indices (boundary) of region B. */
  region_b: number[];
  op: BoolOp;
}

/** Successful output of `logos_vn_boolean_op`. */
export interface BoolOpSuccess {
  ok:       true;
  anchors:  VNAnchor[];
  segments: VNSegment[];
  /** Each entry is an ordered list of segment indices forming one output region. */
  regions:  number[][];
}

/** Error output of `logos_vn_boolean_op`. */
export interface BoolOpError {
  ok:    false;
  error: string;
}

export type BoolOpResult = BoolOpSuccess | BoolOpError;

// ─────────────────────────────────────────────────────────────────────────────
// Find regions
// ─────────────────────────────────────────────────────────────────────────────

/** Input for `logos_vn_find_regions`. */
export interface FindRegionsRequest {
  net: VNNetwork;
}

/** Successful output of `logos_vn_find_regions`. */
export interface FindRegionsSuccess {
  ok:      true;
  regions: number[][];
}

export interface FindRegionsError {
  ok:    false;
  error: string;
}

export type FindRegionsResult = FindRegionsSuccess | FindRegionsError;

// ─────────────────────────────────────────────────────────────────────────────
// Worker message protocol
// ─────────────────────────────────────────────────────────────────────────────

export type VectorNetworkMessageIn =
  | { type: "BOOL_OP";      id: string; payload: BoolOpRequest }
  | { type: "FIND_REGIONS"; id: string; payload: FindRegionsRequest };

export type VectorNetworkMessageOut =
  | { type: "BOOL_OP_RESULT";      id: string; result: BoolOpResult }
  | { type: "FIND_REGIONS_RESULT"; id: string; result: FindRegionsResult }
  | { type: "READY" }
  | { type: "ERROR"; id: string; error: string };
