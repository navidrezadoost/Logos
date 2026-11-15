import React from "react";
import { createRoot } from "react-dom/client";
import { getFile } from "../api/client";
import { useDocumentStore } from "../stores/documentStore";
import { projectLegacyFileData, encodeGeometryChange } from "./p04-document-adapter";
import { classifyRustIdentityEligibility } from "../document-core/identity-eligibility";
import { createOfflineDocument, loadPersistedDocument, persistDocumentNow } from "../offline/persist";

type TestConfig = { fileId: string };
type VersionedFileDetail = Awaited<ReturnType<typeof getFile>> & { revn: number; vern: number };

const status = document.getElementById("p05-status") as HTMLPreElement;

let stage = "setup";
function setStage(next: string): void {
  stage = next;
  status.dataset.stage = next;
}
const browserErrors: string[] = [];
window.addEventListener("error", (event) => browserErrors.push(event.error?.stack ?? event.message));
window.addEventListener("unhandledrejection", (event) => browserErrors.push(String(event.reason)));
const originalConsoleError = console.error.bind(console);
console.error = (...args: unknown[]) => {
  browserErrors.push(args.map(String).join(" "));
  originalConsoleError(...args);
};

function fail(error: unknown): void {
  status.dataset.status = "failed";
  status.textContent = JSON.stringify({ stage, error: String(error), browserErrors }, null, 2);
}

/** Isolate the identity lifecycle from Chromium's virtual-time IndexedDB scheduler. */
function installMemoryIndexedDb(): void {
  const records = new Map<string, unknown>();
  const database = {
    objectStoreNames: { contains: () => true },
    createObjectStore: () => ({ createIndex: () => undefined }),
    transaction: () => {
      const tx: { oncomplete: (() => void) | null; onerror: (() => void) | null; onabort: (() => void) | null } = {
        oncomplete: null, onerror: null, onabort: null,
      };
      const finish = (callback: () => void): void => queueMicrotask(() => {
        callback();
        tx.oncomplete?.();
      });
      return Object.assign(tx, {
        objectStore: () => ({
          put(value: { documentId: string }) {
            const request: { onsuccess: (() => void) | null; onerror: (() => void) | null } = {
              onsuccess: null, onerror: null,
            };
            finish(() => { records.set(value.documentId, value); request.onsuccess?.(); });
            return request;
          },
          get(key: string) {
            const request: { result?: unknown; onsuccess: (() => void) | null; onerror: (() => void) | null } = {
              onsuccess: null, onerror: null,
            };
            finish(() => { request.result = records.get(key); request.onsuccess?.(); });
            return request;
          },
          index: () => ({ getAll: () => ({ onsuccess: null, onerror: null }) }),
          delete: (key: string) => { records.delete(key); return {}; },
        }),
      });
    },
  };
  const factory = {
    open: () => {
      const request: { result?: unknown; onsuccess: (() => void) | null; onerror: (() => void) | null; onupgradeneeded: ((event: unknown) => void) | null } = {
        onsuccess: null, onerror: null, onupgradeneeded: null,
      };
      queueMicrotask(() => {
        request.result = database;
        request.onupgradeneeded?.({ target: { result: database } });
        request.onsuccess?.();
      });
      return request;
    },
  };
  Object.defineProperty(window, "indexedDB", { configurable: true, value: factory });
}

async function jsonResponse<T>(response: Response, label: string): Promise<T> {
  if (!response.ok) {
    throw new Error(`${label}: HTTP ${response.status}: ${await response.text()}`);
  }
  return response.json() as Promise<T>;
}

async function run(): Promise<void> {
  setStage("load browser test configuration");
  const config = await jsonResponse<TestConfig>(
    await fetch("/__test/config", { credentials: "include" }),
    "browser test configuration",
  );
  if (!config.fileId) throw new Error("browser test configuration omitted fileId");

  setStage("Go get-file with browser session cookie");
  const before = await getFile(config.fileId) as VersionedFileDetail;
  if (before.id !== config.fileId || before.revn !== 0) {
    throw new Error(`initial get-file identity/revision=${before.id}/${before.revn}, expected ${config.fileId}/0`);
  }
  const projection = projectLegacyFileData(before.data);
  const page = projection.pages[projection.currentPageId];
  const shapeId = page.rootShapeIds[0];
  const initialShape = projection.shapes[shapeId];
  if (!initialShape) throw new Error("versioned adapter produced no supported rectangle");

  const hydration = useDocumentStore.getState().hydrateDocument({
    pages: projection.pages,
    pageOrder: projection.pageOrder,
    currentPageId: projection.currentPageId,
    shapes: projection.shapes,
  });
  if (!hydration.ok) throw new Error(`P0.5 fixture failed hydration: ${hydration.diagnostic.code}`);
  const identityActions = useDocumentStore.getState();
  if (identityActions.associateServerDocumentIdentity(config.fileId, crypto.randomUUID(), projection.pageOrder) ||
      identityActions.associateServerDocumentIdentity(config.fileId, before.id, ["wrong-page-id"])) {
    throw new Error("mismatched server identity or page ownership was accepted");
  }
  if (!useDocumentStore.getState().associateServerDocumentIdentity(config.fileId, before.id, projection.pageOrder)) {
    throw new Error("server file ID/page membership was not bound to its exact returned identity");
  }

  setStage("existing editor geometry command");
  useDocumentStore.getState().updateShape(shapeId, {
    bounds: { ...initialShape.bounds, x: initialShape.bounds.x + 5 },
  });
  const editedShape = useDocumentStore.getState().shapes[shapeId];
  if (editedShape.bounds.x !== initialShape.bounds.x + 5) {
    throw new Error(`editor command geometry x=${editedShape.bounds.x}, expected ${initialShape.bounds.x + 5}`);
  }

  setStage("Go-compatible update-file operation");
  const change = encodeGeometryChange(projection, shapeId, editedShape.bounds);
  const updateResponse = await jsonResponse<{ revn: number }>(await fetch("/api/rpc/command/update-file", {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      id: config.fileId,
      "session-id": crypto.randomUUID(),
      revn: before.revn,
      vern: before.vern,
      changes: [change],
    }),
  }), "update-file");
  if (updateResponse.revn !== 0) {
    throw new Error(`update-file acknowledged base revision ${updateResponse.revn}, expected 0`);
  }

  setStage("Go get-file reload");
  const reloaded = await getFile(config.fileId) as VersionedFileDetail;
  if (reloaded.revn !== 1) throw new Error(`reload revision=${reloaded.revn}, expected 1`);
  const after = projectLegacyFileData(reloaded.data);
  const reloadedShape = after.shapes[shapeId];
  if (!reloadedShape) throw new Error(`reloaded shape ${shapeId} is missing`);
  if (reloaded.id !== before.id || after.documentId !== projection.documentId ||
      JSON.stringify(after.pageOrder) !== JSON.stringify(projection.pageOrder) ||
      JSON.stringify(after.pages[projection.currentPageId].rootShapeIds) !== JSON.stringify(page.rootShapeIds)) {
    throw new Error("reloaded document/page identity or ordered root hierarchy changed");
  }
  if (JSON.stringify(reloadedShape.bounds) !== JSON.stringify(editedShape.bounds)) {
    throw new Error(`reloaded geometry=${JSON.stringify(reloadedShape.bounds)}, expected ${JSON.stringify(editedShape.bounds)}`);
  }
  if (JSON.stringify(reloadedShape.fills[0]) !== JSON.stringify(initialShape.fills[0])) {
    throw new Error("reloaded solid fill changed");
  }
  const extension = after.unknownFieldsById[shapeId]["extension-p0"] as { sentinel?: string } | undefined;
  if (extension?.sentinel !== "preserve-me") throw new Error("reloaded unknown extension sentinel changed");
  if (browserErrors.length) throw new Error(`browser errors: ${browserErrors.join(" | ")}`);

  setStage("verified server identity IndexedDB reload");
  installMemoryIndexedDb();
  setStage("persist verified server identity");
  await persistDocumentNow(config.fileId);
  setStage("reload verified server identity");
  if (!await loadPersistedDocument(config.fileId) ||
      useDocumentStore.getState().documentId !== config.fileId ||
      useDocumentStore.getState().identitySource !== "server") {
    throw new Error("verified server document identity did not survive IndexedDB reload");
  }
  setStage("server identity reload complete");

  setStage("read-only eligibility classifications");
  const originalIdentity = useDocumentStore.getState().documentId;
  const pageId = crypto.randomUUID();
  const readyInput = {
    documentId: crypto.randomUUID(), identitySource: "offline" as const,
    pages: { [pageId]: { id: pageId, name: "Identity test", rootShapeIds: [] } },
    pageOrder: [pageId], shapes: {},
  };
  if (classifyRustIdentityEligibility(readyInput) !== "CANONICAL_UUID_READY") {
    throw new Error("valid canonical empty document was not Rust-candidate eligible");
  }
  const legacyPageId = "historical-nanoid-page";
  const legacyInput = {
    ...readyInput, identitySource: "server" as const,
    pages: { [legacyPageId]: { id: legacyPageId, name: "Legacy", rootShapeIds: [] } },
    pageOrder: [legacyPageId],
  };
  if (classifyRustIdentityEligibility(legacyInput) !== "LEGACY_IDENTITY") {
    throw new Error("historical page identity was not classified as legacy");
  }
  const unsupportedShape = { ...initialShape, type: "vector-network" as const };
  const unsupportedInput = {
    ...readyInput,
    pages: { [pageId]: { id: pageId, name: "Identity test", rootShapeIds: [unsupportedShape.id] } },
    shapes: { [unsupportedShape.id]: unsupportedShape },
  };
  const unsupportedBefore = JSON.stringify(unsupportedInput);
  if (classifyRustIdentityEligibility(unsupportedInput) !== "UNSUPPORTED_DOCUMENT_CONTENT" ||
      JSON.stringify(unsupportedInput) !== unsupportedBefore) {
    throw new Error("unsupported content was not rejected read-only");
  }
  if (classifyRustIdentityEligibility({ ...readyInput, documentId: null }) !== "DOCUMENT_ID_UNAVAILABLE" ||
      classifyRustIdentityEligibility({ ...readyInput, identitySource: "unknown" }) !== "AMBIGUOUS_OWNERSHIP") {
    throw new Error("missing or unverified document ownership classification failed");
  }
  if (useDocumentStore.getState().documentId !== originalIdentity) {
    throw new Error("eligibility evaluation mutated the live document identity");
  }

  setStage("offline identity durability and reload");
  const emptyPageId = crypto.randomUUID();
  useDocumentStore.getState().restoreDocumentIdentity(null, "unknown");
  const emptyHydration = useDocumentStore.getState().hydrateDocument({
    pages: { [emptyPageId]: { id: emptyPageId, name: "Page 1", rootShapeIds: [] } },
    pageOrder: [emptyPageId], currentPageId: emptyPageId, shapes: {},
  });
  if (!emptyHydration.ok) throw new Error(`offline empty fixture hydration failed: ${emptyHydration.diagnostic.code}`);
  const offlineId = await createOfflineDocument();
  if (useDocumentStore.getState().documentId !== offlineId ||
      useDocumentStore.getState().identitySource !== "offline") {
    throw new Error("offline identity was not bound after durable creation");
  }
  const newPageId = useDocumentStore.getState().addPage("Added page");
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(newPageId)) {
    throw new Error("new page ID is not a UUID");
  }
  setStage("persist offline edits");
  await persistDocumentNow(offlineId);
  setStage("reload offline identity");
  if (!await loadPersistedDocument(offlineId)) throw new Error("offline document did not reload");
  const offlineIdsAfterFirstLoad = JSON.stringify({
    documentId: useDocumentStore.getState().documentId,
    pageOrder: useDocumentStore.getState().pageOrder,
  });
  if (!await loadPersistedDocument(offlineId)) throw new Error("offline document did not reload twice");
  const offlineIdsAfterSecondLoad = JSON.stringify({
    documentId: useDocumentStore.getState().documentId,
    pageOrder: useDocumentStore.getState().pageOrder,
  });
  if (offlineIdsAfterFirstLoad !== offlineIdsAfterSecondLoad ||
      useDocumentStore.getState().pages[legacyPageId] !== undefined) {
    throw new Error("repeated hydration changed offline identities");
  }
  setStage("offline identity reload complete");
  // Historical IDs are preserved through the ordinary React editing path.
  const legacyHydration = useDocumentStore.getState().hydrateDocument({
    pages: { [legacyPageId]: { id: legacyPageId, name: "Legacy page", rootShapeIds: [] } },
    pageOrder: [legacyPageId], currentPageId: legacyPageId, shapes: {},
  });
  if (!legacyHydration.ok || useDocumentStore.getState().pageOrder[0] !== legacyPageId) {
    throw new Error("legacy page ID was not preserved during React hydration");
  }
  const legacyEditedShapeId = useDocumentStore.getState().addRect({ x: 0, y: 0, w: 10, h: 10 });
  if (useDocumentStore.getState().pages[legacyPageId]?.rootShapeIds[0] !== legacyEditedShapeId) {
    throw new Error("legacy document could not use the React compatibility edit path");
  }

  status.dataset.status = "passed";
  status.textContent = JSON.stringify({
    test: "authenticated browser fetch → existing document command → Go update-file → get-file reload",
    adapterVersion: projection.adapterVersion,
    fileId: config.fileId,
    revision: reloaded.revn,
    pageOrder: after.pageOrder,
    rootShapeIds: after.pages[after.currentPageId].rootShapeIds,
    shapeId,
    initialBounds: initialShape.bounds,
    reloadedBounds: reloadedShape.bounds,
    fill: reloadedShape.fills[0],
    extension,
    browserErrors,
    identityEvidence: {
      serverId: before.id,
      offlineId,
      newPageId,
      eligibility: ["CANONICAL_UUID_READY", "LEGACY_IDENTITY", "UNSUPPORTED_DOCUMENT_CONTENT",
        "DOCUMENT_ID_UNAVAILABLE", "AMBIGUOUS_OWNERSHIP"],
      repeatedHydrationStable: offlineIdsAfterFirstLoad === offlineIdsAfterSecondLoad,
      historicalPageId: legacyPageId,
    },
  }, null, 2);
}

createRoot(document.getElementById("app")!).render(<div id="p05-ready">P0.5 browser persistence test ready</div>);
void run().catch(fail);
