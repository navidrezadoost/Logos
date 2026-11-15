import * as React from "react";
import { createRoot } from "react-dom/client";
import { MemoryRouter, Route, Routes, useNavigate } from "react-router-dom";
import { WorkspacePage } from "../pages/WorkspacePage";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";

const A = "10000000-0000-4000-8000-000000000001";
const PAGE_A = "10000000-0000-4000-8000-000000000002";
const SHAPE_A = "10000000-0000-4000-8000-000000000003";
const B = "20000000-0000-4000-8000-000000000001";
const PAGE_B = "20000000-0000-4000-8000-000000000002";
const SHAPE_B = "20000000-0000-4000-8000-000000000003";

type BrowserEvidence = {
  state: () => unknown;
  navigateToB: () => void;
  isAPending: () => boolean;
  releaseA: () => void;
  errors: () => string[];
  ids: { a: string; b: string; pageA: string; pageB: string; shapeA: string; shapeB: string };
};

declare global {
  interface Window { __g1b92?: BrowserEvidence }
}

const status = document.getElementById("workspace-isolation-status") as HTMLPreElement;
const browserErrors: string[] = [];
window.addEventListener("error", (event) => browserErrors.push(event.message));
window.addEventListener("unhandledrejection", (event) => browserErrors.push(String(event.reason)));

function snapshot(documentId: string, pageId: string, shapeId: string, color: string, name: string) {
  return {
    documentId,
    identitySource: "server",
    pages: { [pageId]: { id: pageId, name: `${name} page`, rootShapeIds: [shapeId] } },
    pageOrder: [pageId],
    currentPageId: pageId,
    shapes: {
      [shapeId]: {
        id: shapeId, type: "rect", name, bounds: { x: 20, y: 20, w: 48, h: 48 },
        transform: [1, 0, 0, 1, 0, 0], rotation: 0,
        fills: [{ type: "solid", color, opacity: 1 }], opacity: 1, hidden: false,
        locked: false, parentId: null, children: [],
      },
    },
  };
}

async function seedSnapshots(): Promise<void> {
  const db = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open("logos-offline", 1);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains("snapshots")) database.createObjectStore("snapshots", { keyPath: "documentId" });
      if (!database.objectStoreNames.contains("pending-ops")) {
        const ops = database.createObjectStore("pending-ops", { keyPath: "opId", autoIncrement: true });
        ops.createIndex("by-document", "documentId", { unique: false });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction("snapshots", "readwrite");
    const store = tx.objectStore("snapshots");
    for (const [id, pageId, shapeId, color, name] of [
      [A, PAGE_A, SHAPE_A, "#ff0000", "Route A red"],
      [B, PAGE_B, SHAPE_B, "#00ff00", "Route B green"],
    ]) {
      store.put({ documentId: id, savedAt: new Date().toISOString(), snapshot: snapshot(id, pageId, shapeId, color, name) });
    }
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
  db.close();
}

let releaseA!: (response: Response) => void;
let aPending = false;
const delayedA = new Promise<Response>((resolve) => { releaseA = resolve; });
const nativeFetch = window.fetch.bind(window);
window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
  const url = new URL(typeof input === "string" || input instanceof URL ? input : input.url, location.href);
  if (url.pathname.endsWith("/get-file")) {
    const id = url.searchParams.get("id");
    const response = new Response(JSON.stringify({ id, data: { pages: id === A ? [PAGE_A] : [PAGE_B] } }), {
      status: 200, headers: { "Content-Type": "application/json" },
    });
    if (id === A) {
      aPending = true;
      return delayedA;
    }
    return Promise.resolve(response);
  }
  return nativeFetch(input, init);
}) as typeof window.fetch;

function RouteControls(): React.ReactElement | null {
  const navigate = useNavigate();
  React.useEffect(() => {
    window.__g1b92!.navigateToB = () => {
      useSelectionStore.getState().select(SHAPE_A);
      navigate(`/workspace/project-b/${B}`);
    };
  }, [navigate]);
  return null;
}

async function start(): Promise<void> {
  await seedSnapshots();
  window.__g1b92 = {
    state: () => {
      const state = useDocumentStore.getState();
      return { documentId: state.documentId, identitySource: state.identitySource, currentPageId: state.currentPageId,
        pageOrder: state.pageOrder, shapes: state.shapes, selectedIds: useSelectionStore.getState().selectedIds };
    },
    navigateToB: () => {},
    isAPending: () => aPending,
    releaseA: () => releaseA(new Response(JSON.stringify({ id: A, data: { pages: [PAGE_A] } }), {
      status: 200, headers: { "Content-Type": "application/json" },
    })),
    errors: () => [...browserErrors],
    ids: { a: A, b: B, pageA: PAGE_A, pageB: PAGE_B, shapeA: SHAPE_A, shapeB: SHAPE_B },
  };
  createRoot(document.getElementById("root")!).render(
    <MemoryRouter initialEntries={[`/workspace/project-a/${A}`]}>
      <RouteControls />
      <Routes><Route path="/workspace/:projectId/:fileId" element={<WorkspacePage />} /></Routes>
    </MemoryRouter>,
  );
  status.dataset.status = "ready";
  status.textContent = "ready";
}

void start().catch((error) => {
  status.dataset.status = "failed";
  status.textContent = String(error?.stack ?? error);
});
