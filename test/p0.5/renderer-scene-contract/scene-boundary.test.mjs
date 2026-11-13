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
let syncScene2D;
let UnsupportedRendererShapeError;
let UnsupportedCanvas2DShapeError;
let rendererFailureStatus;
let buildSceneProjection;
let NIL_RENDERER_ROOT_ID;
let projectAndSyncScene;
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
  ({ syncScene, syncScene2D, UnsupportedRendererShapeError, UnsupportedCanvas2DShapeError } =
    await viteServer.ssrLoadModule("/src/render-wasm/scene.ts"));
  ({ rendererFailureStatus } = await viteServer.ssrLoadModule("/src/render-wasm/renderer-status.ts"));
  ({ buildSceneProjection, NIL_RENDERER_ROOT_ID } = await viteServer.ssrLoadModule("/src/render-wasm/scene-projection.ts"));
  ({ projectAndSyncScene } =
    await viteServer.ssrLoadModule("/src/render-wasm/scene.ts"));
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

function makeProjectedFakeModule() {
  const heap = new Uint8Array(1 << 16);
  const calls = {
    resize: 0,
    begin: 0,
    base: 0,
    fills: 0,
    use: 0,
    children: 0,
    renderAttempts: 0,
    rendered: [],
    generations: [],
    active: null,
    partialTarget: null,
  };
  let cursor = 0;
  let allocationStart = 0;
  let currentShapeId = null;
  let failures = [];

  function failAfter(stage) {
    if (failures[0] !== stage) return;
    failures.shift();
    throw new Error(`injected WASM boundary failure at ${stage}`);
  }

  function readUuid(view, offset) {
    const hex = [0, 4, 8, 12]
      .map((part) => view.getUint32(offset + part, true).toString(16).padStart(8, "0"))
      .join("");
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }

  return {
    HEAPU8: heap,
    calls,
    setFailureSequence(stages) { failures = [...stages]; },
    _resize_viewbox(width, height) {
      calls.resize++;
      calls.dimensions = [width, height];
      failAfter("resize");
    },
    _begin_scene_upload(capacity) {
      calls.begin++;
      calls.active = { capacity, base: [], children: [] };
      calls.generations.push(calls.active);
      currentShapeId = null;
      cursor = 0;
      failAfter("begin");
    },
    _alloc_bytes(size) {
      failAfter("alloc");
      allocationStart = cursor;
      heap.fill(0, allocationStart, allocationStart + size);
      cursor += size;
      return allocationStart;
    },
    _free_bytes() { cursor = allocationStart; },
    _set_shape_base_props() {
      calls.base++;
      const view = new DataView(heap.buffer, allocationStart, 104);
      calls.active.base.push({
        id: readUuid(view, 0),
        parentId: readUuid(view, 16),
        type: view.getUint8(32),
        bytes: Array.from(heap.slice(allocationStart, allocationStart + 104)),
      });
      failAfter("base");
    },
    _set_shape_fills() {
      calls.fills++;
      failAfter("fills");
    },
    _use_shape(a, b, c, d) {
      calls.use++;
      const hex = [a, b, c, d].map((word) => word.toString(16).padStart(8, "0")).join("");
      currentShapeId = `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
      failAfter("use");
    },
    _set_children_0() {
      calls.children++;
      calls.active.children.push({ parentId: currentShapeId, ids: [] });
      failAfter("children");
    },
    _set_children() {
      calls.children++;
      const bytes = cursor - allocationStart;
      const view = new DataView(heap.buffer, allocationStart, bytes);
      const ids = [];
      for (let offset = 0; offset < bytes; offset += 16) ids.push(readUuid(view, offset));
      calls.active.children.push({ parentId: currentShapeId, ids });
      failAfter("children");
      cursor = allocationStart;
    },
    _render_sync() {
      calls.renderAttempts++;
      calls.partialTarget = structuredClone(calls.active);
      failAfter("render");
      calls.rendered.push(structuredClone(calls.active));
    },
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

function makeCanvas2DRecorder() {
  const calls = [];
  const record = (name) => (...args) => calls.push([name, ...args]);
  return {
    calls,
    clearRect: record("clearRect"),
    fillRect: record("fillRect"),
    beginPath: record("beginPath"),
    ellipse: record("ellipse"),
    fill: record("fill"),
    stroke: record("stroke"),
    moveTo: record("moveTo"),
    lineTo: record("lineTo"),
    bezierCurveTo: record("bezierCurveTo"),
  };
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

test("Canvas 2D rejects unsupported shapes before clearing or drawing", () => {
  const known = fixture.shapes[fixture.page.rootShapeIds[1]];
  const unsupportedTypes = ["component", "instance", "path", "text", "group", "bool", "svg-raw", "future-shape"];
  for (const [index, discriminator] of unsupportedTypes.entries()) {
    const ctx = makeCanvas2DRecorder();
    const shape = {
      ...known,
      id: `00000000-0000-4000-8000-00000000005${index}`,
      type: discriminator,
    };
    assert.throws(
      () => syncScene2D(ctx, [shape], 320, 240),
      (error) => error instanceof UnsupportedCanvas2DShapeError
        && error.shapeId === shape.id
        && error.discriminator === discriminator,
    );
    assert.deepEqual(ctx.calls, [], `${discriminator} mutated the canvas before rejection`);
  }
});

test("Canvas 2D and Rust fail closed for component, instance, and future types in mixed scenes", () => {
  const known = structuredClone(fixture.shapes[fixture.page.rootShapeIds[1]]);
  for (const [index, discriminator] of ["component", "instance", "future-shape"].entries()) {
    const unsupported = {
      ...known,
      id: `00000000-0000-4000-8000-00000000006${index}`,
      type: discriminator,
    };

    const context = makeCanvas2DRecorder();
    assert.throws(
      () => syncScene2D(context, [known, unsupported], 320, 240),
      (error) => error instanceof UnsupportedCanvas2DShapeError
        && error.shapeId === unsupported.id
        && error.discriminator === discriminator,
    );
    assert.deepEqual(context.calls, [], `${discriminator} partially mutated Canvas 2D`);

    const module = makeFakeModule();
    assert.throws(
      () => syncScene(module, [known, unsupported], 320, 240),
      (error) => error instanceof UnsupportedRendererShapeError
        && error.shapeId === unsupported.id
        && error.discriminator === discriminator,
    );
    assert.equal(module.calls.resize, 0);
    assert.deepEqual(module.calls.pool, []);
    assert.deepEqual(module.calls.baseRecords, []);
    assert.equal(module.calls.fills, 0);
    assert.equal(module.calls.render, 0);
  }
});

test("Canvas 2D keeps explicit ellipse and vector-network drawing paths", () => {
  const ellipseContext = makeCanvas2DRecorder();
  const ellipse = {
    ...fixture.shapes[fixture.page.rootShapeIds[1]],
    type: "ellipse",
  };
  syncScene2D(ellipseContext, [ellipse], 320, 240);
  assert.equal(ellipseContext.calls.filter(([name]) => name === "ellipse").length, 1);
  assert.deepEqual(ellipseContext.calls.filter(([name]) => name === "fillRect"), [
    ["fillRect", 0, 0, 320, 240],
  ], "ellipse was approximated with a shape-sized rectangle");

  const vectorContext = makeCanvas2DRecorder();
  const vector = {
    ...fixture.shapes[fixture.page.rootShapeIds[1]],
    type: "vector-network",
    vnAnchors: [{ x: 10, y: 10 }, { x: 25, y: 25 }],
    vnSegments: [{ s: 0, e: 1 }],
  };
  syncScene2D(vectorContext, [vector], 320, 240);
  assert.ok(vectorContext.calls.some(([name]) => name === "moveTo"));
  assert.ok(vectorContext.calls.some(([name]) => name === "lineTo"));
  assert.ok(vectorContext.calls.some(([name]) => name === "stroke"));
  assert.deepEqual(vectorContext.calls.filter(([name]) => name === "fillRect"), [
    ["fillRect", 0, 0, 320, 240],
  ], "vector-network was approximated with a shape-sized rectangle");
});

test("renderer failures map to stable editor statuses without leaking raw engine messages", () => {
  const unsupported = rendererFailureStatus(
    new UnsupportedCanvas2DShapeError("shape-123", "component"),
    "wasm",
    true,
  );
  assert.deepEqual(unsupported, {
    state: "UNSUPPORTED_SHAPE",
    backend: "canvas2d",
    code: "LOGOS_RENDER_UNSUPPORTED_SHAPE",
    shapeId: "shape-123",
    shapeType: "component",
    frameDisposition: "previous",
  });

  const failure = rendererFailureStatus(new Error("private wasm stack payload"), "wasm", true);
  assert.deepEqual(failure, {
    state: "RENDER_FAILED",
    backend: "wasm",
    code: "LOGOS_RENDER_FAILED",
    frameDisposition: "unverified",
  });
  assert.equal(JSON.stringify(failure).includes("private wasm stack payload"), false);
});

function projectionFailure(scene, code, records = scene.shapes, roots = scene.page.rootShapeIds) {
  const result = buildSceneProjection(scene.page, records, roots);
  assert.equal(result.ok, false, `expected projection failure ${code}`);
  assert.equal(result.diagnostic.code, code, JSON.stringify(result.diagnostic));
  return result.diagnostic;
}

test("pure projection emits a renderer-only root and deterministic back-to-front child order", () => {
  const source = structuredClone(fixture);
  const before = structuredClone(source);
  const result = buildSceneProjection(source.page, source.shapes, source.page.rootShapeIds);
  assert.equal(result.ok, true);
  const { plan } = result;

  assert.equal(plan.rootId, NIL_RENDERER_ROOT_ID);
  assert.equal(plan.nodes[0].synthetic, true);
  assert.equal(plan.nodes[0].shape.type, "frame");
  assert.equal(plan.nodes[0].shape.id, NIL_RENDERER_ROOT_ID);
  assert.deepEqual(plan.sourceRootIds, source.page.rootShapeIds);
  assert.deepEqual(plan.rendererRootChildrenIds, [...source.page.rootShapeIds].reverse());
  assert.deepEqual(plan.nodes.map(({ id }) => id), [
    NIL_RENDERER_ROOT_ID,
    "00000000-0000-4000-8000-000000000020",
    "00000000-0000-4000-8000-000000000022",
    "00000000-0000-4000-8000-000000000021",
    "00000000-0000-4000-8000-000000000030",
  ]);
  assert.deepEqual(plan.nodes[1].rendererChildrenIds, [
    "00000000-0000-4000-8000-000000000021",
    "00000000-0000-4000-8000-000000000022",
  ]);
  assert.equal(Object.hasOwn(source.shapes, NIL_RENDERER_ROOT_ID), false, "synthetic root leaked into document records");
  assert.deepEqual(source, before, "projection mutated its inputs");
});

test("pure projection rejects graph, identity, root, cycle, orphan, and nested capability errors", () => {
  projectionFailure(fixture, "LOGOS_SCENE_ROOT_ORDER_MISMATCH", fixture.shapes, [...fixture.page.rootShapeIds].reverse());

  const missingRoot = structuredClone(fixture);
  missingRoot.page.rootShapeIds.push("00000000-0000-4000-8000-000000000099");
  projectionFailure(missingRoot, "LOGOS_SCENE_MISSING_ROOT");

  const duplicateRoot = structuredClone(fixture);
  duplicateRoot.page.rootShapeIds.push(duplicateRoot.page.rootShapeIds[0]);
  projectionFailure(duplicateRoot, "LOGOS_SCENE_DUPLICATE_ROOT");

  const missingParent = structuredClone(fixture);
  const childId = missingParent.page.rootShapeIds[0] === "00000000-0000-4000-8000-000000000020"
    ? "00000000-0000-4000-8000-000000000022"
    : "00000000-0000-4000-8000-000000000021";
  missingParent.shapes[childId].parentId = "00000000-0000-4000-8000-000000000099";
  missingParent.shapes["00000000-0000-4000-8000-000000000020"].children =
    missingParent.shapes["00000000-0000-4000-8000-000000000020"].children.filter((id) => id !== childId);
  projectionFailure(missingParent, "LOGOS_SCENE_MISSING_PARENT");

  const missingChild = structuredClone(fixture);
  missingChild.shapes["00000000-0000-4000-8000-000000000020"].children.push("00000000-0000-4000-8000-000000000099");
  projectionFailure(missingChild, "LOGOS_SCENE_MISSING_CHILD");

  const duplicateChild = structuredClone(fixture);
  const children = duplicateChild.shapes["00000000-0000-4000-8000-000000000020"].children;
  children.push(children[0]);
  projectionFailure(duplicateChild, "LOGOS_SCENE_DUPLICATE_CHILD");

  const twoOwners = structuredClone(fixture);
  twoOwners.shapes["00000000-0000-4000-8000-000000000030"].children = [
    twoOwners.shapes["00000000-0000-4000-8000-000000000020"].children[0],
  ];
  projectionFailure(twoOwners, "LOGOS_SCENE_DUPLICATE_CHILD_MEMBERSHIP");

  const backlinkMismatch = structuredClone(fixture);
  const mismatchId = backlinkMismatch.shapes["00000000-0000-4000-8000-000000000020"].children[0];
  backlinkMismatch.shapes[mismatchId].parentId = "00000000-0000-4000-8000-000000000030";
  backlinkMismatch.shapes["00000000-0000-4000-8000-000000000020"].children =
    backlinkMismatch.shapes["00000000-0000-4000-8000-000000000020"].children.filter((id) => id !== mismatchId);
  projectionFailure(backlinkMismatch, "LOGOS_SCENE_PARENT_CHILD_MISMATCH");

  const cycle = structuredClone(fixture);
  const rootId = cycle.page.rootShapeIds[0];
  const first = "00000000-0000-4000-8000-000000000021";
  const second = "00000000-0000-4000-8000-000000000022";
  cycle.shapes[rootId].children = [];
  cycle.shapes[first].parentId = second;
  cycle.shapes[first].children = [second];
  cycle.shapes[second].parentId = first;
  cycle.shapes[second].children = [first];
  projectionFailure(cycle, "LOGOS_SCENE_CYCLE");

  const orphan = structuredClone(fixture);
  const orphanId = "00000000-0000-4000-8000-000000000040";
  orphan.shapes[orphanId] = { ...orphan.shapes["00000000-0000-4000-8000-000000000030"], id: orphanId, name: "Orphan" };
  projectionFailure(orphan, "LOGOS_SCENE_ORPHAN_NODE");

  const nilCollision = structuredClone(fixture);
  nilCollision.shapes[NIL_RENDERER_ROOT_ID] = {
    ...nilCollision.shapes[nilCollision.page.rootShapeIds[0]],
    id: NIL_RENDERER_ROOT_ID,
  };
  projectionFailure(nilCollision, "LOGOS_SCENE_SYNTHETIC_ROOT_COLLISION");

  const duplicateId = [
    ...Object.values(fixture.shapes),
    structuredClone(fixture.shapes[fixture.page.rootShapeIds[0]]),
  ];
  projectionFailure(fixture, "LOGOS_SCENE_DUPLICATE_SHAPE_ID", duplicateId);

  const caseFoldId = "abcdefab-cdef-4abc-8def-abcdefabcdef";
  const caseFoldShape = { ...fixture.shapes[fixture.page.rootShapeIds[0]], id: caseFoldId };
  projectionFailure(fixture, "LOGOS_SCENE_DUPLICATE_SHAPE_ID", [
    caseFoldShape,
    { ...caseFoldShape, id: caseFoldId.toUpperCase() },
  ]);

  const unsupportedNested = structuredClone(fixture);
  const nestedId = unsupportedNested.shapes["00000000-0000-4000-8000-000000000020"].children[0];
  unsupportedNested.shapes[nestedId].type = "component";
  projectionFailure(unsupportedNested, "LOGOS_SCENE_UNSUPPORTED_SHAPE");
});

test("invalid projection returns before any WASM mutation", () => {
  const module = makeProjectedFakeModule();
  const broken = structuredClone(fixture);
  broken.shapes["00000000-0000-4000-8000-000000000020"].children.push("00000000-0000-4000-8000-000000000099");

  const result = projectAndSyncScene(module, broken.page, broken.shapes, broken.page.rootShapeIds, 320, 240);
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_SCENE_MISSING_CHILD");
  assert.equal(module.calls.resize, 0);
  assert.equal(module.calls.begin, 0);
  assert.equal(module.calls.base, 0);
  assert.equal(module.calls.children, 0);
  assert.equal(module.calls.renderAttempts, 0);
});

test("root and descendant records plus ordered child vectors serialize before render", () => {
  const module = makeProjectedFakeModule();
  const result = projectAndSyncScene(module, fixture.page, fixture.shapes, fixture.page.rootShapeIds, 320, 240);
  assert.equal(result.ok, true);
  const renderedGeneration = module.calls.rendered[0];
  const ids = renderedGeneration.base.map((record) => record.id);
  assert.deepEqual(ids, [
    NIL_RENDERER_ROOT_ID,
    "00000000-0000-4000-8000-000000000020",
    "00000000-0000-4000-8000-000000000022",
    "00000000-0000-4000-8000-000000000021",
    "00000000-0000-4000-8000-000000000030",
  ]);
  assert.equal(renderedGeneration.capacity, ids.length);
  assert.equal(renderedGeneration.base[0].type, 0, "synthetic root must use the neutral Frame discriminant");
  assert.deepEqual(renderedGeneration.children[0], {
    parentId: NIL_RENDERER_ROOT_ID,
    ids: ["00000000-0000-4000-8000-000000000030", "00000000-0000-4000-8000-000000000020"],
  });
  assert.deepEqual(renderedGeneration.children[1], {
    parentId: "00000000-0000-4000-8000-000000000020",
    ids: ["00000000-0000-4000-8000-000000000021", "00000000-0000-4000-8000-000000000022"],
  });
  assert.equal(module.calls.renderAttempts, 1, "renderer ran before complete node/link serialization");
});

test("interrupted projected uploads restore the last committed scene and are retryable", () => {
  const edited = structuredClone(fixture);
  edited.shapes["00000000-0000-4000-8000-000000000021"].bounds.x += 7;

  for (const stage of ["resize", "begin", "alloc", "base", "fills", "use", "children", "render"]) {
    const module = makeProjectedFakeModule();
    const initial = projectAndSyncScene(module, fixture.page, fixture.shapes, fixture.page.rootShapeIds, 320, 240);
    assert.equal(initial.ok, true, `initial ${stage} fixture upload`);
    const committedFrame = structuredClone(module.calls.rendered.at(-1));

    module.setFailureSequence([stage]);
    const interrupted = projectAndSyncScene(module, edited.page, edited.shapes, edited.page.rootShapeIds, 320, 240);
    assert.equal(interrupted.ok, false, `${stage} failure was reported as successful`);
    assert.equal(interrupted.diagnostic.code, "LOGOS_SCENE_UPLOAD_INTERRUPTED");
    assert.equal(interrupted.diagnostic.recovered, true, `${stage} failure did not report compensated recovery`);
    assert.deepEqual(module.calls.rendered.at(-1), committedFrame, `${stage} failure exposed a partial scene`);
    assert.equal(module.calls.rendered.length, 2, `${stage} failure rendered the partial candidate`);

    const successfulRetry = projectAndSyncScene(module, edited.page, edited.shapes, edited.page.rootShapeIds, 320, 240);
    assert.equal(successfulRetry.ok, true, `${stage} retry did not recover`);
    const committedNewFrame = structuredClone(module.calls.rendered.at(-1));
    const repeatedSync = projectAndSyncScene(module, edited.page, edited.shapes, edited.page.rootShapeIds, 320, 240);
    assert.equal(repeatedSync.ok, true, `${stage} repeated sync failed`);
    assert.deepEqual(module.calls.rendered.at(-1), committedNewFrame, `${stage} retry was not idempotent`);
  }
});

test("an interrupted first upload recovers to an empty synthetic-root scene", () => {
  const module = makeProjectedFakeModule();
  module.setFailureSequence(["base"]);
  const result = projectAndSyncScene(module, fixture.page, fixture.shapes, fixture.page.rootShapeIds, 320, 240);
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "LOGOS_SCENE_UPLOAD_INTERRUPTED");
  assert.equal(result.diagnostic.recovered, true);
  assert.equal(module.calls.rendered.length, 1, "the failed candidate was rendered");
  assert.deepEqual(module.calls.rendered[0].base.map((record) => record.id), [NIL_RENDERER_ROOT_ID]);
  assert.deepEqual(module.calls.rendered[0].children, [{ parentId: NIL_RENDERER_ROOT_ID, ids: [] }]);
});

test("failed compensation poisons the renderer module and blocks further mutations", () => {
  const edited = structuredClone(fixture);
  edited.shapes["00000000-0000-4000-8000-000000000021"].bounds.x += 7;

  const module = makeProjectedFakeModule();
  assert.equal(projectAndSyncScene(module, fixture.page, fixture.shapes, fixture.page.rootShapeIds, 320, 240).ok, true);
  module.setFailureSequence(["base", "base"]);
  const failedRecovery = projectAndSyncScene(module, edited.page, edited.shapes, edited.page.rootShapeIds, 320, 240);
  assert.equal(failedRecovery.ok, false);
  assert.equal(failedRecovery.diagnostic.code, "LOGOS_SCENE_RECOVERY_FAILED");
  assert.equal(failedRecovery.diagnostic.recovered, false);
  const mutationCounts = [module.calls.resize, module.calls.begin, module.calls.base, module.calls.children, module.calls.renderAttempts];
  const poisoned = projectAndSyncScene(module, edited.page, edited.shapes, edited.page.rootShapeIds, 320, 240);
  assert.equal(poisoned.ok, false);
  assert.equal(poisoned.diagnostic.code, "LOGOS_SCENE_MODULE_POISONED");
  assert.deepEqual(
    [module.calls.resize, module.calls.begin, module.calls.base, module.calls.children, module.calls.renderAttempts],
    mutationCounts,
    "poisoned module accepted another mutating call",
  );
});
