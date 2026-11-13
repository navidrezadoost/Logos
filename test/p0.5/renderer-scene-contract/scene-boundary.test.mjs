import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";

const fixturePath = fileURLToPath(new URL("../../fixtures/renderer-scene-contract/ordered-hierarchy.json", import.meta.url));
const appRoot = fileURLToPath(new URL("../../../logos-app/", import.meta.url));
const require = createRequire(new URL("../../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
let viteServer;
let syncScene;
let UnsupportedRendererShapeError;
let selectCurrentPageShapes;
let useDocumentStore;

before(async () => {
  viteServer = await createServer({
    appType: "custom",
    configFile: false,
    logLevel: "error",
    optimizeDeps: { noDiscovery: true, include: [], entries: [] },
    root: appRoot,
    server: { middlewareMode: true },
  });
  ({ syncScene, UnsupportedRendererShapeError } = await viteServer.ssrLoadModule("/src/render-wasm/scene.ts"));
  ({ selectCurrentPageShapes, useDocumentStore } = await viteServer.ssrLoadModule("/src/stores/documentStore.ts"));
});

after(async () => {
  await viteServer?.close();
});

function validateFixtureReferences(scene) {
  const { rootShapeIds } = scene.page;
  const { shapes } = scene;
  const rootSet = new Set(rootShapeIds);
  if (rootSet.size !== rootShapeIds.length) throw new Error("duplicate page root reference");

  for (const id of rootShapeIds) {
    const shape = shapes[id];
    if (!shape) throw new Error(`page references missing root shape ${id}`);
    if (shape.parentId !== null) throw new Error(`root shape ${id} has parent ${shape.parentId}`);
  }

  for (const [shapeId, shape] of Object.entries(shapes)) {
    if (shape.id !== shapeId) throw new Error(`shape key/identity mismatch for ${shapeId}`);
    if (shape.parentId === null) {
      if (!rootSet.has(shapeId)) throw new Error(`parentless shape ${shapeId} is absent from page roots`);
    } else {
      const parent = shapes[shape.parentId];
      if (!parent) throw new Error(`shape ${shapeId} references missing parent ${shape.parentId}`);
      if (!parent.children.includes(shapeId)) {
        throw new Error(`parent ${parent.id} omits child reference ${shapeId}`);
      }
    }

    if (new Set(shape.children).size !== shape.children.length) {
      throw new Error(`shape ${shapeId} has duplicate child references`);
    }
    for (const childId of shape.children) {
      const child = shapes[childId];
      if (!child) throw new Error(`shape ${shapeId} references missing child ${childId}`);
      if (child.parentId !== shapeId) {
        throw new Error(`child ${childId} points to ${child.parentId}, not parent ${shapeId}`);
      }
    }
  }
  return scene;
}

function makeFakeModule() {
  const heap = new Uint8Array(4096);
  const calls = { resize: 0, pool: [], baseRecords: [], fills: 0, render: 0 };
  let cursor = 0;
  let allocationStart = 0;
  return {
    HEAPU8: heap,
    calls,
    _resize_viewbox() { calls.resize++; },
    _init_shapes_pool(capacity) { calls.pool.push(capacity); },
    _alloc_bytes(size) {
      allocationStart = cursor;
      heap.fill(0, allocationStart, allocationStart + size);
      cursor += size;
      return allocationStart;
    },
    _free_bytes() { cursor = allocationStart; },
    _set_shape_base_props() {
      calls.baseRecords.push(heap.slice(allocationStart, allocationStart + 104));
    },
    _set_shape_fills() { calls.fills++; },
    _render_sync() { calls.render++; },
  };
}

function rootsState(scene) {
  return {
    pages: { [scene.page.id]: scene.page },
    currentPageId: scene.page.id,
    shapes: scene.shapes,
  };
}

function uuidWords(record) {
  const view = new DataView(record.buffer, record.byteOffset, record.byteLength);
  return [0, 4, 8, 12].map((offset) => view.getUint32(offset, true));
}

function readFloat(record, offset) {
  return new DataView(record.buffer, record.byteOffset, record.byteLength).getFloat32(offset, true);
}

test("fixture makes root and child ordering and references explicit", () => {
  const scene = validateFixtureReferences(structuredClone(fixture));
  const [frameId, secondRootId] = scene.page.rootShapeIds;
  assert.deepEqual(scene.page.rootShapeIds, [frameId, secondRootId]);
  assert.deepEqual(scene.shapes[frameId].children, [
    "00000000-0000-4000-8000-000000000022",
    "00000000-0000-4000-8000-000000000021",
  ]);
  assert.equal(scene.shapes[scene.shapes[frameId].children[0]].parentId, frameId);
  assert.deepEqual(
    selectCurrentPageShapes(rootsState(scene)).map((shape) => shape.id),
    scene.page.rootShapeIds,
  );
});

test("contract fixture rejects a missing parent with both reference IDs", () => {
  const broken = structuredClone(fixture);
  const childId = broken.shapes[broken.page.rootShapeIds[0]].children[0];
  const missingParentId = "00000000-0000-4000-8000-000000000099";
  broken.shapes[childId].parentId = missingParentId;
  broken.shapes[broken.page.rootShapeIds[0]].children = broken.shapes[broken.page.rootShapeIds[0]].children
    .filter((id) => id !== childId);
  assert.throws(
    () => validateFixtureReferences(broken),
    new RegExp(`shape ${childId} references missing parent ${missingParentId}`),
  );
});

test("deleted root is absent from the current-page production projection", () => {
  const scene = structuredClone(fixture);
  const deletedId = scene.page.rootShapeIds[1];
  const before = useDocumentStore.getState();
  useDocumentStore.setState({
    pages: { [scene.page.id]: scene.page },
    pageOrder: [scene.page.id],
    currentPageId: scene.page.id,
    shapes: scene.shapes,
  });
  try {
    useDocumentStore.getState().removeShape(deletedId);
    const state = useDocumentStore.getState();
    assert.equal(state.shapes[deletedId], undefined);
    assert.deepEqual(selectCurrentPageShapes(state).map((shape) => shape.id), [scene.page.rootShapeIds[0]]);
  } finally {
    useDocumentStore.setState({
      pages: before.pages,
      pageOrder: before.pageOrder,
      currentPageId: before.currentPageId,
      shapes: before.shapes,
    });
  }
});

test("known rectangle uploads its UUID, Rust discriminator, geometry, and transform deterministically", () => {
  const rootShapes = selectCurrentPageShapes(rootsState(fixture));
  const rect = rootShapes[1];
  const module = makeFakeModule();

  syncScene(module, rootShapes, 320, 240);
  const firstGeneration = module.calls.baseRecords.map((record) => record.slice());
  syncScene(module, rootShapes, 320, 240);

  assert.deepEqual(module.calls.baseRecords.slice(0, 2), module.calls.baseRecords.slice(2));
  assert.deepEqual(module.calls.baseRecords.slice(0, 2).map(uuidWords), [
    [0, 0x4000, 0x80000000, 0x20],
    [0, 0x4000, 0x80000000, 0x30],
  ]);
  assert.deepEqual(firstGeneration, module.calls.baseRecords.slice(0, 2));
  const rectangleRecord = module.calls.baseRecords[1];
  assert.equal(rectangleRecord[32], 3, "Rust RawShapeType::Rect discriminant");
  assert.deepEqual([72, 76, 80, 84].map((offset) => readFloat(rectangleRecord, offset)), [180, 30, 210, 60]);
  assert.deepEqual([48, 52, 56, 60, 64, 68].map((offset) => readFloat(rectangleRecord, offset)), [1, 0, 0, 1, 0, 0]);
  assert.equal(module.calls.render, 2);
});

test("unsupported frontend discriminators fail before any live WASM mutation", () => {
  const known = structuredClone(fixture.shapes[fixture.page.rootShapeIds[1]]);
  for (const [index, discriminator] of ["vector-network", "component", "instance", "future-shape"].entries()) {
    const module = makeFakeModule();
    const unsupported = {
      ...known,
      id: `00000000-0000-4000-8000-00000000004${index}`,
      type: discriminator,
    };

    assert.throws(
      () => syncScene(module, [known, unsupported], 320, 240),
      (error) => error instanceof UnsupportedRendererShapeError
        && error.shapeId === unsupported.id
        && error.discriminator === discriminator
        && error.message.includes(unsupported.id)
        && error.message.includes(discriminator),
    );
    assert.equal(module.calls.resize, 0);
    assert.deepEqual(module.calls.pool, []);
    assert.deepEqual(module.calls.baseRecords, []);
    assert.equal(module.calls.fills, 0);
    assert.equal(module.calls.render, 0);
  }
});
