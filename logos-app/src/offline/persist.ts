/**
 * offline/persist.ts — IndexedDB persistence layer
 *
 * Subscribes to `documentStore` mutations and durably writes the document
 * state to IndexedDB in the background, enabling offline / local-first mode.
 *
 * Database layout:
 *   DB name:  logos-offline
 *   version:  1
 *
 *   store: "snapshots"
 *     key:   string (documentId)
 *     value: PersistedDocument
 *
 *   store: "pending-ops"
 *     key:   autoIncrement (opId)
 *     value: PendingOp
 *
 * Usage:
 *   import { initPersistence, loadPersistedDocument } from './offline/persist';
 *   await loadPersistedDocument();   // call before React tree mounts
 *   initPersistence();               // call once inside App (subscribes to store)
 */

import { useDocumentStore } from "../stores/documentStore";
import type { DocumentIdentitySource } from "../stores/documentStore";
import type { SyncStatus } from "./sync";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DB_NAME = "logos-offline";
const DB_VERSION = 1;
const STORE_SNAPSHOTS = "snapshots";
const STORE_PENDING_OPS = "pending-ops";
/** Debounce: only write to IDB at most once per this many ms */
const WRITE_DEBOUNCE_MS = 500;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PersistedDocument {
  /** Logical document ID (e.g. project-file UUID from the backend) */
  documentId: string;
  /** ISO-8601 timestamp of the last write */
  savedAt: string;
  /**
   * A JSON-serialisable snapshot of the Zustand documentStore state.
   * Only the fields needed for restore are persisted.
   */
  snapshot: DocumentSnapshot;
}

export interface DocumentSnapshot {
  /** Present only for snapshots created or verified after the identity contract. */
  documentId?: string | null;
  identitySource?: DocumentIdentitySource;
  pages: Record<string, { id: string; name: string; rootShapeIds: string[] }>;
  pageOrder: string[];
  currentPageId: string;
  shapes: Record<string, unknown>;
}

/**
 * A single local mutation that has not yet been acknowledged by the server.
 *
 * These are collected during offline periods and replayed (with OT rebase)
 * when the connection is restored — see `offline/sync.ts`.
 */
export interface PendingOp {
  opId?: number; // autoIncrement — assigned by IDB
  documentId: string;
  /** ISO-8601 timestamp when the op was generated */
  createdAt: string;
  /** The full Zustand diff that produced this op */
  patch: Record<string, unknown>;
  /** Sequential clock used for OT ordering */
  localClock: number;
}

// ---------------------------------------------------------------------------
// IDB helpers
// ---------------------------------------------------------------------------

let dbPromise: Promise<IDBDatabase> | null = null;

function openDb(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;

  dbPromise = new Promise<IDBDatabase>((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);

    req.onupgradeneeded = (evt) => {
      const db = (evt.target as IDBOpenDBRequest).result;
      if (!db.objectStoreNames.contains(STORE_SNAPSHOTS)) {
        db.createObjectStore(STORE_SNAPSHOTS, { keyPath: "documentId" });
      }
      if (!db.objectStoreNames.contains(STORE_PENDING_OPS)) {
        const ops = db.createObjectStore(STORE_PENDING_OPS, {
          keyPath: "opId",
          autoIncrement: true,
        });
        ops.createIndex("by-document", "documentId", { unique: false });
      }
    };

    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });

  return dbPromise;
}

function idbPut<T>(storeName: string, value: T): Promise<void> {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        const tx = db.transaction(storeName, "readwrite");
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error ?? new Error("LOGOS_IDB_WRITE_FAILED"));
        tx.onabort = () => reject(tx.error ?? new Error("LOGOS_IDB_WRITE_ABORTED"));
        try {
          const request = tx.objectStore(storeName).put(value);
          request.onerror = () => reject(request.error ?? new Error("LOGOS_IDB_REQUEST_FAILED"));
        } catch (error) {
          reject(error);
        }
      })
  );
}

function idbGet<T>(storeName: string, key: IDBValidKey): Promise<T | undefined> {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        const tx = db.transaction(storeName, "readonly");
        const req = tx.objectStore(storeName).get(key);
        req.onsuccess = () => resolve(req.result as T | undefined);
        req.onerror = () => reject(req.error);
      })
  );
}

function idbGetAllByIndex<T>(
  storeName: string,
  indexName: string,
  key: IDBValidKey
): Promise<T[]> {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        const tx = db.transaction(storeName, "readonly");
        const req = tx.objectStore(storeName).index(indexName).getAll(key);
        req.onsuccess = () => resolve(req.result as T[]);
        req.onerror = () => reject(req.error);
      })
  );
}

function idbDelete(storeName: string, key: IDBValidKey): Promise<void> {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        const tx = db.transaction(storeName, "readwrite");
        tx.objectStore(storeName).delete(key);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
      })
  );
}

// ---------------------------------------------------------------------------
// Local clock (monotonic per session)
// ---------------------------------------------------------------------------

let localClock = 0;

export function nextLocalClock(): number {
  return ++localClock;
}

// ---------------------------------------------------------------------------
// Snapshot write
// ---------------------------------------------------------------------------

function buildSnapshot(state = useDocumentStore.getState()): DocumentSnapshot {
  const { documentId, identitySource, pages, pageOrder, currentPageId, shapes } = state;
  return {
    documentId,
    identitySource,
    pages: Object.fromEntries(
      Object.entries(pages).map(([id, p]) => [
        id,
        { id: p.id, name: p.name, rootShapeIds: p.rootShapeIds },
      ])
    ),
    pageOrder,
    currentPageId,
    shapes: shapes as Record<string, unknown>,
  };
}

async function writeSnapshot(documentId: string, snapshot: DocumentSnapshot): Promise<void> {
  const record: PersistedDocument = {
    documentId,
    savedAt: new Date().toISOString(),
    snapshot,
  };
  await idbPut(STORE_SNAPSHOTS, record);
}

const snapshotWriteQueues = new Map<string, Promise<void>>();

function enqueueSnapshotWrite(documentId: string, snapshot: DocumentSnapshot): Promise<void> {
  const previous = snapshotWriteQueues.get(documentId) ?? Promise.resolve();
  const next = previous.catch(() => undefined).then(() => writeSnapshot(documentId, snapshot));
  snapshotWriteQueues.set(documentId, next);
  void next.finally(() => {
    if (snapshotWriteQueues.get(documentId) === next) snapshotWriteQueues.delete(documentId);
  }).catch(() => undefined);
  return next;
}

/** Persist the current document immediately; used by explicit save boundaries and verification. */
export async function persistDocumentNow(documentId: string): Promise<void> {
  await enqueueSnapshotWrite(documentId, buildSnapshot());
}

/**
 * Create an offline document at the only supported offline identity boundary.
 * The generated ID and empty initial snapshot reach IndexedDB before this
 * resolves, so callers must await it before enabling editing.
 */
export interface OfflineCreationOptions {
  /** Writer must resolve only after the initial snapshot reaches durable storage. */
  writeInitialSnapshot?: (documentId: string, snapshot: DocumentSnapshot) => Promise<void>;
  /** Prevent an async creation attempt from binding identity after its route owner changes. */
  isCurrent?: () => boolean;
}

let offlineCreationPromise: Promise<string> | null = null;
let offlineCreationCandidate: string | null = null;

export function createOfflineDocument(options: OfflineCreationOptions = {}): Promise<string> {
  const store = useDocumentStore.getState();
  if (store.documentId && store.identitySource === "offline") return Promise.resolve(store.documentId);
  if (offlineCreationPromise) return offlineCreationPromise;
  if (store.documentId || Object.keys(store.shapes).length !== 0 || store.pageOrder.length !== 1 ||
      store.pages[store.pageOrder[0]]?.rootShapeIds.length !== 0) {
    return Promise.reject(new Error("LOGOS_OFFLINE_CREATION_REQUIRES_NEW_EMPTY_DOCUMENT"));
  }
  const documentId = offlineCreationCandidate ??= crypto.randomUUID();
  const snapshot = { ...buildSnapshot(), documentId, identitySource: "offline" as const };
  const writeInitialSnapshot = options.writeInitialSnapshot ?? enqueueSnapshotWrite;
  const creation = writeInitialSnapshot(documentId, snapshot).then(() => {
    if (!(options.isCurrent?.() ?? true)) {
      offlineCreationCandidate = null;
      throw new Error("LOGOS_OFFLINE_CREATION_STALE_WORKSPACE");
    }
    if (!useDocumentStore.getState().associateOfflineDocumentIdentity(documentId)) {
      throw new Error("LOGOS_OFFLINE_IDENTITY_BIND_FAILED");
    }
    offlineCreationCandidate = null;
    return documentId;
  });
  // Keep the candidate after failure. A retry uses the same identity and an
  // idempotent IndexedDB key, but identity is never promoted before a write.
  offlineCreationPromise = creation.finally(() => { offlineCreationPromise = null; });
  return offlineCreationPromise;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Load a previously persisted document snapshot into the Zustand store.
 *
 * Call this **before** mounting the React tree so the UI immediately has data.
 * Returns `true` if a snapshot was found and restored, `false` otherwise.
 */
export async function readPersistedDocument(documentId: string): Promise<PersistedDocument | null> {
  const persisted = await idbGet<PersistedDocument>(STORE_SNAPSHOTS, documentId);
  if (!persisted) return null;
  if (persisted.documentId !== documentId) {
    console.warn("[logos/persist] Preserved but did not load a snapshot with mismatched document identity");
    return null;
  }
  return persisted;
}

/** Synchronous guarded commit; ownership is checked at the Zustand mutation boundary. */
export function applyPersistedDocument(
  documentId: string,
  persisted: PersistedDocument,
  canApply: () => boolean = () => true
): boolean {
  if (!canApply() || persisted.documentId !== documentId || persisted.snapshot.documentId && persisted.snapshot.documentId !== documentId) {
    return false;
  }
  const { snapshot } = persisted;
  const source = snapshot.identitySource;
  const trustedSource = (source === "offline" || source === "server") && snapshot.documentId === documentId
    ? source
    : source === "unknown" || source === "ambiguous" ? "unknown" : "legacy";
  const result = useDocumentStore.getState().hydrateWorkspaceDocument({
    pages: snapshot.pages as ReturnType<typeof useDocumentStore.getState>["pages"],
    pageOrder: snapshot.pageOrder,
    currentPageId: snapshot.currentPageId,
    shapes: snapshot.shapes as ReturnType<typeof useDocumentStore.getState>["shapes"],
  }, trustedSource === "server" || trustedSource === "offline" ? documentId : null, trustedSource, canApply);
  if (!result.ok) {
    console.warn("[logos/persist] Preserved but did not load an invalid document snapshot", result.diagnostic);
    return false;
  }
  console.info(
    `[logos/persist] Restored document "${documentId}" from IndexedDB (saved ${persisted.savedAt})`
  );
  return true;
}

/** Backwards-compatible convenience API for non-routed callers and tests. */
export async function loadPersistedDocument(documentId: string, canApply: () => boolean = () => true): Promise<boolean> {
  const persisted = await readPersistedDocument(documentId);
  if (!persisted || !canApply()) return false;
  return applyPersistedDocument(documentId, persisted, canApply);
}

/**
 * Enqueue a pending op (call when a local mutation is made while offline).
 */
export async function enqueuePendingOp(
  documentId: string,
  patch: Record<string, unknown>
): Promise<void> {
  const op: PendingOp = {
    documentId,
    createdAt: new Date().toISOString(),
    patch,
    localClock: nextLocalClock(),
  };
  await idbPut(STORE_PENDING_OPS, op);
}

/**
 * Read all pending ops for a document (for replay on reconnect).
 */
export async function getPendingOps(documentId: string): Promise<PendingOp[]> {
  return idbGetAllByIndex<PendingOp>(STORE_PENDING_OPS, "by-document", documentId);
}

/**
 * Delete a pending op after it has been successfully acknowledged by the server.
 */
export async function deletePendingOp(opId: number): Promise<void> {
  await idbDelete(STORE_PENDING_OPS, opId);
}

/**
 * Clear all persisted data for a document (e.g., on sign-out).
 */
export async function clearPersistedDocument(documentId: string): Promise<void> {
  await idbDelete(STORE_SNAPSHOTS, documentId);
  const ops = await getPendingOps(documentId);
  for (const op of ops) {
    if (op.opId !== undefined) await deletePendingOp(op.opId);
  }
}

// ---------------------------------------------------------------------------
// Persistence subscription
// ---------------------------------------------------------------------------

export interface PersistenceHandle {
  readonly documentId: string;
  stop(): void;
}

export interface PersistenceOptions {
  isCurrent?: () => boolean;
  schedule?: (callback: () => void, delayMs: number) => ReturnType<typeof setTimeout>;
  cancel?: (timer: ReturnType<typeof setTimeout>) => void;
  writeSnapshot?: (documentId: string, snapshot: DocumentSnapshot) => Promise<void>;
}

interface ActivePersistence {
  handle: PersistenceHandle;
  isCurrent: () => boolean;
  schedule: NonNullable<PersistenceOptions["schedule"]>;
  cancel: NonNullable<PersistenceOptions["cancel"]>;
  writeSnapshot: NonNullable<PersistenceOptions["writeSnapshot"]>;
  unsubscribe: (() => void) | null;
  writeTimer: ReturnType<typeof setTimeout> | null;
  pendingSnapshot: DocumentSnapshot | null;
}

let activePersistence: ActivePersistence | null = null;

/**
 * Start persisting the document store to IndexedDB.
 *
 * Call once after the app is mounted. Safe to call multiple times (idempotent).
 *
 * @param documentId  - The logical document ID (use a UUID from the backend,
 *                      or a well-known constant like "local" for single-user mode).
 * @param onStatus    - Optional callback to receive sync-status updates.
 */
export function initPersistence(
  documentId: string,
  onStatus?: (status: SyncStatus) => void,
  options: PersistenceOptions = {}
): PersistenceHandle {
  if (activePersistence?.handle.documentId === documentId && activePersistence.isCurrent()) return activePersistence.handle;
  stopPersistence();

  const isCurrent = options.isCurrent ?? (() => true);
  const schedule = options.schedule ?? ((callback, delay) => setTimeout(callback, delay));
  const cancel = options.cancel ?? ((timer) => clearTimeout(timer));
  const write = options.writeSnapshot ?? enqueueSnapshotWrite;
  const active = {} as ActivePersistence;
  const handle: PersistenceHandle = { documentId, stop: () => stopPersistence(handle) };
  Object.assign(active, { handle, isCurrent, schedule, cancel, writeSnapshot: write, unsubscribe: null, writeTimer: null, pendingSnapshot: null });
  activePersistence = active;
  const ownsPersistence = () => activePersistence === active && isCurrent();
  const emit = (status: SyncStatus) => { if (ownsPersistence()) onStatus?.(status); };

  emit("saving");
  active.unsubscribe = useDocumentStore.subscribe((state) => {
    if (!ownsPersistence()) return;
    // Debounce writes to avoid hammering IDB on every keystroke / drag
    if (active.writeTimer) cancel(active.writeTimer);
    active.pendingSnapshot = buildSnapshot(state);
    active.writeTimer = schedule(async () => {
      active.writeTimer = null;
      const snapshot = active.pendingSnapshot;
      active.pendingSnapshot = null;
      if (!snapshot || !ownsPersistence()) return;
      try {
        emit("saving");
        await write(documentId, snapshot);
        emit("saved");
      } catch (err) {
        console.error("[logos/persist] IDB write failed:", err);
        emit("error");
      }
    }, WRITE_DEBOUNCE_MS);
  });

  // Capture before crossing the async IDB boundary. A later route cannot cause
  // this write to serialize a different document from the shared store.
  const initialSnapshot = buildSnapshot();
  write(documentId, initialSnapshot)
    .then(() => emit("saved"))
    .catch((err) => {
      console.error("[logos/persist] Initial IDB write failed:", err);
      emit("error");
    });
  return handle;
}

/**
 * Stop persisting (e.g., when the user closes the document).
 */
export function stopPersistence(handle?: PersistenceHandle): void {
  const active = activePersistence;
  if (!active || (handle && active.handle !== handle)) return;
  activePersistence = null;
  active.unsubscribe?.();
  active.unsubscribe = null;
  if (active.writeTimer) active.cancel(active.writeTimer);
  active.writeTimer = null;
  active.pendingSnapshot = null;
}
