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

function assertGraphConsistent(state) {
  const reached = new Set();
  for (const pageId of state.pageOrder) {
    const page = state.pages[pageId];
    assert.ok(page, `pageOrder references missing page ${pageId}`);
    const local = new Set();
    const visit = (shapeId, parentId) => {
      const shape = state.shapes[shapeId];
      assert.ok(shape, `membership references missing shape ${shapeId}`);
      assert.equal(shape.parentId, parentId, `parent mismatch for ${shapeId}`);
      assert.equal(local.has(shapeId), false, `shape has duplicate membership: ${shapeId}`);
      assert.equal(reached.has(shapeId), false, `shape is owned by multiple pages: ${shapeId}`);
      local.add(shapeId);
      reached.add(shapeId);
      for (const childId of shape.children) visit(childId, shapeId);
    };
    for (const rootId of page.rootShapeIds) visit(rootId, null);
  }
  assert.deepEqual([...reached].sort(), Object.keys(state.shapes).sort(), "every shape is reachable from one page root");
  for (const shape of Object.values(state.shapes)) {
    if (shape.parentId !== null) assert.ok(state.shapes[shape.parentId].children.includes(shape.id));
  }
}

function observeDocumentPublications(run) {
  let publications = 0;
  const unsubscribe = useDocumentStore.subscribe((state, previous) => {
    if (state.pages !== previous.pages || state.shapes !== previous.shapes) publications += 1;
  });
  let result;
  try {
    result = run();
  } finally {
    unsubscribe();
  }
  return { publications, result };
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
  const frameRendererNode = frameResult.projection.nodes.find((node) => node.id === frameId);
  assert.deepEqual(frameRendererNode.rendererChildrenIds, [fixture.nodes[1].id, insertedRectId]);

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

test("strict-rename-and-invalid-containment-leaves-all-state-unchanged", () => {
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

test("contract-containment-pairs-match-the-actual-planner", () => {
  const cases = [
    { parent: "page", child: "frame", expected: true },
    { parent: "page", child: "rect", expected: true },
    { parent: "frame", child: "rect", expected: true },
    { parent: "group", child: "rect", expected: true },
    { parent: "page", child: "group", expected: false },
    { parent: "page", child: "circle", expected: false },
    { parent: "frame", child: "frame", expected: false },
    { parent: "group", child: "frame", expected: false },
    { parent: "rect", child: "rect", expected: false },
  ];
  const [frameId, groupId] = fixture.rootShapeIds;

  for (const [index, entry] of cases.entries()) {
    hydrateFixture();
    const childId = `00000000-0000-4000-8000-${String(200 + index).padStart(12, "0")}`;
    const bounds = entry.parent === "group" ? { x: 260, y: 20, w: 8, h: 8 } : { x: 20, y: 20, w: 8, h: 8 };
    const child = {
      ...createRect(childId, `Pair ${entry.parent} to ${entry.child}`, bounds),
      type: entry.child,
    };
    const allowedByContract = hierarchyContract.allowsStrictContainment(entry.parent, entry.child);
    assert.equal(allowedByContract, entry.expected, `contract pair ${entry.parent} → ${entry.child}`);
    useSelectionStore.getState().select(fixture.nodes[1].id);
    const before = useDocumentStore.getState();
    const selectedBefore = [...useSelectionStore.getState().selectedIds];
    const { publications, result } = observeDocumentPublications(() => {
      const state = useDocumentStore.getState();
      if (entry.parent === "page") return state.applyHierarchyOperation({ kind: "create-root", shape: child });
      const parentId = entry.parent === "frame" ? frameId : entry.parent === "group" ? groupId : fixture.nodes[1].id;
      return state.applyHierarchyOperation({ kind: "insert-child", parentId, shape: child });
    });
    assert.equal(result.ok, entry.expected, `planner pair ${entry.parent} → ${entry.child}${result.ok ? "" : ` (${result.diagnostic.code})`}`);
    assert.equal(publications, entry.expected ? 1 : 0, `publication count for ${entry.parent} → ${entry.child}`);
    if (!entry.expected) {
      assert.equal(useDocumentStore.getState().pages, before.pages);
      assert.equal(useDocumentStore.getState().shapes, before.shapes);
      assert.deepEqual(useSelectionStore.getState().selectedIds, selectedBefore);
    }
    assertGraphConsistent(useDocumentStore.getState());
  }
});

test("all-seven-strict-operations-publish-once-and-retain-a-valid-ordered-graph", () => {
  const frameId = fixture.rootShapeIds[0];
  const groupId = fixture.rootShapeIds[1];
  const originalFrameChild = fixture.nodes[1].id;
  const originalGroupChild = fixture.nodes[3].id;
  const scenarios = [
    {
      name: "create-root",
      prepare: () => hydrateFixture(),
      operation: () => ({
        kind: "create-root",
        shape: createRect("00000000-0000-4000-8000-000000000121", "Created root", { x: 500, y: 0, w: 12, h: 12 }),
      }),
    },
    {
      name: "insert-child",
      prepare: () => hydrateFixture(),
      operation: () => ({
        kind: "insert-child",
        parentId: frameId,
        shape: createRect("00000000-0000-4000-8000-000000000122", "Inserted child", { x: 50, y: 20, w: 8, h: 8 }),
        index: 0,
      }),
    },
    {
      name: "rename",
      prepare: () => hydrateFixture(),
      operation: () => ({ kind: "rename", shapeId: originalFrameChild, name: "Renamed" }),
    },
    {
      name: "delete",
      prepare: () => hydrateFixture(),
      operation: () => ({ kind: "delete", shapeId: frameId }),
      before: () => useSelectionStore.getState().select(originalFrameChild),
    },
    {
      name: "reorder",
      prepare: () => {
        const state = hydrateFixture();
        state.applyHierarchyOperation({
          kind: "insert-child",
          parentId: frameId,
          shape: createRect("00000000-0000-4000-8000-000000000123", "Second child", { x: 70, y: 20, w: 8, h: 8 }),
          index: 1,
        });
      },
      operation: () => ({ kind: "reorder", shapeId: "00000000-0000-4000-8000-000000000123", toIndex: 0 }),
    },
    {
      name: "reorder-root-list",
      prepare: () => hydrateFixture(),
      operation: () => ({ kind: "reorder-root-list", rootShapeIds: [...fixture.rootShapeIds].reverse() }),
    },
    {
      name: "reparent",
      prepare: () => hydrateFixture(),
      operation: () => ({ kind: "reparent", shapeId: originalFrameChild, parentId: groupId, index: 1, preserveWorldPosition: true }),
    },
  ];

  for (const scenario of scenarios) {
    scenario.prepare();
    scenario.before?.();
    const before = useDocumentStore.getState();
    const expectedIds = Object.keys(before.shapes).sort();
    const { publications, result } = observeDocumentPublications(() =>
      useDocumentStore.getState().applyHierarchyOperation(scenario.operation()),
    );
    assert.equal(result.ok, true, `${scenario.name}: ${result.ok ? "" : result.diagnostic.code}`);
    assert.equal(publications, 1, `${scenario.name} must publish exactly one document change`);
    assertGraphConsistent(useDocumentStore.getState());
    const after = useDocumentStore.getState();
    for (const id of expectedIds) {
      if (scenario.name === "delete" && (id === frameId || id === originalFrameChild)) continue;
      assert.equal(after.shapes[id]?.id, id, `${scenario.name} preserves existing ID ${id}`);
    }
    if (scenario.name === "reorder-root-list") assert.deepEqual(after.pages[fixture.pageId].rootShapeIds, [...fixture.rootShapeIds].reverse());
    if (scenario.name === "reorder") assert.deepEqual(after.shapes[frameId].children, ["00000000-0000-4000-8000-000000000123", originalFrameChild]);
    if (scenario.name === "reparent") {
      assert.deepEqual(after.shapes[frameId].children, []);
      assert.deepEqual(after.shapes[groupId].children, [originalGroupChild, originalFrameChild]);
    }
    if (scenario.name === "delete") {
      assert.equal(after.shapes[frameId], undefined);
      assert.equal(after.shapes[originalFrameChild], undefined);
      assert.deepEqual(after.pages[fixture.pageId].rootShapeIds, fixture.rootShapeIds.slice(1));
      assert.deepEqual(useSelectionStore.getState().selectedIds, []);
    }
  }
});

test("rejected-strict-operation-publishes-nothing-and-preserves-document-and-selection", () => {
  const store = hydrateFixture();
  const child = fixture.nodes[1];
  useSelectionStore.getState().select(child.id);
  const before = useDocumentStore.getState();
  const selectedBefore = [...useSelectionStore.getState().selectedIds];
  const rejectedOperation = {
    kind: "reparent",
    shapeId: fixture.rootShapeIds[0],
    parentId: child.id,
    index: 0,
    preserveWorldPosition: true,
  };
  const { publications, result } = observeDocumentPublications(() => store.applyHierarchyOperation(rejectedOperation));
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_OPERATION_CYCLE");
  assert.equal(publications, 0);
  assert.equal(useDocumentStore.getState().pages, before.pages);
  assert.equal(useDocumentStore.getState().shapes, before.shapes);
  assert.deepEqual(useSelectionStore.getState().selectedIds, selectedBefore);

  const frameId = fixture.rootShapeIds[0];
  useDocumentStore.getState().updateShape(frameId, { locked: true });
  useSelectionStore.getState().select(child.id);
  const lockedBefore = useDocumentStore.getState();
  const lockedSelection = [...useSelectionStore.getState().selectedIds];
  const lockedAttempt = observeDocumentPublications(() =>
    useDocumentStore.getState().applyHierarchyOperation({ kind: "delete", shapeId: frameId }),
  );
  assert.equal(lockedAttempt.result.ok, false);
  assert.equal(lockedAttempt.result.diagnostic.code, "LOGOS_OPERATION_LOCKED_SHAPE");
  assert.equal(lockedAttempt.publications, 0);
  assert.equal(useDocumentStore.getState().pages, lockedBefore.pages);
  assert.equal(useDocumentStore.getState().shapes, lockedBefore.shapes);
  assert.deepEqual(useSelectionStore.getState().selectedIds, lockedSelection);

  const otherPageId = useDocumentStore.getState().addPage("Ambiguous ownership page");
  useDocumentStore.setState((state) => ({
    pages: { ...state.pages, [otherPageId]: { ...state.pages[otherPageId], rootShapeIds: [frameId] } },
  }));
  const ambiguousBefore = useDocumentStore.getState();
  const ambiguousSelection = [...useSelectionStore.getState().selectedIds];
  const ambiguousAttempt = observeDocumentPublications(() =>
    useDocumentStore.getState().applyHierarchyOperation({ kind: "rename", shapeId: child.id, name: "Must reject" }),
  );
  assert.equal(ambiguousAttempt.result.ok, false);
  assert.equal(ambiguousAttempt.result.diagnostic.code, "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS");
  assert.equal(ambiguousAttempt.publications, 0);
  assert.equal(useDocumentStore.getState().pages, ambiguousBefore.pages);
  assert.equal(useDocumentStore.getState().shapes, ambiguousBefore.shapes);
  assert.deepEqual(useSelectionStore.getState().selectedIds, ambiguousSelection);
});
