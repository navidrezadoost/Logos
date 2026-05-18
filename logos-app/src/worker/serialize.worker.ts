/**
 * worker/serialize.worker.ts
 *
 * Serializes the React-shell shape tree into the binary format expected
 * by the Rust/Skia WASM renderer on the main thread.
 *
 * This worker pre-encodes shape records so the main-thread render loop only
 * needs to `postMessage` an ArrayBuffer to the canvas effect — avoiding
 * heavy JSON.stringify in the UI thread.
 *
 * Message protocol
 * ─────────────────
 * IN:
 *   { type: "SERIALIZE"; id: string; payload: SerializeRequest }
 *
 * OUT:
 *   { type: "SERIALIZE_RESULT"; id: string; buffer: ArrayBuffer }   (transferable)
 *   { type: "SERIALIZE_ERROR";  id: string; error: string }
 */

import type { Shape } from "../types/shapes";

export interface SerializeRequest {
  shapes: Shape[];
  /** Viewport width (used by _resize_viewbox) */
  width: number;
  /** Viewport height */
  height: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Binary layout (per-shape record)
// Mirrors the layout expected by module.ts / syncScene()
//
// Header: 4 bytes
//   u32 LE — number of shape records
//
// Per shape record (fixed 96 bytes):
//   [0..15]  u8[16]  UUID bytes  (from uuidToU32x4 → stored as 4×u32 LE)
//   [16]     u8      ShapeType   (0=rect, 1=ellipse, 2=text, 3=path, 4=frame)
//   [17..19] u8[3]   padding
//   [20..35] f32[4]  selrect: x1, y1, x2, y2
//   [36..59] f32[6]  transform: a, b, c, d, e, f
//   [60]     f32     rotation (radians)
//   [64..67] u32     fill ARGB (0xAARRGGBB)
//   [68..95] u8[28]  reserved / future use
//
// ─────────────────────────────────────────────────────────────────────────────

const RECORD_BYTES = 96;
const HEADER_BYTES = 4;

const SHAPE_TYPE_MAP: Record<string, number> = {
  rect: 0,
  ellipse: 1,
  text: 2,
  path: 3,
  frame: 4,
  group: 4, // treat as frame for now
  "svg-raw": 3,
  circle: 1,
  bool: 3,
};

/** Parse a UUID string → four u32 LE words. */
function uuidToWords(uuid: string): [number, number, number, number] {
  const hex = uuid.replace(/-/g, "");
  return [
    parseInt(hex.slice(0, 8), 16),
    parseInt(hex.slice(8, 16), 16),
    parseInt(hex.slice(16, 24), 16),
    parseInt(hex.slice(24, 32), 16),
  ];
}

/** "#rrggbb" + opacity [0-1] → 0xAARRGGBB */
function hexToARGB(hex: string, opacity: number): number {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const a = Math.round(opacity * 255);
  return ((a << 24) | (r << 16) | (g << 8) | b) >>> 0;
}

function serialize(req: SerializeRequest): ArrayBuffer {
  const { shapes, width, height } = req;
  const n = shapes.length;

  // Buffer: 4-byte header (width u16 + height u16) + count u32 + records
  // Simplified: just shape count in header, width/height passed separately
  const buf = new ArrayBuffer(HEADER_BYTES + n * RECORD_BYTES);
  const view = new DataView(buf);

  // Header: shape count (u32 LE)
  view.setUint32(0, n, true);

  for (let i = 0; i < n; i++) {
    const shape = shapes[i];
    const offset = HEADER_BYTES + i * RECORD_BYTES;

    // UUID → 4×u32
    const [w0, w1, w2, w3] = uuidToWords(shape.id);
    view.setUint32(offset + 0, w0, true);
    view.setUint32(offset + 4, w1, true);
    view.setUint32(offset + 8, w2, true);
    view.setUint32(offset + 12, w3, true);

    // ShapeType u8
    view.setUint8(offset + 16, SHAPE_TYPE_MAP[shape.type] ?? 0);

    // selrect: x1, y1, x2, y2
    const { x, y, w, h } = shape.bounds;
    view.setFloat32(offset + 20, x, true);
    view.setFloat32(offset + 24, y, true);
    view.setFloat32(offset + 28, x + w, true);
    view.setFloat32(offset + 32, y + h, true);

    // transform: a, b, c, d, e, f
    const [a, b, c, d, e, f] = shape.transform;
    view.setFloat32(offset + 36, a, true);
    view.setFloat32(offset + 40, b, true);
    view.setFloat32(offset + 44, c, true);
    view.setFloat32(offset + 48, d, true);
    view.setFloat32(offset + 52, e, true);
    view.setFloat32(offset + 56, f, true);

    // rotation (degrees → radians)
    view.setFloat32(offset + 60, (shape.rotation * Math.PI) / 180, true);

    // fill ARGB
    const solid = shape.fills.find((f) => f.type === "solid");
    const argb = solid ? hexToARGB(solid.color, solid.opacity * shape.opacity) : 0xff000000;
    view.setUint32(offset + 64, argb, true);
  }

  return buf;
}

// ─────────────────────────────────────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────────────────────────────────────

self.onmessage = (e: MessageEvent) => {
  const { type, id, payload } = e.data as {
    type: string;
    id: string;
    payload: SerializeRequest;
  };

  if (type !== "SERIALIZE") return;

  try {
    const buffer = serialize(payload);
    // Transfer the ArrayBuffer (zero-copy) using the global postMessage available in Worker scope.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (self.postMessage as (msg: unknown, transfer: Transferable[]) => void)(
      { type: "SERIALIZE_RESULT", id, buffer },
      [buffer]
    );
  } catch (err) {
    self.postMessage({
      type: "SERIALIZE_ERROR",
      id,
      error: err instanceof Error ? err.message : String(err),
    });
  }
};

self.postMessage({ type: "READY" });
