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
        tx.objectStore(storeName).put(value);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error);
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

function buildSnapshot(): DocumentSnapshot {
  const { pages, pageOrder, currentPageId, shapes } = useDocumentStore.getState();
  return {
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

async function writeSnapshot(documentId: string): Promise<void> {
  const snapshot = buildSnapshot();
  const record: PersistedDocument = {
    documentId,
    savedAt: new Date().toISOString(),
    snapshot,
  };
  await idbPut(STORE_SNAPSHOTS, record);
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
export async function loadPersistedDocument(documentId: string): Promise<boolean> {
  const persisted = await idbGet<PersistedDocument>(STORE_SNAPSHOTS, documentId);
  if (!persisted) return false;

  const { snapshot } = persisted;
  useDocumentStore.setState({
    pages: snapshot.pages as ReturnType<typeof useDocumentStore.getState>["pages"],
    pageOrder: snapshot.pageOrder,
    currentPageId: snapshot.currentPageId,
    shapes: snapshot.shapes as ReturnType<typeof useDocumentStore.getState>["shapes"],
  });

  console.info(
    `[logos/persist] Restored document "${documentId}" from IndexedDB (saved ${persisted.savedAt})`
  );
  return true;
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

let unsubscribe: (() => void) | null = null;
let writeTimer: ReturnType<typeof setTimeout> | null = null;

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
  onStatus?: (status: SyncStatus) => void
): void {
  if (unsubscribe) return; // already initialised

  onStatus?.("saving");

  unsubscribe = useDocumentStore.subscribe(() => {
    // Debounce writes to avoid hammering IDB on every keystroke / drag
    if (writeTimer) clearTimeout(writeTimer);
    writeTimer = setTimeout(async () => {
      try {
        onStatus?.("saving");
        await writeSnapshot(documentId);
        onStatus?.("saved");
      } catch (err) {
        console.error("[logos/persist] IDB write failed:", err);
        onStatus?.("error");
      }
    }, WRITE_DEBOUNCE_MS);
  });

  // Write an initial snapshot immediately
  writeSnapshot(documentId)
    .then(() => onStatus?.("saved"))
    .catch((err) => {
      console.error("[logos/persist] Initial IDB write failed:", err);
      onStatus?.("error");
    });
}

/**
 * Stop persisting (e.g., when the user closes the document).
 */
export function stopPersistence(): void {
  if (unsubscribe) {
    unsubscribe();
    unsubscribe = null;
  }
  if (writeTimer) {
    clearTimeout(writeTimer);
    writeTimer = null;
  }
}
