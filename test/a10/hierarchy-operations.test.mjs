import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const require = createRequire(new URL("../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
let server;
let useDocumentStore;
let useSelectionStore;
let createRect;

before(async () => {
  server = await createServer({ appType: "custom", configFile: false, logLevel: "error", optimizeDeps: { noDiscovery: true, include: [], entries: [] }, root: appRoot, server: { middlewareMode: true } });
  ({ useDocumentStore } = await server.ssrLoadModule("/src/stores/documentStore.ts"));
  ({ useSelectionStore } = await server.ssrLoadModule("/src/stores/selectionStore.ts"));
  ({ createRect } = await server.ssrLoadModule("/src/types/shapes.ts"));
});

after(async () => server?.close());

const id = (suffix) => `00000000-0000-4000-8000-${suffix.padStart(12, "0")}`;
const pageId = id("1");
const frameAId = id("2");
const frameBId = id("3");
const rootRectId = id("4");
const childId = id("5");
const child2Id = id("6");

function freshState() {
  const frameA = { ...createRect(frameAId, "Frame A", { x: 10, y: 10, w: 180, h: 140 }, "#ffffff"), type: "frame", children: [childId, child2Id] };
  const frameB = { ...createRect(frameBId, "Frame B", { x: 0, y: 0, w: 500, h: 200 }, "#ffffff"), type: "frame" };
  const rootRect = createRect(rootRectId, "Root rect", { x: 400, y: 40, w: 60, h: 50 }, "#ff0000");
  const child = { ...createRect(childId, "Child", { x: 20, y: 20, w: 30, h: 30 }, "#00ff00"), parentId: frameAId };
  const child2 = { ...createRect(child2Id, "Child 2", { x: 60, y: 20, w: 30, h: 30 }, "#0000ff"), parentId: frameAId };
  useDocumentStore.setState({
    pages: { [pageId]: { id: pageId, name: "Page", rootShapeIds: [frameAId, frameBId, rootRectId] } },
    pageOrder: [pageId], currentPageId: pageId,
    shapes: { [frameAId]: frameA, [frameBId]: frameB, [rootRectId]: rootRect, [childId]: child, [child2Id]: child2 },
  });
  useSelectionStore.getState().clearSelection();
  return useDocumentStore.getState();
}

test("child insertion and create-root preserve stable IDs, parent symmetry and order", () => {
  freshState();
  const insertedId = id("7");
  const inserted = createRect(insertedId, "Inserted", { x: 30, y: 30, w: 20, h: 20 }, "#abcdef");
  const result = useDocumentStore.getState().applyHierarchyOperation({ kind: "insert-child", parentId: frameAId, shape: inserted, index: 1 });
  assert.equal(result.ok, true);
  const state = useDocumentStore.getState();
  assert.deepEqual(state.shapes[frameAId].children, [childId, insertedId, child2Id]);
  assert.equal(state.shapes[insertedId].parentId, frameAId);
  assert.deepEqual(state.pages[pageId].rootShapeIds, [frameAId, frameBId, rootRectId]);
  const rootId = id("8");
  const root = { ...createRect(rootId, "Root", { x: 1, y: 2, w: 10, h: 10 }, "#abcdef"), type: "frame" };
  assert.equal(useDocumentStore.getState().applyHierarchyOperation({ kind: "create-root", shape: root }).ok, true);
  assert.equal(useDocumentStore.getState().pages[pageId].rootShapeIds[0], rootId);
});

test("rename, sibling/root reorder and reparent are validated atomic operations", () => {
  freshState();
  const store = useDocumentStore.getState();
  assert.equal(store.applyHierarchyOperation({ kind: "rename", shapeId: childId, name: " Updated " }).ok, true);
  assert.equal(useDocumentStore.getState().shapes[childId].name, "Updated");
  assert.equal(useDocumentStore.getState().applyHierarchyOperation({ kind: "reorder", shapeId: child2Id, toIndex: 0 }).ok, true);
  assert.deepEqual(useDocumentStore.getState().shapes[frameAId].children, [child2Id, childId]);
  assert.equal(useDocumentStore.getState().applyHierarchyOperation({ kind: "reorder-root-list", rootShapeIds: [rootRectId, frameAId, frameBId] }).ok, true);
  const beforeBounds = { ...useDocumentStore.getState().shapes[childId].bounds };
  assert.equal(useDocumentStore.getState().applyHierarchyOperation({ kind: "reparent", shapeId: childId, parentId: frameBId, index: 0, preserveWorldPosition: true }).ok, true);
  const state = useDocumentStore.getState();
  assert.deepEqual(state.shapes[childId].bounds, beforeBounds);
  assert.equal(state.shapes[childId].parentId, frameBId);
  assert.deepEqual(state.shapes[frameAId].children, [child2Id]);
  assert.deepEqual(state.shapes[frameBId].children, [childId]);
  assert.deepEqual(state.pages[pageId].rootShapeIds, [rootRectId, frameAId, frameBId]);
});

test("cycle, invalid index, bad name, and unsupported transform reject without state mutation", () => {
  freshState();
  const before = useDocumentStore.getState();
  const selectionBefore = useSelectionStore.getState().selectedIds;
  const attempt = (operation, code) => {
    const result = useDocumentStore.getState().applyHierarchyOperation(operation);
    assert.equal(result.ok, false);
    assert.equal(result.diagnostic.code, code);
    assert.equal(useDocumentStore.getState().shapes, before.shapes);
    assert.equal(useDocumentStore.getState().pages, before.pages);
    assert.equal(useSelectionStore.getState().selectedIds, selectionBefore);
  };
  attempt({ kind: "reparent", shapeId: frameAId, parentId: childId, index: 0, preserveWorldPosition: true }, "LOGOS_OPERATION_CYCLE");
  attempt({ kind: "reorder", shapeId: childId, toIndex: 20 }, "LOGOS_OPERATION_INVALID_ORDER_INDEX");
  attempt({ kind: "rename", shapeId: childId, name: "\u0000bad" }, "LOGOS_OPERATION_INVALID_NAME");
  attempt({ kind: "reparent", shapeId: childId, parentId: frameBId, index: 0, preserveWorldPosition: false }, "LOGOS_OPERATION_WORLD_POSITION_REQUIRED");
  assert.throws(() => useDocumentStore.getState().updateShape(childId, { parentId: null }), /LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY/);
  assert.throws(() => useDocumentStore.getState().batchUpdate({ [childId]: { children: [] } }), /LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY/);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
});

test("subtree deletion removes descendants and reconciles selected descendants", () => {
  freshState();
  useSelectionStore.getState().select(childId);
  useSelectionStore.getState().toggleSelect(child2Id);
  assert.equal(useDocumentStore.getState().removeShape(frameAId).ok, true);
  const state = useDocumentStore.getState();
  assert.deepEqual(state.pages[pageId].rootShapeIds, [frameBId, rootRectId]);
  for (const removed of [frameAId, childId, child2Id]) assert.equal(state.shapes[removed], undefined);
  assert.deepEqual(useSelectionStore.getState().selectedIds, []);
});

test("page switching clears selection and selection reconciliation preserves in-page IDs", () => {
  freshState();
  useSelectionStore.getState().select(childId);
  useSelectionStore.getState().toggleSelect(child2Id);
  useDocumentStore.getState().applyHierarchyOperation({ kind: "rename", shapeId: childId, name: "In-page" });
  assert.deepEqual(useSelectionStore.getState().selectedIds, [childId, child2Id]);
  const otherPage = useDocumentStore.getState().addPage("Other");
  useDocumentStore.getState().setCurrentPage(otherPage);
  assert.deepEqual(useSelectionStore.getState().selectedIds, []);
});

test("locked shapes cannot participate in structural operations", () => {
  const state = freshState();
  state.updateShape(frameAId, { locked: true });
  const before = useDocumentStore.getState();
  const result = before.applyHierarchyOperation({ kind: "delete", shapeId: frameAId });
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_OPERATION_LOCKED_SHAPE");
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
});

test("ambiguous cross-page shape ownership rejects a mutation before commit", () => {
  const state = freshState();
  const otherPage = state.addPage("Ambiguous owner");
  useDocumentStore.setState((snapshot) => ({
    pages: { ...snapshot.pages, [otherPage]: { ...snapshot.pages[otherPage], rootShapeIds: [frameAId] } },
  }));
  const before = useDocumentStore.getState();
  const result = before.applyHierarchyOperation({ kind: "rename", shapeId: childId, name: "Must not commit" });
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS");
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useDocumentStore.getState().pages, before.pages);
});
