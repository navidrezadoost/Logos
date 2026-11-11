import React from "react";
import { createRoot } from "react-dom/client";
import { getFile } from "../api/client";
import { useDocumentStore } from "../stores/documentStore";
import { projectLegacyFileData, encodeGeometryChange } from "./p04-document-adapter";

type TestConfig = { fileId: string };
type VersionedFileDetail = Awaited<ReturnType<typeof getFile>> & { revn: number; vern: number };

const status = document.getElementById("p05-status") as HTMLPreElement;

let stage = "setup";
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

async function jsonResponse<T>(response: Response, label: string): Promise<T> {
  if (!response.ok) {
    throw new Error(`${label}: HTTP ${response.status}: ${await response.text()}`);
  }
  return response.json() as Promise<T>;
}

async function run(): Promise<void> {
  stage = "load browser test configuration";
  const config = await jsonResponse<TestConfig>(
    await fetch("/__test/config", { credentials: "include" }),
    "browser test configuration",
  );
  if (!config.fileId) throw new Error("browser test configuration omitted fileId");

  stage = "Go get-file with browser session cookie";
  const before = await getFile(config.fileId) as VersionedFileDetail;
  if (before.id !== config.fileId || before.revn !== 0) {
    throw new Error(`initial get-file identity/revision=${before.id}/${before.revn}, expected ${config.fileId}/0`);
  }
  const projection = projectLegacyFileData(before.data);
  const page = projection.pages[projection.currentPageId];
  const shapeId = page.rootShapeIds[0];
  const initialShape = projection.shapes[shapeId];
  if (!initialShape) throw new Error("versioned adapter produced no supported rectangle");

  useDocumentStore.setState({
    pages: projection.pages,
    pageOrder: projection.pageOrder,
    currentPageId: projection.currentPageId,
    shapes: projection.shapes,
  });

  stage = "existing editor geometry command";
  useDocumentStore.getState().updateShape(shapeId, {
    bounds: { ...initialShape.bounds, x: initialShape.bounds.x + 5 },
  });
  const editedShape = useDocumentStore.getState().shapes[shapeId];
  if (editedShape.bounds.x !== initialShape.bounds.x + 5) {
    throw new Error(`editor command geometry x=${editedShape.bounds.x}, expected ${initialShape.bounds.x + 5}`);
  }

  stage = "Go-compatible update-file operation";
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

  stage = "Go get-file reload";
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
  }, null, 2);
}

createRoot(document.getElementById("app")!).render(<div id="p05-ready">P0.5 browser persistence test ready</div>);
void run().catch(fail);
