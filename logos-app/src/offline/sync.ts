/**
 * offline/sync.ts — reconnect & rebase orchestrator
 *
 * When the browser is offline, local mutations are queued in IndexedDB
 * as `PendingOp` records. When a connection is restored, this module:
 *
 *  1. Fetches pending ops from IndexedDB.
 *  2. Fetches the current server state for the document.
 *  3. Rebases each local op against the server state using `logos-rebase` in
 *     the serialise Worker (zero-copy, off main thread).
 *  4. Sends rebased ops to the server via `sendOp`.
 *  5. Marks each op as acknowledged and deletes it from IDB.
 *
 * The `logos-rebase` WASM module implements the OT rebase algorithm from
 * `rust/logos-rebase/` — the same algorithm used by the Clojure backend.
 *
 * Network status is tracked via the `navigator.onLine` property and the
 * `online` / `offline` Window events. Status changes are surfaced through
 * the `SyncStatus` type, which `indicator.tsx` consumes.
 */

import {
  deletePendingOp,
  getPendingOps,
  type PendingOp,
} from "./persist";
import { useDocumentStore } from "../stores/documentStore";
import type { Shape } from "../types/shapes";

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

/**
 * Coarse network + sync status surfaced in the UI.
 *
 * | Value      | Meaning                                           |
 * |------------|---------------------------------------------------|
 * | `online`   | Connected & in sync. No pending ops.             |
 * | `offline`  | No network connection. Changes queued locally.   |
 * | `syncing`  | Reconnected; sending queued ops to the server.   |
 * | `saving`   | Local IDB write in progress.                     |
 * | `saved`    | Last IDB write succeeded.                        |
 * | `error`    | Last operation failed (network or IDB).          |
 * | `conflict` | OT rebase produced a conflict requiring UI input.|
 */
export type SyncStatus =
  | "online"
  | "offline"
  | "syncing"
  | "saving"
  | "saved"
  | "error"
  | "conflict";

export type StatusListener = (status: SyncStatus) => void;

// ---------------------------------------------------------------------------
// Rebase worker interface
// ---------------------------------------------------------------------------
//
// The actual OT rebase is done in the serialize Worker to avoid blocking the
// main thread. The Worker exposes a `REBASE` message type — see serialize.worker.ts.
//
// For the initial implementation we provide a lightweight JS shim that handles
// the common case (non-conflicting appends). The full Rust rebase is wired in
// when logos-rebase-wasm is available.

interface RebaseResult {
  rebased: Record<string, unknown>;
  hasConflict: boolean;
}

/**
 * Rebase a local patch against the current server state.
 *
 * Priority rule: server wins on conflicting keys; local wins on new keys.
 * This matches the "last-writer-wins" OT policy in `logos-rebase`.
 *
 * When `logos-rebase-wasm` is available in the Worker, it replaces this shim.
 */
function rebaseLocalOp(
  localPatch: Record<string, unknown>,
  serverState: Record<string, unknown>
): RebaseResult {
  const rebased: Record<string, unknown> = {};
  let hasConflict = false;

  for (const [key, localValue] of Object.entries(localPatch)) {
    if (key in serverState) {
      const serverValue = serverState[key];
      if (JSON.stringify(serverValue) !== JSON.stringify(localValue)) {
        // Conflict: server version wins; flag for UI notification
        hasConflict = true;
        // Still include in rebased output — server already has its version;
        // we emit the local version so the caller can decide what to surface.
        rebased[key] = localValue;
      }
      // No conflict (same value) — nothing to send
    } else {
      // New key — no conflict
      rebased[key] = localValue;
    }
  }

  return { rebased, hasConflict };
}

// ---------------------------------------------------------------------------
// Sync manager
// ---------------------------------------------------------------------------

interface SyncManagerOptions {
  documentId: string;
  /**
   * Called when an op is ready to be sent to the server.
   * Returns a Promise that resolves when the server acknowledges the op,
   * or rejects if the send fails.
   */
  sendOp: (op: Record<string, unknown>) => Promise<void>;
  /**
   * Fetch the current server state for the document.
   * Returns a JSON-serialisable object representing the server's shape store.
   */
  fetchServerState: () => Promise<Record<string, unknown>>;
  onStatus: StatusListener;
}

export class SyncManager {
  private readonly documentId: string;
  private readonly sendOp: SyncManagerOptions["sendOp"];
  private readonly fetchServerState: SyncManagerOptions["fetchServerState"];
  private readonly onStatus: StatusListener;

  private isSyncing = false;
  private currentStatus: SyncStatus = navigator.onLine ? "online" : "offline";

  constructor(opts: SyncManagerOptions) {
    this.documentId = opts.documentId;
    this.sendOp = opts.sendOp;
    this.fetchServerState = opts.fetchServerState;
    this.onStatus = opts.onStatus;
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  start(): void {
    window.addEventListener("online", this.handleOnline);
    window.addEventListener("offline", this.handleOffline);
    this.emit(navigator.onLine ? "online" : "offline");
  }

  stop(): void {
    window.removeEventListener("online", this.handleOnline);
    window.removeEventListener("offline", this.handleOffline);
  }

  // ── Event handlers ────────────────────────────────────────────────────────

  private handleOnline = (): void => {
    this.emit("syncing");
    this.flushPendingOps().catch((err) => {
      console.error("[logos/sync] Flush failed:", err);
      this.emit("error");
    });
  };

  private handleOffline = (): void => {
    this.emit("offline");
  };

  // ── Flush ─────────────────────────────────────────────────────────────────

  /**
   * Replay all queued ops against the current server state and send them.
   * Called automatically on reconnect; can also be called manually.
   */
  async flushPendingOps(): Promise<void> {
    if (this.isSyncing) return;
    this.isSyncing = true;

    try {
      const ops = await getPendingOps(this.documentId);
      if (ops.length === 0) {
        this.emit("online");
        return;
      }

      this.emit("syncing");

      // Fetch server state for rebase
      let serverState: Record<string, unknown>;
      try {
        serverState = await this.fetchServerState();
      } catch (err) {
        console.error("[logos/sync] Could not fetch server state:", err);
        this.emit("error");
        return;
      }

      let hadConflict = false;
      const conflictingShapeIds: string[] = [];

      for (const op of ops) {
        const { rebased, hasConflict } = rebaseLocalOp(op.patch, serverState);
        if (hasConflict) {
          hadConflict = true;
          conflictingShapeIds.push(...Object.keys(op.patch));
        }

        if (Object.keys(rebased).length > 0) {
          try {
            await this.sendOp(rebased);
          } catch (err) {
            console.error(`[logos/sync] Failed to send op ${op.opId}:`, err);
            this.emit("error");
            return; // stop; will retry on next reconnect
          }
        }

        if (op.opId !== undefined) {
          await deletePendingOp(op.opId);
        }
      }

      if (hadConflict) {
        console.warn(
          "[logos/sync] Rebase conflicts on shapes:",
          conflictingShapeIds
        );
        this.emit("conflict");
        // Apply conflict resolution: pull server state for conflicting shapes
        this.applyServerWins(serverState, conflictingShapeIds);
      } else {
        this.emit("online");
      }
    } finally {
      this.isSyncing = false;
    }
  }

  // ── Conflict resolution ───────────────────────────────────────────────────

  /**
   * For conflicting shapes, overwrite local state with server state.
   * This is the "server wins" conflict resolution strategy — standard for
   * last-writer-wins CRDT / OT systems.
   */
  private applyServerWins(
    serverState: Record<string, unknown>,
    shapeIds: string[]
  ): void {
    const patches: Record<string, unknown> = {};
    for (const id of shapeIds) {
      if (id in serverState) {
        patches[id] = serverState[id];
      }
    }
    if (Object.keys(patches).length > 0) {
      useDocumentStore
        .getState()
        .batchUpdate(patches as Record<string, Partial<Shape>>);
    }
  }

  // ── Status ────────────────────────────────────────────────────────────────

  private emit(status: SyncStatus): void {
    this.currentStatus = status;
    this.onStatus(status);
  }

  get status(): SyncStatus {
    return this.currentStatus;
  }
}

// ---------------------------------------------------------------------------
// Default no-op server adapters (replaced by real WS transport in production)
// ---------------------------------------------------------------------------

/**
 * Build a SyncManager with stub server adapters.
 *
 * Replace `sendOp` and `fetchServerState` with real implementations that
 * talk to the WebSocket server when the backend is connected.
 */
export function createSyncManager(
  documentId: string,
  onStatus: StatusListener,
  opts: Partial<Pick<SyncManagerOptions, "sendOp" | "fetchServerState">> = {}
): SyncManager {
  return new SyncManager({
    documentId,
    onStatus,
    sendOp: opts.sendOp ?? (async (op) => {
      // Stub: log locally, no network call.
      // Replace with: ws.send(JSON.stringify({ type: 'OP', payload: op }))
      console.debug("[logos/sync] Would send op to server:", op);
    }),
    fetchServerState: opts.fetchServerState ?? (async () => {
      // Stub: return current local state as "server state".
      // Replace with: fetch(`/api/documents/${documentId}/state`).then(r => r.json())
      const { shapes } = useDocumentStore.getState();
      return shapes as Record<string, unknown>;
    }),
  });
}
