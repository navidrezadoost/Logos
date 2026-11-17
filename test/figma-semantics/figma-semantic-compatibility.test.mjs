import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const fixturePath = new URL("../fixtures/figma-semantics/supported-containment.json", import.meta.url);
const fixture = JSON.parse(await readFile(fixturePath, "utf8"));
const require = createRequire(new URL("../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
let server;
let useDocumentStore;
let useSelectionStore;
let createRect;
let hierarchyContract;
let buildSceneProjection;

before(async () => {
  server = await createServer({
    appType: "custom",
    configFile: false,
    logLevel: "error",
    optimizeDeps: { noDiscovery: true, include: [], entries: [] },
    root: appRoot,
    server: { middlewareMode: true, hmr: false },
  });
  ({ useDocumentStore } = await server.ssrLoadModule("/src/stores/documentStore.ts"));
  ({ useSelectionStore } = await server.ssrLoadModule("/src/stores/selectionStore.ts"));
  ({ createRect } = await server.ssrLoadModule("/src/types/shapes.ts"));
  hierarchyContract = await server.ssrLoadModule("/src/document-core/figmaHierarchyContract.ts");
  ({ buildSceneProjection } = await server.ssrLoadModule("/src/render-wasm/scene-projection.ts"));
});

after(async () => server?.close());

function hydrateFixture() {
  const shapes = Object.fromEntries(fixture.nodes.map((node) => {
    const shape = {
      ...createRect(node.id, node.name, node.bounds, "#123456"),
      type: node.type,
      parentId: node.parentId,
      children: [...node.children],
    };
    return [node.id, shape];
  }));
  const result = useDocumentStore.getState().hydrateDocument({
    pages: {
      [fixture.pageId]: {
        id: fixture.pageId,
        name: fixture.pageName,
        rootShapeIds: [...fixture.rootShapeIds],
      },
    },
    pageOrder: [fixture.pageId],
    currentPageId: fixture.pageId,
    shapes,
  });
  assert.equal(result.ok, true, result.ok ? "" : result.diagnostic.code);
  useSelectionStore.getState().clearSelection();
  return useDocumentStore.getState();
}

const insertedRectId = "00000000-0000-4000-8000-000000000107";

test("page-roots-and-ordered-containment", () => {
  const store = hydrateFixture();
  const rectRootId = fixture.nodes.find((node) => node.name === "Rectangle root").id;
  assert.deepEqual(store.pages[fixture.pageId].rootShapeIds, fixture.rootShapeIds);
  assert.equal(store.shapes[rectRootId].parentId, null);
  assert.deepEqual(store.shapes[fixture.rootShapeIds[0]].children, [fixture.nodes[1].id]);
  assert.deepEqual(store.shapes[fixture.rootShapeIds[1]].children, [fixture.nodes[3].id]);
  const hydratedProjection = buildSceneProjection(
    store.pages[fixture.pageId],
    store.shapes,
    store.pages[fixture.pageId].rootShapeIds,
  );
  assert.equal(hydratedProjection.ok, true);
  assert.deepEqual(hydratedProjection.plan.sourceRootIds, fixture.rootShapeIds);
  assert.deepEqual(hydratedProjection.plan.rendererRootChildrenIds, fixture.expectedRendererRootOrder);
  assert.deepEqual(
    hydratedProjection.plan.nodes.filter((node) => !node.synthetic).map((node) => node.id).sort(),
    fixture.nodes.map((node) => node.id).sort(),
  );
  const liveSnapshot = useDocumentStore.getState();
  const jsonRoundTrip = JSON.parse(JSON.stringify({
    pages: liveSnapshot.pages,
    pageOrder: liveSnapshot.pageOrder,
    currentPageId: liveSnapshot.currentPageId,
    shapes: liveSnapshot.shapes,
  }));
  const rehydrated = useDocumentStore.getState().hydrateDocument(jsonRoundTrip);
  assert.equal(rehydrated.ok, true, rehydrated.ok ? "" : rehydrated.diagnostic.code);
  const afterRoundTrip = useDocumentStore.getState();
  assert.deepEqual(afterRoundTrip.pageOrder, [fixture.pageId]);
  assert.deepEqual(afterRoundTrip.pages[fixture.pageId].rootShapeIds, fixture.rootShapeIds);
  for (const node of fixture.nodes) {
    assert.equal(afterRoundTrip.shapes[node.id].id, node.id);
    assert.equal(afterRoundTrip.shapes[node.id].parentId, node.parentId);
    assert.deepEqual(afterRoundTrip.shapes[node.id].children, node.children);
  }

  const root = {
    ...createRect("00000000-0000-4000-8000-000000000108", "Created frame", { x: 1, y: 2, w: 80, h: 60 }),
    type: "frame",
  };
  const created = useDocumentStore.getState().applyHierarchyOperation({ kind: "create-root", shape: root });
  assert.equal(created.ok, true);
  assert.equal(useDocumentStore.getState().pages[fixture.pageId].rootShapeIds[0], root.id);

  const nextRect = createRect("00000000-0000-4000-8000-000000000109", "Created rectangle", { x: 0, y: 0, w: 10, h: 10 });
  const createdRect = useDocumentStore.getState().applyHierarchyOperation({ kind: "create-root", shape: nextRect });
  assert.equal(createdRect.ok, true);
  assert.equal(useDocumentStore.getState().pages[fixture.pageId].rootShapeIds[0], nextRect.id);
  const nextPageId = useDocumentStore.getState().addPage("Second page");
  assert.deepEqual(useDocumentStore.getState().pageOrder, [fixture.pageId, nextPageId]);
});

test("frame-and-group-rectangle-children", () => {
  const store = hydrateFixture();
  const [frameId, groupId] = fixture.rootShapeIds;
  const frameChild = createRect(insertedRectId, "Inserted under frame", { x: 40, y: 20, w: 8, h: 8 });
  const frameResult = store.applyHierarchyOperation({ kind: "insert-child", parentId: frameId, shape: frameChild, index: 0 });
  assert.equal(frameResult.ok, true);
  assert.equal(useDocumentStore.getState().shapes[insertedRectId].parentId, frameId);
  assert.deepEqual(useDocumentStore.getState().shapes[frameId].children, [insertedRectId, fixture.nodes[1].id]);

  const groupChildId = "00000000-0000-4000-8000-000000000110";
  const groupChild = createRect(groupChildId, "Inserted under group", { x: 270, y: 20, w: 8, h: 8 });
  const groupResult = useDocumentStore.getState().applyHierarchyOperation({ kind: "insert-child", parentId: groupId, shape: groupChild, index: 1 });
  assert.equal(groupResult.ok, true);
  const final = useDocumentStore.getState();
  assert.equal(final.shapes[groupChildId].parentId, groupId);
  assert.deepEqual(final.shapes[groupId].children, [fixture.nodes[3].id, groupChildId]);
  assert.deepEqual(final.pages[fixture.pageId].rootShapeIds, fixture.rootShapeIds);
  assert.deepEqual(groupResult.projection.rendererRootChildrenIds, fixture.expectedRendererRootOrder);
});

test("unsupported-strict-child-kind-rejects-atomically", () => {
  const store = hydrateFixture();
  const before = useDocumentStore.getState();
  const selectedBefore = useSelectionStore.getState().selectedIds;
  const unsupportedFrame = {
    ...createRect("00000000-0000-4000-8000-000000000111", "Nested frame not in strict subset", { x: 0, y: 0, w: 20, h: 20 }),
    type: "frame",
  };
  const result = store.applyHierarchyOperation({ kind: "insert-child", parentId: fixture.rootShapeIds[0], shape: unsupportedFrame });
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_OPERATION_UNSUPPORTED_CHILD");
  assert.equal(useDocumentStore.getState().pages, before.pages);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.equal(useSelectionStore.getState().selectedIds, selectedBefore);
});

test("versioned-containment-and-order-contract-is-renderer-independent", () => {
  const contract = hierarchyContract.FIGMA_HIERARCHY_CONTRACT_V1;
  assert.equal(contract.version, 1);
  assert.deepEqual(contract.strictOperations, [
    "create-root", "insert-child", "rename", "delete", "reorder", "reorder-root-list", "reparent",
  ]);
  assert.equal(hierarchyContract.allowsStrictContainment("page", "frame"), true);
  assert.equal(hierarchyContract.allowsStrictContainment("page", "rect"), true);
  assert.equal(hierarchyContract.allowsStrictContainment("frame", "rect"), true);
  assert.equal(hierarchyContract.allowsStrictContainment("group", "rect"), true);
  assert.equal(hierarchyContract.allowsStrictContainment("rect", "rect"), false);
  assert.equal(hierarchyContract.allowsStrictContainment("frame", "frame"), false);
  assert.deepEqual(contract.unsupportedReferenceSemantics, ["section", "component-set", "instance"]);
  assert.match(contract.ordering.roots, /front-to-back/);
  assert.match(contract.ordering.children, /front-to-back/);
  assert.match(contract.ordering.renderer, /reverse once/);
});

test("strict-reorder-reparent-delete-preserve-membership-order-and-selection", () => {
  const store = hydrateFixture();
  const [frameId, groupId] = fixture.rootShapeIds;
  const originalFrameChild = fixture.nodes[1].id;
  const originalGroupChild = fixture.nodes[3].id;
  const secondFrameChildId = "00000000-0000-4000-8000-000000000112";
  const secondFrameChild = createRect(secondFrameChildId, "Second frame child", { x: 60, y: 20, w: 8, h: 8 });
  assert.equal(store.applyHierarchyOperation({ kind: "insert-child", parentId: frameId, shape: secondFrameChild, index: 1 }).ok, true);

  const reordered = useDocumentStore.getState().applyHierarchyOperation({ kind: "reorder", shapeId: secondFrameChildId, toIndex: 0 });
  assert.equal(reordered.ok, true);
  assert.deepEqual(useDocumentStore.getState().shapes[frameId].children, [secondFrameChildId, originalFrameChild]);

  const beforeRootIds = [...fixture.rootShapeIds];
  const rootOrder = [...beforeRootIds].reverse();
  const rootsReordered = useDocumentStore.getState().applyHierarchyOperation({ kind: "reorder-root-list", rootShapeIds: rootOrder });
  assert.equal(rootsReordered.ok, true);
  assert.deepEqual(useDocumentStore.getState().pages[fixture.pageId].rootShapeIds, rootOrder);

  const moved = useDocumentStore.getState().applyHierarchyOperation({
    kind: "reparent",
    shapeId: originalFrameChild,
    parentId: groupId,
    index: 1,
    preserveWorldPosition: true,
  });
  assert.equal(moved.ok, true, moved.ok ? "" : moved.diagnostic.code);
  let state = useDocumentStore.getState();
  assert.equal(state.shapes[originalFrameChild].id, originalFrameChild);
  assert.equal(state.shapes[originalFrameChild].parentId, groupId);
  assert.deepEqual(state.shapes[frameId].children, [secondFrameChildId]);
  assert.deepEqual(state.shapes[groupId].children, [originalGroupChild, originalFrameChild]);
  assert.deepEqual(state.pages[fixture.pageId].rootShapeIds, rootOrder);

  useSelectionStore.getState().select(originalGroupChild);
  const deleted = useDocumentStore.getState().applyHierarchyOperation({ kind: "delete", shapeId: originalGroupChild });
  assert.equal(deleted.ok, true);
  state = useDocumentStore.getState();
  assert.equal(state.shapes[originalGroupChild], undefined);
  assert.deepEqual(state.shapes[groupId].children, [originalFrameChild]);
  assert.deepEqual(useSelectionStore.getState().selectedIds, []);
});

test("strict-rename-commits-once-and-invalid-containment-leaves-all-state-unchanged", () => {
  const store = hydrateFixture();
  const rectId = fixture.nodes[1].id;
  const renamed = store.applyHierarchyOperation({ kind: "rename", shapeId: rectId, name: "Renamed child" });
  assert.equal(renamed.ok, true);
  assert.equal(useDocumentStore.getState().shapes[rectId].name, "Renamed child");

  const before = useDocumentStore.getState();
  useSelectionStore.getState().select(rectId);
  const selectedBefore = useSelectionStore.getState().selectedIds;
  const invalidChild = createRect("00000000-0000-4000-8000-000000000113", "Invalid leaf child", { x: 2, y: 2, w: 4, h: 4 });
  const rejected = useDocumentStore.getState().applyHierarchyOperation({ kind: "insert-child", parentId: rectId, shape: invalidChild });
  assert.equal(rejected.ok, false);
  assert.equal(rejected.diagnostic.code, "LOGOS_OPERATION_PARENT_NOT_CONTAINER");
  assert.equal(useDocumentStore.getState().pages, before.pages);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.deepEqual(useSelectionStore.getState().selectedIds, selectedBefore);
});
