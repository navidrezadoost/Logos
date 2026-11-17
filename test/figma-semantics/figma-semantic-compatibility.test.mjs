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
