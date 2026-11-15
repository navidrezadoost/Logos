import "fake-indexeddb/auto";
import { webcrypto } from "node:crypto";
import assert from "node:assert/strict";
import { test } from "node:test";

Object.defineProperty(globalThis, "crypto", { configurable: true, value: webcrypto });

const [{ useDocumentStore }, persistence, { classifyRustIdentityEligibility }] = await Promise.all([
  import("../src/stores/documentStore.ts"),
  import("../src/offline/persist.ts"),
  import("../src/document-core/identity-eligibility.ts"),
]);

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function persistedRecord(documentId: string): Promise<{ savedAt: string; snapshot: unknown } | undefined> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open("logos-offline", 1);
    request.onerror = () => reject(request.error);
    request.onsuccess = () => {
      const tx = request.result.transaction("snapshots", "readonly");
      const get = tx.objectStore("snapshots").get(documentId);
      get.onerror = () => reject(get.error);
      get.onsuccess = () => resolve(get.result as { savedAt: string; snapshot: unknown } | undefined);
    };
  });
}

test("offline identity is persisted before edits and remains stable across repeated reloads", async () => {
  const store = useDocumentStore.getState();
  const originalPageId = store.pageOrder[0];
  assert.match(originalPageId, UUID, "new empty React document page uses a UUID");

  const documentId = await persistence.createOfflineDocument();
  assert.match(documentId, UUID);
  assert.equal(useDocumentStore.getState().identitySource, "offline");
  const creation = await persistedRecord(documentId);
  assert.ok(creation);
  assert.equal(creation.snapshot && (creation.snapshot as { documentId: string }).documentId, documentId);

  const addedPageId = useDocumentStore.getState().addPage("Added");
  assert.match(addedPageId, UUID);
  assert.notEqual(addedPageId, originalPageId);
  await persistence.persistDocumentNow(documentId);

  useDocumentStore.getState().restoreDocumentIdentity(null, "unknown");
  assert.equal(await persistence.loadPersistedDocument(documentId), true);
  const firstLoad = useDocumentStore.getState();
  const loadedIds = { documentId: firstLoad.documentId, pageOrder: [...firstLoad.pageOrder] };
  assert.equal(firstLoad.documentId, documentId);
  assert.equal(firstLoad.identitySource, "offline");

  assert.equal(await persistence.loadPersistedDocument(documentId), true);
  assert.deepEqual({
    documentId: useDocumentStore.getState().documentId,
    pageOrder: useDocumentStore.getState().pageOrder,
  }, loadedIds);
});

test("legacy page IDs survive hydration and remain editable in React", () => {
  const legacyPageId = "historical-nanoid-page-id";
  const result = useDocumentStore.getState().hydrateDocument({
    pages: { [legacyPageId]: { id: legacyPageId, name: "Old page", rootShapeIds: [] } },
    pageOrder: [legacyPageId], currentPageId: legacyPageId, shapes: {},
  });
  assert.deepEqual(result, { ok: true });
  assert.equal(useDocumentStore.getState().pageOrder[0], legacyPageId);

  const shapeId = useDocumentStore.getState().addRect({ x: 0, y: 0, w: 10, h: 10 });
  assert.equal(useDocumentStore.getState().pages[legacyPageId].rootShapeIds[0], shapeId);
});

test("server identity requires exact file and page ownership and survives IndexedDB reload", async () => {
  const documentId = crypto.randomUUID();
  const pageId = crypto.randomUUID();
  const hydration = useDocumentStore.getState().hydrateDocument({
    pages: { [pageId]: { id: pageId, name: "Server page", rootShapeIds: [] } },
    pageOrder: [pageId], currentPageId: pageId, shapes: {},
  });
  assert.deepEqual(hydration, { ok: true });
  const store = useDocumentStore.getState();
  assert.equal(store.associateServerDocumentIdentity(documentId, crypto.randomUUID(), [pageId]), false);
  assert.equal(store.associateServerDocumentIdentity(documentId, documentId, [crypto.randomUUID()]), false);
  assert.equal(store.associateServerDocumentIdentity(documentId, documentId, [pageId]), true);

  await persistence.persistDocumentNow(documentId);
  useDocumentStore.getState().restoreDocumentIdentity(null, "unknown");
  assert.equal(await persistence.loadPersistedDocument(documentId), true);
  assert.equal(useDocumentStore.getState().documentId, documentId);
  assert.equal(useDocumentStore.getState().identitySource, "server");
});

test("Rust eligibility is read-only and distinguishes legacy, unsupported, unavailable, and ambiguous documents", async () => {
  const documentId = crypto.randomUUID();
  const pageId = crypto.randomUUID();
  const ready = {
    documentId, identitySource: "offline" as const,
    pages: { [pageId]: { id: pageId, name: "Page", rootShapeIds: [] } },
    pageOrder: [pageId], shapes: {},
  };
  assert.equal(classifyRustIdentityEligibility(ready), "CANONICAL_UUID_READY");
  assert.equal(classifyRustIdentityEligibility({ ...ready, documentId: null }), "DOCUMENT_ID_UNAVAILABLE");
  assert.equal(classifyRustIdentityEligibility({ ...ready, identitySource: "unknown" }), "AMBIGUOUS_OWNERSHIP");

  const legacyPageId = "nanoid-page-legacy";
  assert.equal(classifyRustIdentityEligibility({
    ...ready, pages: { [legacyPageId]: { id: legacyPageId, name: "Old", rootShapeIds: [] } },
    pageOrder: [legacyPageId],
  }), "LEGACY_IDENTITY");

  const shapeId = crypto.randomUUID();
  const unsupported = {
    ...ready,
    pages: { [pageId]: { id: pageId, name: "Page", rootShapeIds: [shapeId] } },
    shapes: { [shapeId]: {
      id: shapeId, type: "vector-network", name: "Legacy path", bounds: { x: 0, y: 0, w: 5, h: 5 },
      transform: [1, 0, 0, 1, 0, 0], rotation: 0, fills: [], opacity: 1, hidden: false,
      locked: false, parentId: null, children: [],
    } },
  } as const;
  const inputBefore = JSON.stringify(unsupported);
  const recordBefore = await persistedRecord(documentId);
  assert.equal(classifyRustIdentityEligibility(unsupported), "UNSUPPORTED_DOCUMENT_CONTENT");
  assert.equal(JSON.stringify(unsupported), inputBefore);
  assert.deepEqual(await persistedRecord(documentId), recordBefore);
});
