/**
 * worker/vector-network.worker.ts
 *
 * Vector-network boolean operations and region detection via `logos-vector-wasm`.
 * Runs entirely off the main thread.
 *
 * Message protocol
 * ─────────────────
 * IN:
 *   { type: "BOOL_OP";      id: string; payload: BoolOpRequest }
 *   { type: "FIND_REGIONS"; id: string; payload: FindRegionsRequest }
 *
 * OUT:
 *   { type: "BOOL_OP_RESULT";      id: string; result: BoolOpResult }
 *   { type: "FIND_REGIONS_RESULT"; id: string; result: FindRegionsResult }
 *   { type: "READY" }
 *   { type: "ERROR"; id: string; error: string }
 *
 * WASM memory protocol (mirrors logos-layout-wasm):
 *   1. Call `logos_vn_alloc(len)` → get ptr.
 *   2. Write UTF-8 JSON into memory at ptr.
 *   3. Call `logos_vn_<op>(ptr, len)` → get outLen.
 *   4. Call `logos_vn_output_ptr()` → get outPtr; read outLen bytes.
 *   5. Call `logos_vn_free_input(ptr, len)` + `logos_vn_free_output()`.
 */

import type {
  BoolOpRequest, BoolOpResult,
  FindRegionsRequest, FindRegionsResult,
  VectorNetworkMessageIn,
} from "./vector-network.types";

// ─────────────────────────────────────────────────────────────────────────────
// WASM bootstrap
// ─────────────────────────────────────────────────────────────────────────────

/** Raw WASM exports — filled after the module loads. */
interface VnWasmExports {
  memory: WebAssembly.Memory;
  logos_vn_alloc:        (len: number) => number;
  logos_vn_free_input:   (ptr: number, len: number) => void;
  logos_vn_output_ptr:   () => number;
  logos_vn_free_output:  () => void;
  logos_vn_boolean_op:   (ptr: number, len: number) => number;
  logos_vn_find_regions: (ptr: number, len: number) => number;
}

let wasm: VnWasmExports | null = null;
const enc = new TextEncoder();
const dec = new TextDecoder();

// Injected by Vite at build time; falls back to the conventional path at runtime.
declare const __LOGOS_VECTOR_WASM__: string | undefined;
const WASM_URL: string =
  typeof __LOGOS_VECTOR_WASM__ !== "undefined"
    ? __LOGOS_VECTOR_WASM__
    : "/js/logos_vector_wasm.wasm";

async function initWasm(): Promise<void> {
  try {
    // Fetch and instantiate the raw C-ABI WASM binary directly.
    // The exports are the #[no_mangle] extern "C" functions — no wasm-bindgen glue needed.
    const result = await WebAssembly.instantiateStreaming(fetch(WASM_URL));
    wasm = result.instance.exports as unknown as VnWasmExports;
    console.log("[vector-network.worker] logos-vector-wasm loaded ✓");
  } catch (err) {
    console.warn(
      "[vector-network.worker] WASM init failed — using TS fallback",
      err
    );
  }
  self.postMessage({ type: "READY" });
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM call helper
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Encode `payload` as JSON, hand it to `fn_name` in WASM, and decode the
 * JSON result. Handles the full alloc / write / call / read / free cycle.
 */
function callWasm<T>(
  fnName: "logos_vn_boolean_op" | "logos_vn_find_regions",
  payload: unknown
): T {
  if (!wasm) throw new Error("WASM not loaded");

  const json = JSON.stringify(payload);
  const bytes = enc.encode(json);
  const len = bytes.length;

  // 1. Allocate
  const ptr = wasm.logos_vn_alloc(len);

  // 2. Write into WASM memory
  const mem = new Uint8Array(wasm.memory.buffer);
  mem.set(bytes, ptr);

  // 3. Call operation
  const outLen = wasm[fnName](ptr, len);

  // 4. Read output
  const outPtr = wasm.logos_vn_output_ptr();
  // Re-read memory in case WASM grew it
  const mem2 = new Uint8Array(wasm.memory.buffer);
  const outBytes = mem2.slice(outPtr, outPtr + outLen);
  const result = JSON.parse(dec.decode(outBytes)) as T;

  // 5. Free
  wasm.logos_vn_free_input(ptr, len);
  wasm.logos_vn_free_output();

  return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure-TS fallback (geometry primitives only — no Bézier clipping)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Fallback union: just return both networks' boundaries as separate regions.
 * This keeps the UI working during development before the WASM is compiled.
 */
function tsFallbackBoolOp(req: BoolOpRequest): BoolOpResult {
  const net = {
    anchors:  [...req.net_a.anchors, ...req.net_b.anchors],
    segments: [
      ...req.net_a.segments,
      ...req.net_b.segments.map(s => ({
        ...s,
        s: s.s + req.net_a.anchors.length,
        e: s.e + req.net_a.anchors.length,
      })),
    ],
  };
  const regionB = req.net_b.segments.map((_, i) => req.net_a.segments.length + i);
  const regions: number[][] = [];
  if (req.op === "union" || req.op === "exclude") {
    regions.push(req.region_a, regionB);
  } else if (req.op === "intersect") {
    // Approximate: return region_a as-is (empty until WASM loads)
    regions.push(req.region_a);
  } else if (req.op === "subtract") {
    regions.push(req.region_a);
  }
  return { ok: true, anchors: net.anchors, segments: net.segments, regions };
}

function tsFallbackFindRegions(req: FindRegionsRequest): FindRegionsResult {
  // Trivial: treat all segments as one boundary
  const allSegs = req.net.segments.map((_, i) => i);
  return { ok: true, regions: allSegs.length > 0 ? [allSegs] : [] };
}

// ─────────────────────────────────────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────────────────────────────────────

self.onmessage = (e: MessageEvent) => {
  const msg = e.data as VectorNetworkMessageIn;

  try {
    if (msg.type === "BOOL_OP") {
      const result: BoolOpResult = wasm
        ? callWasm<BoolOpResult>("logos_vn_boolean_op", msg.payload)
        : tsFallbackBoolOp(msg.payload);
      self.postMessage({ type: "BOOL_OP_RESULT", id: msg.id, result });

    } else if (msg.type === "FIND_REGIONS") {
      const result: FindRegionsResult = wasm
        ? callWasm<FindRegionsResult>("logos_vn_find_regions", msg.payload)
        : tsFallbackFindRegions(msg.payload);
      self.postMessage({ type: "FIND_REGIONS_RESULT", id: msg.id, result });

    } else {
      // narrow-escape: tell the caller we didn't understand the message
      self.postMessage({
        type: "ERROR",
        id: (msg as VectorNetworkMessageIn & { id: string }).id ?? "?",
        error: `Unknown message type: ${(msg as { type: string }).type}`,
      });
    }
  } catch (err) {
    self.postMessage({
      type: "ERROR",
      id: (msg as VectorNetworkMessageIn & { id: string }).id ?? "?",
      error: err instanceof Error ? err.message : String(err),
    });
  }
};

// Boot
initWasm();
