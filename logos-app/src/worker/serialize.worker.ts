/**
 * worker/serialize.worker.ts
 *
 * Serializes the React-shell shape array into 104-byte per-shape binary records
 * matching the Rust `set_shape_base_props()` ABI exactly.
 *
 * Binary layout (matches render-wasm/src/wasm/shapes/base_props.rs):
 *
 * | Offset | Size | Field        | Type                               |
 * |--------|------|--------------|------------------------------------|
 * |  0     |  16  | id           | UUID (4 × u32 LE)                  |
 * | 16     |  16  | parent_id    | UUID (4 × u32 LE, zero if null)    |
 * | 32     |   1  | shape_type   | u8 (Frame=0,Group=1,Bool=2,Rect=3, |
 * |        |      |              |      Path=4,Text=5,Circle=6,SVG=7) |
 * | 33     |   1  | flags        | u8 bit0=clip, bit1=hidden          |
 * | 34     |   1  | blend_mode   | u8 (0 = Normal)                    |
 * | 35     |   1  | constraint_h | u8 (0xFF = None)                   |
 * | 36     |   1  | constraint_v | u8 (0xFF = None)                   |
 * | 37     |   3  | padding      | zeros                               |
 * | 40     |   4  | opacity      | f32 LE                              |
 * | 44     |   4  | rotation     | f32 LE (degrees)                   |
 * | 48     |  24  | transform    | 6 × f32 LE (a,b,c,d,e,f)           |
 * | 72     |  16  | selrect      | 4 × f32 LE (x1,y1,x2,y2)           |
 * | 88     |  16  | corners      | 4 × f32 LE (r1,r2,r3,r4)           |
 * |--------|------|--------------|----------------------------------------|
 * | Total  | 104  |              |                                        |
 *
 * Fills are serialized separately (8 bytes per fill via applySolidFill).
 *
 * Message protocol
 * ─────────────────
 * IN:
 *   { type: "SERIALIZE"; id: string; payload: SerializeRequest }
 *
 * OUT (transferable — zero-copy):
 *   { type: "SERIALIZE_RESULT"; id: string; buffer: ArrayBuffer }
 *   — buffer layout: 4-byte header (shape count u32 LE) + N × 104-byte records.
 *
 *   { type: "SERIALIZE_ERROR"; id: string; error: string }
 */

import type { Shape } from "../types/shapes";

export interface SerializeRequest {
  shapes: Shape[];
  width: number;
  height: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Constants — must stay in sync with Rust RawShapeType enum
// ─────────────────────────────────────────────────────────────────────────────

const RECORD_BYTES = 104;
const HEADER_BYTES = 4; // u32 LE: shape count

const SHAPE_TYPE: Record<string, number> = {
  frame:    0,
  group:    1,
  bool:     2,
  rect:     3,
  path:     4,
  text:     5,
  circle:   6,
  ellipse:  6, // same as circle in Rust enum
  "svg-raw": 7,
};

const FLAG_HIDDEN   = 0x02;
const BLEND_NORMAL  = 0x00;
const CONSTRAINT_NONE = 0xFF;

// Pre-built zero UUID (16 bytes of zeros) — used for null parent_id
const ZERO_UUID_BYTES = new Uint8Array(16);

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Parse a standard UUID string (with dashes) into 4 × u32 LE and write to
 * a DataView at the given byte offset.
 *
 * UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
 * The four u32 words are taken from consecutive 8-hex-char groups after
 * stripping dashes. This matches uuid/get-u32 in ClojureScript and the
 * uuid_from_u32_quartet() Rust helper used by set_shape_base_props.
 */
function writeUUID(view: DataView, offset: number, uuid: string | null): void {
  if (!uuid) {
    // Write 16 zero bytes
    for (let i = 0; i < 16; i++) view.setUint8(offset + i, 0);
    return;
  }
  const hex = uuid.replace(/-/g, "");
  if (hex.length !== 32) {
    for (let i = 0; i < 16; i++) view.setUint8(offset + i, 0);
    return;
  }
  view.setUint32(offset + 0,  parseInt(hex.slice( 0,  8), 16), true);
  view.setUint32(offset + 4,  parseInt(hex.slice( 8, 16), 16), true);
  view.setUint32(offset + 8,  parseInt(hex.slice(16, 24), 16), true);
  view.setUint32(offset + 12, parseInt(hex.slice(24, 32), 16), true);
}

// ─────────────────────────────────────────────────────────────────────────────
// Main serializer
// ─────────────────────────────────────────────────────────────────────────────

function serializeShapes(shapes: Shape[]): ArrayBuffer {
  const n = shapes.length;
  const buf = new ArrayBuffer(HEADER_BYTES + n * RECORD_BYTES);
  const view = new DataView(buf);

  // Header: shape count (u32 LE)
  view.setUint32(0, n, true);

  for (let i = 0; i < n; i++) {
    const shape = shapes[i];
    const base = HEADER_BYTES + i * RECORD_BYTES;

    // [0..16) — id UUID
    writeUUID(view, base + 0, shape.id);

    // [16..32) — parent_id UUID (zero if null)
    writeUUID(view, base + 16, shape.parentId);

    // [32] — shape_type u8
    view.setUint8(base + 32, SHAPE_TYPE[shape.type] ?? SHAPE_TYPE.rect);

    // [33] — flags u8 (bit1=hidden)
    const flags = shape.hidden ? FLAG_HIDDEN : 0;
    view.setUint8(base + 33, flags);

    // [34] — blend_mode u8 (always Normal for now)
    view.setUint8(base + 34, BLEND_NORMAL);

    // [35] — constraint_h u8
    view.setUint8(base + 35, CONSTRAINT_NONE);

    // [36] — constraint_v u8
    view.setUint8(base + 36, CONSTRAINT_NONE);

    // [37..40) — padding (already zero from ArrayBuffer)

    // [40] — opacity f32 LE
    view.setFloat32(base + 40, shape.opacity, true);

    // [44] — rotation f32 LE (degrees — Rust expects degrees, not radians)
    view.setFloat32(base + 44, shape.rotation, true);

    // [48..72) — transform 6 × f32 LE (a,b,c,d,e,f)
    const [a, b, c, d, e, f] = shape.transform;
    view.setFloat32(base + 48, a, true);
    view.setFloat32(base + 52, b, true);
    view.setFloat32(base + 56, c, true);
    view.setFloat32(base + 60, d, true);
    view.setFloat32(base + 64, e, true);
    view.setFloat32(base + 68, f, true);

    // [72..88) — selrect 4 × f32 LE (x1, y1, x2, y2)
    const { x, y, w, h } = shape.bounds;
    view.setFloat32(base + 72, x,     true);
    view.setFloat32(base + 76, y,     true);
    view.setFloat32(base + 80, x + w, true);
    view.setFloat32(base + 84, y + h, true);

    // [88..104) — corners 4 × f32 LE (r1, r2, r3, r4) — zero for non-frames
    view.setFloat32(base + 88,  0, true);
    view.setFloat32(base + 92,  0, true);
    view.setFloat32(base + 96,  0, true);
    view.setFloat32(base + 100, 0, true);
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
    const buffer = serializeShapes(payload.shapes);
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
