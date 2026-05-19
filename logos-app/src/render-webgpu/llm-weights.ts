/**
 * render-webgpu/llm-weights.ts
 *
 * Phase 5.5 — Local LLM: Model weight loading + IndexedDB persistence.
 *
 * Binary weight format (.bin):
 *
 *   Offset  Size  Field
 *   0       4     Magic:       0x4C4C4D57  ("LLMW")
 *   4       2     Version:     1
 *   6       2     Num layers:  n_layers
 *   8       2     d_model:     e.g. 512
 *   10      2     d_ff:        e.g. 2048
 *   12      2     n_heads:     e.g. 8
 *   14      2     vocab_size:  e.g. 8192
 *   16      2     max_seq:     e.g. 256
 *   18      2     dtype:       0=f16  1=f32  2=int8
 *   20      4     n_tensors:   total tensor count
 *   24      8     (reserved)
 *   32      ...   Tensor descriptors (each 64 bytes):
 *                   0  name[40]  null-padded ASCII
 *                   40 shape[4]  u32 × 4  (unused dims = 0)
 *                   56 offset    u64  byte offset from start of data section
 *   32 + n_tensors*64  ...  Tensor data (packed, dtype-encoded)
 *
 * IndexedDB caching:
 *   Database: "logos-ai-cache"
 *   Object store: "models"
 *   Key: model URL string
 *   Value: { url, version, data: ArrayBuffer, timestamp }
 *
 * The weight file for the Logos AI model is expected to be placed at:
 *   /models/logos-ai-sm.bin   (~40 MB)
 *
 * This file is NOT bundled with the app.  It is downloaded on first use and
 * cached in IndexedDB (P4.5 offline persistence continues to apply here).
 */

// ─────────────────────────────────────────────────────────────────────────────
// Model config (read from binary header)
// ─────────────────────────────────────────────────────────────────────────────

export interface ModelConfig {
  nLayers:   number;
  dModel:    number;
  dFF:       number;
  nHeads:    number;
  vocabSize: number;
  maxSeq:    number;
  dtype:     "f16" | "f32" | "int8";
}

// ─────────────────────────────────────────────────────────────────────────────
// TensorMap — name → Float32Array view
// ─────────────────────────────────────────────────────────────────────────────

export type TensorMap = ReadonlyMap<string, Float32Array>;

// ─────────────────────────────────────────────────────────────────────────────
// Progress callback
// ─────────────────────────────────────────────────────────────────────────────

export type ProgressCallback = (loaded: number, total: number, phase: string) => void;

// ─────────────────────────────────────────────────────────────────────────────
// IndexedDB cache
// ─────────────────────────────────────────────────────────────────────────────

const IDB_DB_NAME    = "logos-ai-cache";
const IDB_STORE_NAME = "models";
const IDB_VERSION    = 1;
const BINARY_MAGIC   = 0x4C4C4D57; // "LLMW"

interface CacheEntry {
  url:       string;
  version:   number;
  data:      ArrayBuffer;
  timestamp: number;
}

function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(IDB_DB_NAME, IDB_VERSION);
    req.onupgradeneeded = (e) => {
      const db = (e.target as IDBOpenDBRequest).result;
      if (!db.objectStoreNames.contains(IDB_STORE_NAME)) {
        db.createObjectStore(IDB_STORE_NAME, { keyPath: "url" });
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror   = () => reject(req.error);
  });
}

async function cacheGet(url: string): Promise<CacheEntry | null> {
  try {
    const db = await openDB();
    return new Promise((resolve, reject) => {
      const tx  = db.transaction(IDB_STORE_NAME, "readonly");
      const req = tx.objectStore(IDB_STORE_NAME).get(url);
      req.onsuccess = () => resolve((req.result as CacheEntry) ?? null);
      req.onerror   = () => reject(req.error);
    });
  } catch {
    return null;
  }
}

async function cachePut(entry: CacheEntry): Promise<void> {
  try {
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx  = db.transaction(IDB_STORE_NAME, "readwrite");
      const req = tx.objectStore(IDB_STORE_NAME).put(entry);
      req.onsuccess = () => resolve();
      req.onerror   = () => reject(req.error);
    });
  } catch {
    // Silently ignore — cache miss is acceptable.
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Binary header parsing
// ─────────────────────────────────────────────────────────────────────────────

const HEADER_BYTES     = 32;
const DESCRIPTOR_BYTES = 64;

function parseHeader(buf: ArrayBuffer): { config: ModelConfig; nTensors: number } {
  const dv = new DataView(buf);
  const magic = dv.getUint32(0, false);
  if (magic !== BINARY_MAGIC) {
    throw new Error(`LLM weight file: invalid magic 0x${magic.toString(16)}, expected 0x4C4C4D57`);
  }
  const version  = dv.getUint16(4, true);
  if (version !== 1) throw new Error(`LLM weight file: unsupported version ${version}`);

  const dtypeRaw = dv.getUint16(18, true);
  const dtype: ModelConfig["dtype"] = dtypeRaw === 0 ? "f16" : dtypeRaw === 1 ? "f32" : "int8";

  return {
    config: {
      nLayers:   dv.getUint16(6,  true),
      dModel:    dv.getUint16(8,  true),
      dFF:       dv.getUint16(10, true),
      nHeads:    dv.getUint16(12, true),
      vocabSize: dv.getUint16(14, true),
      maxSeq:    dv.getUint16(16, true),
      dtype,
    },
    nTensors: dv.getUint32(20, true),
  };
}

function parseTensors(
  buf: ArrayBuffer,
  nTensors: number,
  dtype: ModelConfig["dtype"],
): TensorMap {
  const descStart  = HEADER_BYTES;
  const dataStart  = descStart + nTensors * DESCRIPTOR_BYTES;
  const dv         = new DataView(buf);
  const nameBytes  = new Uint8Array(buf);
  const out        = new Map<string, Float32Array>();

  for (let i = 0; i < nTensors; i++) {
    const base = descStart + i * DESCRIPTOR_BYTES;

    // Name: 40-byte null-padded ASCII.
    let nameEnd = base;
    while (nameEnd < base + 40 && nameBytes[nameEnd] !== 0) nameEnd++;
    const name = new TextDecoder("ascii").decode(nameBytes.subarray(base, nameEnd));

    // Shape.
    const dims: number[] = [];
    for (let d = 0; d < 4; d++) {
      const n = dv.getUint32(base + 40 + d * 4, true);
      if (n > 0) dims.push(n);
    }
    const numel = dims.reduce((a, b) => a * b, 1);

    // Byte offset into data section.
    const lo     = dv.getUint32(base + 56, true);
    const hi     = dv.getUint32(base + 60, true);
    const offset = dataStart + lo + hi * 0x100000000;

    // Convert to Float32Array.
    const f32 = dtype === "f32"
      ? new Float32Array(buf, offset, numel)
      : dtype === "f16"
      ? float16ToFloat32(new Uint16Array(buf, offset, numel))
      : int8ToFloat32(new Int8Array(buf, offset, numel));

    out.set(name, f32);
  }
  return out;
}

// ─────────────────────────────────────────────────────────────────────────────
// dtype conversions
// ─────────────────────────────────────────────────────────────────────────────

function float16ToFloat32(src: Uint16Array): Float32Array {
  const dst = new Float32Array(src.length);
  for (let i = 0; i < src.length; i++) {
    const h = src[i];
    const sign   = (h >> 15) & 1;
    const exp    = (h >> 10) & 0x1f;
    const mant   = h & 0x3ff;
    if (exp === 0) {
      dst[i] = (sign ? -1 : 1) * Math.pow(2, -14) * (mant / 1024);
    } else if (exp === 31) {
      dst[i] = mant ? NaN : (sign ? -Infinity : Infinity);
    } else {
      dst[i] = (sign ? -1 : 1) * Math.pow(2, exp - 15) * (1 + mant / 1024);
    }
  }
  return dst;
}

function int8ToFloat32(src: Int8Array): Float32Array {
  // Symmetric int8 quantization: scale = 1 / 127
  const dst = new Float32Array(src.length);
  for (let i = 0; i < src.length; i++) {
    dst[i] = src[i] / 127.0;
  }
  return dst;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

export interface LoadedWeights {
  config:  ModelConfig;
  tensors: TensorMap;
}

/**
 * Load model weights from `url`, using IndexedDB as a persistent cache.
 *
 * The file is only downloaded once; subsequent calls return the cached copy
 * instantly.  Pass `forceRefresh = true` to bypass the cache.
 *
 * @param url         URL to the .bin weight file. Default: "/models/logos-ai-sm.bin".
 * @param onProgress  Optional progress callback (loaded bytes, total bytes, phase).
 * @param forceRefresh  If true, re-download even if cached.
 * @param signal      AbortSignal for cancellation.
 */
export async function loadWeights(
  url = "/models/logos-ai-sm.bin",
  onProgress?: ProgressCallback,
  forceRefresh = false,
  signal?: AbortSignal,
): Promise<LoadedWeights> {
  // ── Check cache ────────────────────────────────────────────────────────────
  if (!forceRefresh) {
    onProgress?.(0, 0, "Checking cache…");
    const cached = await cacheGet(url);
    if (cached) {
      onProgress?.(1, 1, "Cache hit");
      const { config, nTensors } = parseHeader(cached.data);
      const tensors = parseTensors(cached.data, nTensors, config.dtype);
      return { config, tensors };
    }
  }

  // ── Download ───────────────────────────────────────────────────────────────
  onProgress?.(0, 0, "Downloading model…");
  const response = await fetch(url, { signal });
  if (!response.ok) {
    throw new Error(`Failed to download model weights: HTTP ${response.status} from ${url}`);
  }

  const total  = Number(response.headers.get("content-length") ?? 0);
  const reader = response.body!.getReader();
  const chunks: Uint8Array[] = [];
  let loaded = 0;

  while (true) {
    // eslint-disable-next-line no-await-in-loop
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    onProgress?.(loaded, total, "Downloading…");
  }

  // Concatenate chunks.
  const buf = new ArrayBuffer(loaded);
  const u8  = new Uint8Array(buf);
  let off   = 0;
  for (const chunk of chunks) { u8.set(chunk, off); off += chunk.length; }

  // ── Cache ──────────────────────────────────────────────────────────────────
  onProgress?.(loaded, total, "Caching…");
  await cachePut({ url, version: 1, data: buf, timestamp: Date.now() });

  // ── Parse ──────────────────────────────────────────────────────────────────
  onProgress?.(loaded, total, "Parsing weights…");
  const { config, nTensors } = parseHeader(buf);
  const tensors = parseTensors(buf, nTensors, config.dtype);
  return { config, tensors };
}

/**
 * Evict a cached model from IndexedDB.
 */
export async function evictModel(url = "/models/logos-ai-sm.bin"): Promise<void> {
  try {
    const db = await openDB();
    await new Promise<void>((resolve, reject) => {
      const tx  = db.transaction(IDB_STORE_NAME, "readwrite");
      const req = tx.objectStore(IDB_STORE_NAME).delete(url);
      req.onsuccess = () => resolve();
      req.onerror   = () => reject(req.error);
    });
  } catch {
    // Ignore.
  }
}

/**
 * Return true if a weight file is currently cached locally.
 */
export async function isModelCached(url = "/models/logos-ai-sm.bin"): Promise<boolean> {
  const entry = await cacheGet(url);
  return entry !== null;
}
