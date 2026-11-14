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
  const result = useDocumentStore.getState().hydrateDocument({
    pages: { [pageId]: { id: pageId, name: "Page", rootShapeIds: [frameAId, frameBId, rootRectId] } },
    pageOrder: [pageId], currentPageId: pageId,
    shapes: { [frameAId]: frameA, [frameBId]: frameB, [rootRectId]: rootRect, [childId]: child, [child2Id]: child2 },
  });
  assert.equal(result.ok, true, result.ok ? "" : result.diagnostic.code);
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

test("Vector Network creation uses the compatibility boundary and preserves geometry and visual data", () => {
  freshState();
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    const anchors = [{ x: 20, y: 20 }, { x: 60, y: 80 }];
    const segments = [{ s: 0, e: 1 }];
    const vectorId = useDocumentStore.getState().addVectorNetwork(anchors, segments, [], "#123456");
    const state = useDocumentStore.getState();
    const vector = state.shapes[vectorId];
    assert.equal(vector.type, "vector-network");
    assert.deepEqual(vector.vnAnchors, anchors);
    assert.deepEqual(vector.vnSegments, segments);
    assert.equal(vector.fills[0].color, "#123456");
    assert.equal(vector.parentId, null);
    assert.equal(state.pages[pageId].rootShapeIds[0], vectorId);
  } finally {
    console.warn = originalWarn;
  }
});

test("Vector Network replacement removes multiple roots atomically and preserves survivor order and selection", () => {
  freshState();
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    const state = useDocumentStore.getState();
    const first = state.addVectorNetwork([{ x: 0, y: 0 }, { x: 20, y: 20 }], [{ s: 0, e: 1 }]);
    const second = useDocumentStore.getState().addVectorNetwork([{ x: 40, y: 40 }, { x: 60, y: 60 }], [{ s: 0, e: 1 }]);
    const current = useDocumentStore.getState();
    const beforeRoots = [first, frameAId, second, frameBId, rootRectId];
    const hydration = current.hydrateDocument({
      pages: { [pageId]: { ...current.pages[pageId], rootShapeIds: beforeRoots } },
      pageOrder: [pageId], currentPageId: pageId,
      shapes: current.shapes,
    });
    assert.equal(hydration.ok, true);
    useSelectionStore.getState().select(first);
    useSelectionStore.getState().toggleSelect(second);
    const replacement = useDocumentStore.getState().replaceWithVectorNetwork(
      [first, second], [{ x: 5, y: 5 }, { x: 55, y: 55 }], [{ s: 0, e: 1 }], [[0]],
    );
    const after = useDocumentStore.getState();
    assert.equal(after.shapes[first], undefined);
    assert.equal(after.shapes[second], undefined);
    assert.equal(after.shapes[replacement].type, "vector-network");
    const expectedRoots = [];
    for (const id of beforeRoots) {
      if (id === first) expectedRoots.push(replacement);
      else if (id !== second) expectedRoots.push(id);
    }
    assert.deepEqual(after.pages[pageId].rootShapeIds, expectedRoots);
    assert.deepEqual(useSelectionStore.getState().selectedIds, [replacement]);
  } finally {
    console.warn = originalWarn;
  }
});

test("invalid Vector Network replacement is fully atomic", () => {
  freshState();
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    const first = useDocumentStore.getState().addVectorNetwork([{ x: 0, y: 0 }, { x: 1, y: 1 }], [{ s: 0, e: 1 }]);
    const before = useDocumentStore.getState();
    useSelectionStore.getState().select(first);
    const selectedBefore = useSelectionStore.getState().selectedIds;
    assert.throws(() => before.replaceWithVectorNetwork(
      [first, id("999")], [{ x: 0, y: 0 }, { x: 1, y: 1 }], [{ s: 0, e: 1 }],
    ), /LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN/);
    assert.equal(useDocumentStore.getState().shapes, before.shapes);
    assert.equal(useDocumentStore.getState().pages, before.pages);
    assert.equal(useSelectionStore.getState().selectedIds, selectedBefore);
  } finally {
    console.warn = originalWarn;
  }
});

test("compatibility addition rejects malformed membership and cross-page parents before commit", () => {
  const state = freshState();
  const otherPage = state.addPage("Other page");
  useDocumentStore.getState().setCurrentPage(otherPage);
  const foreignRoot = createRect(id("70"), "Foreign parent", { x: 0, y: 0, w: 200, h: 150 });
  const added = state.addShape(foreignRoot);
  assert.equal(added, foreignRoot.id);
  useDocumentStore.getState().setCurrentPage(pageId);
  const before = useDocumentStore.getState();
  const selectionBefore = useSelectionStore.getState().selectedIds;
  const child = { ...createRect(id("71"), "Cross-page child", { x: 1, y: 1, w: 5, h: 5 }), type: "vector-network", parentId: foreignRoot.id };
  assert.throws(() => useDocumentStore.getState().addShape(child), /LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS/);
  const malformed = { ...createRect(id("72"), "Malformed root", { x: 1, y: 1, w: 5, h: 5 }), children: [id("73")] };
  assert.throws(() => useDocumentStore.getState().addShape(malformed), /LOGOS_OPERATION_CREATE_MUST_BE_EMPTY_ROOT/);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useDocumentStore.getState().pages, before.pages);
  assert.equal(useSelectionStore.getState().selectedIds, selectionBefore);
  assert.ok(useDocumentStore.getState().pages[otherPage]);
});

test("compatibility additions preserve unsupported properties and return a renderer limitation instead of dropping data", () => {
  freshState();
  const legacy = {
    ...createRect(id("80"), "Legacy component", { x: 20, y: 20, w: 40, h: 30 }, "#112233"),
    type: "component",
    vendorExtension: { sentinel: "keep-exactly" },
    componentMeta: { properties: { mode: { kind: "variant", name: "Mode", values: ["a", "b"], defaultValue: "a" } } },
  };
  const result = useDocumentStore.getState().applyCompatibilityOperation({ kind: "compat-add-shape", shape: legacy });
  assert.equal(result.ok, true);
  assert.equal(result.renderingDiagnostic?.code, "LOGOS_SCENE_UNSUPPORTED_SHAPE");
  const stored = useDocumentStore.getState().shapes[legacy.id];
  assert.deepEqual(stored.vendorExtension, legacy.vendorExtension);
  assert.deepEqual(stored.componentMeta, legacy.componentMeta);
  assert.equal(stored.type, "component");
});

test("compatibility batch addition is atomic and resolves parent references independent of input order", () => {
  freshState();
  const parent = { ...createRect(id("81"), "Imported frame", { x: 0, y: 0, w: 100, h: 100 }), type: "frame", children: [] };
  const child = { ...createRect(id("82"), "Imported child", { x: 10, y: 10, w: 20, h: 20 }), parentId: parent.id };
  const added = useDocumentStore.getState().addShapes([child, parent]);
  assert.deepEqual(added, [child.id, parent.id]);
  assert.deepEqual(useDocumentStore.getState().shapes[parent.id].children, [child.id]);
  assert.equal(useDocumentStore.getState().shapes[child.id].parentId, parent.id);

  const before = useDocumentStore.getState();
  const badChild = { ...createRect(id("83"), "Dangling child", { x: 0, y: 0, w: 1, h: 1 }), parentId: id("84") };
  assert.throws(() => useDocumentStore.getState().addShapes([createRect(id("85"), "Would be a root", { x: 0, y: 0, w: 1, h: 1 }), badChild]), /LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN/);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useDocumentStore.getState().pages, before.pages);
});

test("vector replacement rejects a selected record owned by another page atomically", () => {
  const state = freshState();
  const vectorId = state.addVectorNetwork([{ x: 0, y: 0 }, { x: 2, y: 2 }], [{ s: 0, e: 1 }]);
  const otherPage = state.addPage("Other");
  useDocumentStore.setState((snapshot) => ({ pages: { ...snapshot.pages, [otherPage]: { ...snapshot.pages[otherPage], rootShapeIds: [vectorId] } } }));
  const before = useDocumentStore.getState();
  assert.throws(() => useDocumentStore.getState().replaceWithVectorNetwork([vectorId], [{ x: 0, y: 0 }, { x: 1, y: 1 }], [{ s: 0, e: 1 }]), /LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS/);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useDocumentStore.getState().pages, before.pages);
});

test("component promotion preserves hierarchy membership and keeps unsupported renderer behavior explicit", () => {
  freshState();
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    const before = useDocumentStore.getState();
    const snapshot = before.promoteToComponent(frameAId, { properties: {} });
    assert.equal(snapshot.type, "frame");
    const state = useDocumentStore.getState();
    assert.equal(state.shapes[frameAId].type, "component");
    assert.deepEqual(state.shapes[frameAId].children, [childId, child2Id]);
    assert.equal(state.shapes[childId].parentId, frameAId);
    assert.equal(state.pages[pageId].rootShapeIds[0], frameAId);
    assert.equal(useDocumentStore.getState().promoteToComponent(childId, { properties: {} }), null);
  } finally {
    console.warn = originalWarn;
  }
});

test("Vector Network replacement rejects referenced descendants without partial removal", () => {
  freshState();
  const originalWarn = console.warn;
  console.warn = () => {};
  try {
    const parentId = useDocumentStore.getState().addVectorNetwork([{ x: 1, y: 1 }, { x: 20, y: 20 }], [{ s: 0, e: 1 }]);
    const child = { ...createRect(id("88"), "Legacy child", { x: 3, y: 3, w: 5, h: 5 }), type: "component", parentId };
    const inserted = useDocumentStore.getState().applyCompatibilityOperation({ kind: "compat-add-shape", shape: child });
    assert.equal(inserted.ok, true);
    const before = useDocumentStore.getState();
    useSelectionStore.getState().select(parentId);
    const selectionBefore = useSelectionStore.getState().selectedIds;
    assert.throws(() => before.replaceWithVectorNetwork(
      [parentId], [{ x: 0, y: 0 }, { x: 1, y: 1 }], [{ s: 0, e: 1 }],
    ), /LOGOS_COMPAT_REPLACEMENT_HAS_REFERENCED_DESCENDANTS/);
    assert.equal(useDocumentStore.getState().shapes, before.shapes);
    assert.equal(useDocumentStore.getState().pages, before.pages);
    assert.deepEqual(useSelectionStore.getState().selectedIds, selectionBefore);
    assert.equal(useDocumentStore.getState().shapes[child.id].parentId, parentId);
  } finally {
    console.warn = originalWarn;
  }
});

test("subtree deletion removes every referenced descendant and reconciles selected IDs", () => {
  freshState();
  useSelectionStore.getState().select(frameAId);
  useSelectionStore.getState().toggleSelect(childId);
  useSelectionStore.getState().toggleSelect(child2Id);
  const result = useDocumentStore.getState().removeShape(frameAId);
  assert.equal(result.ok, true);
  const state = useDocumentStore.getState();
  assert.equal(state.shapes[frameAId], undefined);
  assert.equal(state.shapes[childId], undefined);
  assert.equal(state.shapes[child2Id], undefined);
  assert.deepEqual(state.pages[pageId].rootShapeIds, [frameBId, rootRectId]);
  assert.deepEqual(useSelectionStore.getState().selectedIds, []);
});

test("hydration preserves stable IDs and unknown fields and rejects malformed hierarchy atomically", () => {
  freshState();
  useSelectionStore.getState().select(childId);
  const current = useDocumentStore.getState();
  const page = { ...current.pages[pageId], rootShapeIds: [rootRectId, frameAId, frameBId] };
  const document = {
    pages: { [pageId]: page },
    pageOrder: [pageId],
    currentPageId: pageId,
    shapes: {
      ...current.shapes,
      [childId]: { ...current.shapes[childId], extension: { keep: "original" } },
      [rootRectId]: { ...current.shapes[rootRectId], type: "component", importedExtension: { sentinel: "keep" } },
    },
  };
  const accepted = useDocumentStore.getState().hydrateDocument(document);
  assert.equal(accepted.ok, true);
  assert.deepEqual(useDocumentStore.getState().pages[pageId].rootShapeIds, [rootRectId, frameAId, frameBId]);
  assert.deepEqual(useDocumentStore.getState().shapes[childId].extension, { keep: "original" });
  assert.equal(useDocumentStore.getState().shapes[rootRectId].id, rootRectId, "hydration rewrote a historical shape identity");
  assert.equal(useDocumentStore.getState().shapes[rootRectId].type, "component", "hydration rewrote an unsupported imported type");
  assert.deepEqual(useDocumentStore.getState().shapes[rootRectId].importedExtension, { sentinel: "keep" });
  assert.deepEqual(useSelectionStore.getState().selectedIds, [childId]);
  const before = useDocumentStore.getState();
  const bad = structuredClone(document);
  bad.shapes[frameAId].children = [id("999")];
  const rejected = useDocumentStore.getState().hydrateDocument(bad);
  assert.equal(rejected.ok, false);
  assert.equal(rejected.diagnostic.code, "LOGOS_SCENE_MISSING_CHILD");
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useDocumentStore.getState().pages, before.pages);
  assert.deepEqual(useSelectionStore.getState().selectedIds, [childId]);
});

test("identity and membership patches are rejected and batch value updates are atomic", () => {
  const state = freshState();
  const before = useDocumentStore.getState();
  assert.throws(() => state.updateShape(childId, { id: id("998") }), /LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY/);
  assert.throws(() => state.batchUpdate({ [childId]: { opacity: 0.5 }, [id("997")]: { opacity: 0.2 } }), /LOGOS_OPERATION_SHAPE_NOT_FOUND/);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
});
