import "fake-indexeddb/auto";
import { webcrypto } from "node:crypto";
import assert from "node:assert/strict";
import { test } from "node:test";

Object.defineProperty(globalThis, "crypto", { configurable: true, value: webcrypto });
Object.defineProperty(globalThis, "navigator", { configurable: true, value: { onLine: true } });
Object.defineProperty(globalThis, "window", {
  configurable: true,
  value: { addEventListener() {}, removeEventListener() {} },
});

const [{ useDocumentStore }, persistence, sessions, { openWorkspaceSession }, { SyncManager }] = await Promise.all([
  import("../src/stores/documentStore.ts"),
  import("../src/offline/persist.ts"),
  import("../src/offline/workspace-session.ts"),
  import("../src/offline/open-workspace.ts"),
  import("../src/offline/sync.ts"),
]);

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function snapshot(documentId: string, pageId = `page-${documentId}`) {
  return {
    documentId,
    identitySource: "server" as const,
    pages: { [pageId]: { id: pageId, name: documentId, rootShapeIds: [] as string[] } },
    pageOrder: [pageId],
    currentPageId: pageId,
    shapes: {},
  };
}

function record(documentId: string) {
  return { documentId, savedAt: "2026-01-01T00:00:00.000Z", snapshot: snapshot(documentId) };
}

function setEmptyDocument() {
  const fresh = useDocumentStore.getState();
  const pageId = fresh.pageOrder[0];
  const result = fresh.hydrateWorkspaceDocument({
    pages: { [pageId]: { id: pageId, name: "Fresh", rootShapeIds: [] } },
    pageOrder: [pageId], currentPageId: pageId, shapes: {},
  }, null, "unknown");
  assert.equal(result.ok, true);
  fresh.restoreDocumentIdentity(null, "unknown");
}

test("stale IndexedDB read cannot hydrate after navigation to another workspace", async () => {
  setEmptyDocument();
  const read = deferred<ReturnType<typeof record> | null>();
  let applies = 0;
  const a = sessions.beginWorkspaceSession("A", "p/A");
  openWorkspaceSession(a, null, {
    readPersistedDocument: () => read.promise,
    applyPersistedDocument: (...args) => { applies++; return persistence.applyPersistedDocument(...args); },
    getFile: async () => ({ id: "A" }),
    associateServerIdentity: () => true,
    selectPage() {}, startPersistence: () => ({ stop() {} }),
    createSyncManager: () => ({ start() {}, stop() {} }), onReady() {}, onError(error) { throw error; },
  });
  const b = sessions.beginWorkspaceSession("B", "p/B");
  read.resolve(record("A"));
  await read.promise;
  await Promise.resolve();
  assert.equal(applies, 0);
  assert.notEqual(useDocumentStore.getState().documentId, "A");
  b.dispose();
});

test("stale getFile response cannot bind identity or start persistence and sync", async () => {
  setEmptyDocument();
  const response = deferred<{ id: string; data: { pages: string[] } }>();
  let associations = 0, persistenceStarts = 0, syncStarts = 0, ready = 0;
  const a = sessions.beginWorkspaceSession("A", "p/A");
  openWorkspaceSession(a, null, {
    readPersistedDocument: async () => record("A"),
    applyPersistedDocument: persistence.applyPersistedDocument,
    getFile: () => response.promise,
    associateServerIdentity: () => { associations++; return true; }, selectPage() {},
    startPersistence: () => { persistenceStarts++; return { stop() {} }; },
    createSyncManager: () => { syncStarts++; return { start() { syncStarts++; }, stop() {} }; },
    onReady: () => { ready++; }, onError(error) { throw error; },
  });
  const b = sessions.beginWorkspaceSession("B", "p/B");
  response.resolve({ id: "A", data: { pages: ["page-A"] } });
  await response.promise;
  await Promise.resolve();
  assert.deepEqual([associations, persistenceStarts, syncStarts, ready], [0, 0, 0, 0]);
  b.dispose();
});

test("unmount before hydration completion prevents every later side effect", async () => {
  setEmptyDocument();
  const read = deferred<ReturnType<typeof record> | null>();
  let applies = 0, starts = 0;
  const a = sessions.beginWorkspaceSession("A", "p/A");
  const unmount = openWorkspaceSession(a, null, {
    readPersistedDocument: () => read.promise,
    applyPersistedDocument: () => { applies++; return true; },
    getFile: async () => ({ id: "A" }), associateServerIdentity: () => true, selectPage() {},
    startPersistence: () => { starts++; return { stop() {} }; },
    createSyncManager: () => { starts++; return { start() {}, stop() {} }; }, onReady() { starts++; }, onError(error) { throw error; },
  });
  unmount();
  read.resolve(record("A"));
  await read.promise;
  await Promise.resolve();
  assert.deepEqual([applies, starts], [0, 0]);
});

test("wrong server file ID and page membership are rejected without identity binding", () => {
  const docId = "requested";
  const current = snapshot(docId);
  assert.equal(useDocumentStore.getState().hydrateWorkspaceDocument(current, null, "unknown").ok, true);
  const store = useDocumentStore.getState();
  assert.equal(store.associateServerDocumentIdentity(docId, "other-file", [`page-${docId}`]), false);
  assert.equal(store.associateServerDocumentIdentity(docId, docId, ["other-page"]), false);
  assert.equal(useDocumentStore.getState().documentId, null);
  assert.equal(useDocumentStore.getState().identitySource, "unknown");
});

test("A to B to A creates a distinct workspace generation", () => {
  const firstA = sessions.beginWorkspaceSession("A", "p/A");
  const b = sessions.beginWorkspaceSession("B", "p/B");
  const secondA = sessions.beginWorkspaceSession("A", "p/A");
  assert.notEqual(firstA.generation, secondA.generation);
  assert.equal(firstA.isCurrent(), false);
  assert.equal(b.isCurrent(), false);
  assert.equal(secondA.isCurrent(), true);
  secondA.dispose();
});

test("initial persistence captures its document snapshot before asynchronous write", async () => {
  setEmptyDocument();
  const pageA = useDocumentStore.getState().currentPageId;
  const pageB = crypto.randomUUID();
  const writes: Array<{ id: string; value: unknown; done: ReturnType<typeof deferred<void>> }> = [];
  const firstWrite = deferred<void>();
  const handle = persistence.initPersistence("A", undefined, {
    writeSnapshot: (id, value) => {
      const done = writes.length === 0 ? firstWrite : deferred<void>();
      writes.push({ id, value, done });
      return done.promise;
    },
  });
  assert.equal(useDocumentStore.getState().hydrateWorkspaceDocument({
    pages: { [pageB]: { id: pageB, name: "Later B", rootShapeIds: [] } },
    pageOrder: [pageB], currentPageId: pageB, shapes: {},
  }, null, "unknown").ok, true);
  assert.equal(writes[0].id, "A");
  assert.equal((writes[0].value as { documentId?: string }).documentId, null);
  assert.equal((writes[0].value as { currentPageId: string }).currentPageId, pageA);
  assert.equal(useDocumentStore.getState().currentPageId, pageB);
  handle.stop();
  firstWrite.resolve();
  await firstWrite.promise;
});

test("persistence debounce callback after teardown cannot write the stale owner", async () => {
  setEmptyDocument();
  const callbacks: Array<() => void> = [];
  const writes: string[] = [];
  const handle = persistence.initPersistence("A", undefined, {
    schedule: (callback) => { callbacks.push(callback); return callbacks.length as unknown as ReturnType<typeof setTimeout>; },
    cancel: () => {},
    writeSnapshot: async (id) => { writes.push(id); },
  });
  useDocumentStore.getState().addPage("change");
  handle.stop();
  callbacks.at(-1)?.();
  await Promise.resolve();
  assert.deepEqual(writes, ["A"]); // only the initial captured write ran; debounce was canceled
});

test("stale sync fetch response cannot send, delete, or patch after ownership changes", async () => {
  const docId = "sync-fetch-race";
  await persistence.enqueuePendingOp(docId, { newShape: { id: "newShape" } });
  const response = deferred<Record<string, unknown>>();
  const fetchStarted = deferred<void>();
  let current = true, sends = 0;
  const statuses: string[] = [];
  const sync = new SyncManager({
    documentId: docId, isCurrent: () => current, sendOp: async () => { sends++; },
    fetchServerState: () => { fetchStarted.resolve(); return response.promise; }, onStatus: (status) => statuses.push(status),
  });
  const flush = sync.flushPendingOps();
  await fetchStarted.promise;
  current = false;
  response.resolve({});
  await flush;
  assert.equal(sends, 0);
  assert.equal((await persistence.getPendingOps(docId)).length, 1);
  assert.equal(statuses.includes("error"), false);
});

test("stale sync send acknowledgement leaves pending operation and document untouched", async () => {
  const docId = "sync-send-race";
  setEmptyDocument();
  const shapeId = useDocumentStore.getState().addRect({ x: 0, y: 0, w: 1, h: 1 });
  useDocumentStore.getState().batchUpdate({ [shapeId]: { name: "before" } });
  await persistence.enqueuePendingOp(docId, { [shapeId]: { name: "after" } });
  const ack = deferred<void>();
  const sendStarted = deferred<void>();
  let current = true;
  const sync = new SyncManager({ documentId: docId, isCurrent: () => current, sendOp: () => { sendStarted.resolve(); return ack.promise; },
    fetchServerState: async () => ({ [shapeId]: { name: "server" } }), onStatus() {} });
  const flush = sync.flushPendingOps();
  await sendStarted.promise;
  current = false;
  ack.resolve();
  await flush;
  assert.equal((await persistence.getPendingOps(docId)).length, 1);
  assert.equal((useDocumentStore.getState().shapes[shapeId] as { name: string }).name, "before");
});

test("concurrent offline creation shares one identity and durable initial snapshot", async () => {
  setEmptyDocument();
  const write = deferred<void>();
  let count = 0, writtenId = "";
  const writer = async (id: string) => { count++; writtenId = id; await write.promise; };
  const first = persistence.createOfflineDocument({ writeInitialSnapshot: writer });
  const second = persistence.createOfflineDocument({ writeInitialSnapshot: writer });
  assert.equal(count, 1);
  assert.equal(useDocumentStore.getState().documentId, null);
  write.resolve();
  assert.equal(await first, await second);
  assert.equal(useDocumentStore.getState().documentId, writtenId);
  assert.equal(useDocumentStore.getState().identitySource, "offline");
});

test("failed offline persistence retains no identity and retry reuses candidate ID", async () => {
  setEmptyDocument();
  let attempted: string[] = [];
  await assert.rejects(persistence.createOfflineDocument({ writeInitialSnapshot: async (id) => {
    attempted.push(id); throw new Error("injected write failure");
  } }), /injected write failure/);
  assert.equal(useDocumentStore.getState().documentId, null);
  assert.equal(useDocumentStore.getState().identitySource, "unknown");
  const result = await persistence.createOfflineDocument({ writeInitialSnapshot: async (id) => { attempted.push(id); } });
  assert.equal(attempted.length, 2);
  assert.equal(attempted[0], attempted[1]);
  assert.equal(result, attempted[0]);
});

test("offline creation cannot bind identity after its workspace owner changes", async () => {
  setEmptyDocument();
  const write = deferred<void>();
  let current = true;
  const creation = persistence.createOfflineDocument({ isCurrent: () => current, writeInitialSnapshot: () => write.promise });
  current = false;
  write.resolve();
  await assert.rejects(creation, /LOGOS_OFFLINE_CREATION_STALE_WORKSPACE/);
  assert.equal(useDocumentStore.getState().documentId, null);
  assert.equal(useDocumentStore.getState().identitySource, "unknown");
});

test("legacy nanoid page identity survives persisted reload unchanged", () => {
  const pageId = "historical-nanoid-abc123";
  const legacy = { ...record("legacy-doc"), snapshot: {
    pages: { [pageId]: { id: pageId, name: "Legacy", rootShapeIds: [] } },
    pageOrder: [pageId], currentPageId: pageId, shapes: {},
  } };
  assert.equal(persistence.applyPersistedDocument("legacy-doc", legacy), true);
  assert.deepEqual(useDocumentStore.getState().pageOrder, [pageId]);
  assert.equal(useDocumentStore.getState().documentId, null);
  assert.equal(useDocumentStore.getState().identitySource, "legacy");
});
