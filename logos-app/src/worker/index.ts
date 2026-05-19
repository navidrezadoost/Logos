/**
 * worker/index.ts
 *
 * Worker pool manager — creates, routes messages to, and terminates
 * the three background workers.  Vite's `?worker` suffix causes it to
 * bundle each file as a separate worker module automatically.
 *
 * Usage
 * ─────
 *   import { workerPool } from "./worker";
 *
 *   // Request a layout computation and get back shape patches:
 *   const patches = await workerPool.layout({ nodes, rootId });
 *
 *   // Request snap candidates for a moving shape:
 *   const snapResult = await workerPool.snap({ subject, targets, threshold, ... });
 *
 *   // Pre-serialize shape tree into binary buffer:
 *   const buffer = await workerPool.serialize({ shapes, width, height });
 */

import type { LayoutRequest } from "./layout.worker";
import type { SnapRequest, SnapResult } from "./snap.worker";
import type { SerializeRequest } from "./serialize.worker";
import type { BoolOpRequest, BoolOpResult, FindRegionsRequest, FindRegionsResult } from "./vector-network.types";

// Vite worker imports — bundled as separate JS modules in production.
// The `?worker` query tells Vite to treat them as Worker constructors.
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore — Vite-specific URL import
import LayoutWorker from "./layout.worker?worker";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import SnapWorker from "./snap.worker?worker";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import SerializeWorker from "./serialize.worker?worker";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import VectorNetworkWorker from "./vector-network.worker?worker";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

type InFlight = {
  resolve: (v: unknown) => void;
  reject: (e: unknown) => void;
};

let _msgId = 0;
function nextId(): string {
  return String(++_msgId);
}

/**
 * Wraps a Worker with a promise-based request/response map keyed by message id.
 */
class WorkerClient {
  private worker: Worker;
  private pending = new Map<string, InFlight>();
  private readyResolve!: () => void;
  readonly ready: Promise<void>;

  constructor(WorkerClass: new () => Worker, resultType: string | string[], errorType: string) {
    this.worker = new WorkerClass();
    const resultTypes = Array.isArray(resultType) ? resultType : [resultType];
    this.ready = new Promise<void>((res) => {
      this.readyResolve = res;
    });

    this.worker.onmessage = (e: MessageEvent) => {
      const { type, id, ...rest } = e.data as {
        type: string;
        id?: string;
        [k: string]: unknown;
      };

      if (type === "READY") {
        this.readyResolve();
        return;
      }

      if (!id) return;
      const inFlight = this.pending.get(id);
      if (!inFlight) return;
      this.pending.delete(id);

      if (resultTypes.includes(type)) {
        inFlight.resolve(rest);
      } else if (type === errorType) {
        inFlight.reject(new Error(rest.error as string));
      }
    };

    this.worker.onerror = (e) => {
      console.error("[WorkerClient] error", e);
    };
  }

  post<T>(type: string, payload: unknown, transfer?: Transferable[]): Promise<T> {
    const id = nextId();
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
      });
      if (transfer?.length) {
        this.worker.postMessage({ type, id, payload }, transfer);
      } else {
        this.worker.postMessage({ type, id, payload });
      }
    });
  }

  terminate(): void {
    this.worker.terminate();
    for (const { reject } of this.pending.values()) {
      reject(new Error("Worker terminated"));
    }
    this.pending.clear();
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pool
// ─────────────────────────────────────────────────────────────────────────────

interface LayoutResult {
  patches: Record<string, { x: number; y: number; w: number; h: number }>;
}

interface SerializeResult {
  buffer: ArrayBuffer;
}

class WorkerPool {
  private _layout: WorkerClient | null = null;
  private _snap: WorkerClient | null = null;
  private _serialize: WorkerClient | null = null;
  private _vectorNetwork: WorkerClient | null = null;
  private _initialized = false;

  /** Call once at app startup (e.g. in App.tsx useEffect). */
  init(): void {
    if (this._initialized) return;
    this._initialized = true;
    try {
      this._layout = new WorkerClient(LayoutWorker, "LAYOUT_RESULT", "LAYOUT_ERROR");
      this._snap = new WorkerClient(SnapWorker, "SNAP_RESULT", "SNAP_ERROR");
      this._serialize = new WorkerClient(SerializeWorker, "SERIALIZE_RESULT", "SERIALIZE_ERROR");
      this._vectorNetwork = new WorkerClient(VectorNetworkWorker, ["BOOL_OP_RESULT", "FIND_REGIONS_RESULT"], "ERROR");
    } catch (err) {
      console.error("[WorkerPool] Failed to initialize workers:", err);
    }
  }

  async layout(req: LayoutRequest): Promise<LayoutResult["patches"]> {
    if (!this._layout) throw new Error("Worker pool not initialized");
    const res = await this._layout.post<LayoutResult>("COMPUTE_LAYOUT", req);
    return res.patches;
  }

  async snap(req: SnapRequest): Promise<SnapResult> {
    if (!this._snap) throw new Error("Worker pool not initialized");
    const res = await this._snap.post<{ result: SnapResult }>("SNAP", req);
    return res.result;
  }

  async serialize(req: SerializeRequest): Promise<ArrayBuffer> {
    if (!this._serialize) throw new Error("Worker pool not initialized");
    const res = await this._serialize.post<SerializeResult>("SERIALIZE", req);
    return res.buffer;
  }

  async boolOp(req: BoolOpRequest): Promise<BoolOpResult> {
    if (!this._vectorNetwork) throw new Error("Worker pool not initialized");
    const res = await this._vectorNetwork.post<{ result: BoolOpResult }>("BOOL_OP", req);
    return res.result;
  }

  async findRegions(req: FindRegionsRequest): Promise<FindRegionsResult> {
    if (!this._vectorNetwork) throw new Error("Worker pool not initialized");
    const res = await this._vectorNetwork.post<{ result: FindRegionsResult }>("FIND_REGIONS", req);
    return res.result;
  }

  terminate(): void {
    this._layout?.terminate();
    this._snap?.terminate();
    this._serialize?.terminate();
    this._vectorNetwork?.terminate();
    this._layout = null;
    this._snap = null;
    this._serialize = null;
    this._vectorNetwork = null;
    this._initialized = false;
  }

  get isInitialized(): boolean {
    return this._initialized;
  }
}

/** Singleton worker pool — shared across the whole app. */
export const workerPool = new WorkerPool();
export type { LayoutRequest, SnapRequest, SnapResult, SerializeRequest, BoolOpRequest, BoolOpResult, FindRegionsRequest, FindRegionsResult };
