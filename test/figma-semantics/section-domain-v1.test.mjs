import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const fixtureUrl = new URL("../fixtures/figma-semantics/section-domain-v1.json", import.meta.url);
const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
const geometryFixtureUrl = new URL("../fixtures/figma-semantics/section-geometry-candidate-v1.json", import.meta.url);
const geometryFixture = JSON.parse(await readFile(geometryFixtureUrl, "utf8"));
const require = createRequire(new URL("../../logos-app/package.json", import.meta.url));
const { createServer } = await import(require.resolve("vite"));
let server;
let domain;

before(async () => {
  server = await createServer({
    appType: "custom",
    configFile: false,
    logLevel: "error",
    optimizeDeps: { noDiscovery: true, include: [], entries: [] },
    root: appRoot,
    server: { middlewareMode: true, hmr: false },
  });
  domain = await server.ssrLoadModule("/src/document-core/sectionDomainV1.ts");
});

after(async () => server?.close());

const ids = {
  page: "10000000-0000-4000-8000-000000000001",
  section: "20000000-0000-4000-8000-000000000001",
  nested: "20000000-0000-4000-8000-000000000002",
  secondRoot: "20000000-0000-4000-8000-000000000004",
  frame: "30000000-0000-4000-8000-000000000001",
  group: "30000000-0000-4000-8000-000000000002",
  rect: "30000000-0000-4000-8000-000000000003",
  newSection: "20000000-0000-4000-8000-000000000005",
  newLeaf: "30000000-0000-4000-8000-000000000004",
};

function clone() {
  return structuredClone(fixture);
}

function assertRejectedUnchanged(snapshot, operation, code) {
  const before = JSON.stringify(snapshot);
  const result = domain.planSectionOperation(snapshot, operation);
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, code);
  assert.strictEqual(result.snapshot, snapshot, "rejection returns the exact input reference");
  assert.deepEqual(result.changedIds, []);
  assert.equal(JSON.stringify(snapshot), before, "rejection does not mutate input");
}

function node(id, pageId, kind, parentId = null, extra = {}) {
  return { id, pageId, kind, parentId, childIds: [], locked: false, visible: true, ...extra };
}

function idFromNumber(value) {
  return `50000000-0000-4000-8000-${String(value).padStart(12, "0")}`;
}

function snapshotForEdge(parentKind, childKind, parentId, childId) {
  const pageId = "10000000-0000-4000-8000-000000000009";
  const parent = node(parentId, pageId, parentKind, null, {
    childIds: [childId],
    ...(parentKind === "section" ? { bounds: { x: 0, y: 0, width: 10, height: 10 } } : {}),
  });
  const child = node(childId, pageId, childKind, parentId, {
    ...(childKind === "section" ? { bounds: { x: 1, y: 1, width: 5, height: 5 } } : {}),
  });
  return {
    version: 1,
    pageOrder: [pageId],
    pages: { [pageId]: { id: pageId, childIds: [parentId] } },
    nodes: { [parentId]: parent, [childId]: child },
  };
}

test("fixture-validates-exact-section-ownership-and-stable-ids", () => {
  const snapshot = clone();
  assert.equal(domain.SECTION_DOMAIN_VERSION, 1);
  assert.equal(domain.validateSectionSnapshot(snapshot), null);
  assert.deepEqual(snapshot.pages[ids.page].childIds, [ids.section, ids.secondRoot]);
  assert.deepEqual(snapshot.nodes[ids.section].childIds, [ids.nested, ids.frame]);
  assert.equal(snapshot.nodes[ids.nested].parentId, ids.section);
  assert.equal(snapshot.nodes[ids.rect].parentId, ids.frame);
  assert.equal(domain.isSectionUuid(ids.section), true);
  assert.equal(domain.isSectionUuid("section-1"), false);
});

test("matrix-driven-containment-covers-every-known-parent-child-kind-and-planner-agrees", () => {
  const { sectionContainment } = domain;
  const kinds = ["section", "frame", "group", "rect", "circle", "ellipse", "path", "text", "svg-raw", "vector-network", "bool", "component", "component-set", "instance"];
  const leaves = new Set(["rect", "circle", "ellipse", "path", "text", "svg-raw", "vector-network"]);
  const unqualified = new Set(["bool", "component", "component-set", "instance"]);
  const expected = (parent, child) => {
    if (unqualified.has(parent) || unqualified.has(child)) return "unqualified";
    if (parent === "page") return "allowed";
    if (parent === "section") return ["section", "frame", "group", ...leaves].includes(child) ? "allowed" : "forbidden";
    if (parent === "frame" || parent === "group") return child === "section" ? "forbidden" : ["frame", "group", ...leaves].includes(child) ? "allowed" : "forbidden";
    return "forbidden";
  };

  let serial = 1;
  for (const parent of ["page", ...kinds]) {
    for (const child of kinds) {
      const classification = expected(parent, child);
      assert.equal(sectionContainment(parent, child), classification, `${parent} -> ${child}`);
      if (parent === "page") continue;
      const parentId = idFromNumber(serial++);
      const childId = idFromNumber(serial++);
      const snapshot = snapshotForEdge(parent, child, parentId, childId);
      assert.equal(domain.validateSectionSnapshot(snapshot), classification === "allowed"
        ? null
        : classification === "unqualified" ? "SECTION_RELATIONSHIP_UNQUALIFIED" : "SECTION_CONTAINMENT_FORBIDDEN",
      `snapshot validator: ${parent} -> ${child}`);
      const result = domain.planSectionOperation(snapshot, {
        kind: "insert-node", parentId, index: 1,
        node: node(idFromNumber(serial++), snapshot.pages[Object.keys(snapshot.pages)[0]].id, child, null,
          child === "section" ? { bounds: { x: 2, y: 2, width: 3, height: 3 } } : {}),
      });
      assert.equal(result.ok, classification === "allowed", `planner: ${parent} -> ${child}`);
      if (classification !== "allowed") {
        assert.equal(result.diagnostic.code, classification === "unqualified"
          ? "SECTION_RELATIONSHIP_UNQUALIFIED" : "SECTION_CONTAINMENT_FORBIDDEN");
      } else {
        assert.equal(domain.validateSectionSnapshot(result.snapshot), null);
      }
    }
  }
  for (const child of kinds) {
    const classification = expected("page", child);
    const rootId = idFromNumber(serial++);
    const pageId = "10000000-0000-4000-8000-000000000008";
    const rootSnapshot = {
      version: 1,
      pageOrder: [pageId],
      pages: { [pageId]: { id: pageId, childIds: [rootId] } },
      nodes: { [rootId]: node(rootId, pageId, child, null,
        child === "section" ? { bounds: { x: 0, y: 0, width: 10, height: 10 } } : {}) },
    };
    assert.equal(domain.validateSectionSnapshot(rootSnapshot), classification === "allowed"
      ? null : "SECTION_RELATIONSHIP_UNQUALIFIED", `Page-root validator: ${child}`);

    if (classification === "allowed") {
      const parentId = idFromNumber(serial++);
      const nestedId = idFromNumber(serial++);
      const source = snapshotForEdge("section", child, parentId, nestedId);
      const result = domain.planSectionOperation(source, {
        kind: "reparent", id: nestedId, parentId: null,
        pageId: source.pageOrder[0], index: 1,
      });
      assert.equal(result.ok, true, `Page-root planner: ${child}`);
      assert.equal(domain.validateSectionSnapshot(result.snapshot), null);
    } else {
      const result = domain.planSectionOperation(rootSnapshot, { kind: "reorder", id: rootId, index: 0 });
      assert.equal(result.ok, false);
      assert.equal(result.diagnostic.code, "SECTION_RELATIONSHIP_UNQUALIFIED");
      assert.strictEqual(result.snapshot, rootSnapshot);
    }
  }
  assert.equal(sectionContainment("page", "future-node"), "unqualified");
  assert.equal(sectionContainment("future-parent", "rect"), "unqualified");
});

test("page-root-snapshot-validation-rejects-unqualified-and-unknown-node-kinds", () => {
  for (const kind of ["component", "component-set", "instance", "bool"]) {
    const snapshot = clone();
    const id = idFromNumber(100 + ["component", "component-set", "instance", "bool"].indexOf(kind));
    snapshot.nodes[id] = node(id, ids.page, kind);
    snapshot.pages[ids.page].childIds.push(id);
    assert.equal(domain.validateSectionSnapshot(snapshot), "SECTION_RELATIONSHIP_UNQUALIFIED", `Page -> ${kind}`);
  }
  const unknown = clone();
  unknown.nodes[ids.newLeaf] = node(ids.newLeaf, ids.page, "future-layer");
  unknown.pages[ids.page].childIds.push(ids.newLeaf);
  assert.equal(domain.validateSectionSnapshot(unknown), "SECTION_NODE_KIND_UNSUPPORTED");
  const rejected = domain.planSectionOperation(unknown, { kind: "reorder", id: ids.secondRoot, index: 0 });
  assert.equal(rejected.ok, false);
  assert.equal(rejected.diagnostic.code, "SECTION_NODE_KIND_UNSUPPORTED");
  assert.strictEqual(rejected.snapshot, unknown);

  const valid = clone();
  const operation = domain.planSectionOperation(valid, {
    kind: "insert-node", parentId: ids.section, index: 2,
    node: node(ids.newLeaf, ids.page, "future-layer"),
  });
  assert.equal(operation.ok, false);
  assert.equal(operation.diagnostic.code, "SECTION_NODE_KIND_UNSUPPORTED");
  assert.strictEqual(operation.snapshot, valid);
});

test("runtime-snapshot-structure-flags-and-child-lists-fail-closed", () => {
  assert.equal(domain.validateSectionSnapshot(null), "SECTION_SNAPSHOT_MALFORMED");
  for (const mutateAndExpect of [
    [(s) => { s.pages[ids.page].childIds = "not-an-array"; }, "SECTION_PAGE_RECORD_MALFORMED"],
    [(s) => { s.nodes[ids.frame].childIds = "not-an-array"; }, "SECTION_CHILD_LIST_INVALID"],
    [(s) => { s.nodes[ids.frame].locked = 1; }, "SECTION_NODE_FLAG_INVALID"],
    [(s) => { delete s.nodes[ids.frame].visible; }, "SECTION_NODE_FLAG_INVALID"],
    [(s) => { s.nodes[ids.frame].parentId = 4; }, "SECTION_NODE_RECORD_MALFORMED"],
    [(s) => { s.nodes[ids.frame].parentId = ids.newLeaf; }, "SECTION_PARENT_MISSING"],
    [(s) => { s.nodes[ids.frame].pageId = "40000000-0000-4000-8000-000000000001"; }, "SECTION_PAGE_MISSING"],
  ]) {
    const snapshot = clone();
    mutateAndExpect[0](snapshot);
    assert.equal(domain.validateSectionSnapshot(snapshot), mutateAndExpect[1]);
    const result = domain.planSectionOperation(snapshot, { kind: "reorder", id: ids.secondRoot, index: 0 });
    assert.equal(result.ok, false);
    assert.equal(result.diagnostic.code, mutateAndExpect[1]);
    assert.strictEqual(result.snapshot, snapshot);
  }
});

test("malformed-operation-objects-and-unknown-kinds-reject-without-falling-through-to-delete", () => {
  const snapshot = clone();
  for (const [operation, code] of [
    [undefined, "SECTION_OPERATION_MALFORMED"],
    [null, "SECTION_OPERATION_MALFORMED"],
    [[], "SECTION_OPERATION_MALFORMED"],
    [{}, "SECTION_OPERATION_MALFORMED"],
    [{ kind: 7, id: ids.section, contents: "delete" }, "SECTION_OPERATION_MALFORMED"],
    [{ kind: "future-operation", id: ids.section, contents: "delete" }, "SECTION_OPERATION_KIND_UNSUPPORTED"],
    [Object.assign(Object.create({ kind: "delete-section", id: ids.section, contents: "delete" }), {}), "SECTION_OPERATION_MALFORMED"],
  ]) assertRejectedUnchanged(snapshot, operation, code);
  assert.ok(snapshot.nodes[ids.section], "unknown operations cannot delete target Section");
  assert.deepEqual(snapshot.nodes[ids.section].childIds, [ids.nested, ids.frame]);
});

test("delete-policy-must-be-explicit-and-exact", () => {
  const snapshot = clone();
  for (const operation of [
    { kind: "delete-section", id: ids.section },
    { kind: "delete-section", id: ids.section, contents: undefined },
    { kind: "delete-section", id: ids.section, contents: null },
    { kind: "delete-section", id: ids.section, contents: "unexpected-value" },
  ]) assertRejectedUnchanged(snapshot, operation, "SECTION_DELETE_CONTENT_POLICY_UNQUALIFIED");
});

test("operation-payloads-validate-required-fields-references-flags-bounds-and-indices", () => {
  const snapshot = clone();
  const validCreate = {
    kind: "create-section", id: ids.newSection, pageId: ids.page, parentId: null,
    index: 0, bounds: { x: 0, y: 0, width: 1, height: 1 },
  };
  const validNode = node(ids.newLeaf, ids.page, "text");
  const createWithoutBounds = { ...validCreate };
  delete createWithoutBounds.bounds;
  const nodeWithoutChildren = { ...validNode };
  delete nodeWithoutChildren.childIds;
  const cases = [
    [{ kind: "create-section", id: ids.newSection, pageId: ids.page, index: 0, bounds: { x: 0, y: 0, width: 1, height: 1 } }, "SECTION_OPERATION_MALFORMED"],
    [createWithoutBounds, "SECTION_OPERATION_MALFORMED"],
    [{ ...validCreate, bounds: null }, "SECTION_INVALID_BOUNDS"],
    [{ ...validCreate, bounds: "bad-bounds" }, "SECTION_INVALID_BOUNDS"],
    [{ ...validCreate, id: "bad-id" }, "SECTION_INVALID_UUID"],
    [{ ...validCreate, pageId: "bad-page-id" }, "SECTION_INVALID_UUID"],
    [{ ...validCreate, pageId: "40000000-0000-4000-8000-000000000001" }, "SECTION_PAGE_MISSING"],
    [{ ...validCreate, parentId: "bad-parent-id" }, "SECTION_INVALID_UUID"],
    [{ ...validCreate, parentId: ids.newLeaf }, "SECTION_PARENT_MISSING"],
    [{ ...validCreate, index: 0.5 }, "SECTION_INVALID_ORDER_INDEX"],
    [{ ...validCreate, index: 999 }, "SECTION_INVALID_ORDER_INDEX"],
    [{ ...validCreate, visible: "yes" }, "SECTION_NODE_FLAG_INVALID"],
    [{ ...validCreate, locked: null }, "SECTION_NODE_FLAG_INVALID"],
    [{ kind: "insert-node", parentId: ids.section, index: 2 }, "SECTION_OPERATION_MALFORMED"],
    [{ kind: "insert-node", parentId: ids.section, index: 2, node: null }, "SECTION_NODE_RECORD_MALFORMED"],
    [{ kind: "insert-node", parentId: ids.section, index: 2, node: { ...validNode, childIds: null } }, "SECTION_CHILD_LIST_INVALID"],
    [{ kind: "insert-node", parentId: ids.section, index: 2, node: nodeWithoutChildren }, "SECTION_CHILD_LIST_INVALID"],
    [{ kind: "insert-node", parentId: ids.section, index: 2, node: { ...validNode, locked: 1 } }, "SECTION_NODE_FLAG_INVALID"],
    [{ kind: "insert-node", parentId: "bad-parent-id", index: 2, node: validNode }, "SECTION_INVALID_UUID"],
    [{ kind: "insert-node", parentId: "40000000-0000-4000-8000-000000000001", index: 0, node: validNode }, "SECTION_PARENT_MISSING"],
    [{ kind: "insert-node", parentId: ids.section, index: 999, node: validNode }, "SECTION_INVALID_ORDER_INDEX"],
    [{ kind: "reorder", id: ids.secondRoot, index: -1 }, "SECTION_INVALID_ORDER_INDEX"],
    [{ kind: "reorder", id: ids.secondRoot, index: 999 }, "SECTION_INVALID_ORDER_INDEX"],
    [{ kind: "reorder", id: "bad-id", index: 0 }, "SECTION_INVALID_UUID"],
    [{ kind: "reparent", id: ids.secondRoot, index: 0 }, "SECTION_OPERATION_MALFORMED"],
    [{ kind: "reparent", id: ids.secondRoot, parentId: "bad-parent-id", index: 0 }, "SECTION_INVALID_UUID"],
    [{ kind: "reparent", id: ids.secondRoot, parentId: null, pageId: "bad-page-id", index: 0 }, "SECTION_INVALID_UUID"],
    [{ kind: "reparent", id: ids.secondRoot, parentId: null, pageId: "40000000-0000-4000-8000-000000000001", index: 0 }, "SECTION_PAGE_MISSING"],
    [{ kind: "reparent", id: ids.secondRoot, parentId: null, pageId: ids.page, index: 999 }, "SECTION_INVALID_ORDER_INDEX"],
    [{ kind: "reparent", id: ids.secondRoot, parentId: ids.newLeaf, index: 0 }, "SECTION_PARENT_MISSING"],
    [{ kind: "delete-section", contents: "delete" }, "SECTION_OPERATION_MALFORMED"],
    [{ kind: "delete-section", id: "bad-id", contents: "delete" }, "SECTION_INVALID_UUID"],
  ];
  for (const [operation, code] of cases) assertRejectedUnchanged(snapshot, operation, code);
  assert.ok(snapshot.nodes[ids.section]);
  assert.deepEqual(snapshot.pages[ids.page].childIds, [ids.section, ids.secondRoot]);
});

test("valid-operation-is-rejected-immutably-when-its-snapshot-is-malformed", () => {
  const snapshot = clone();
  snapshot.nodes[ids.frame].visible = "visible";
  const before = JSON.stringify(snapshot);
  const result = domain.planSectionOperation(snapshot, {
    kind: "create-section", id: ids.newSection, pageId: ids.page, parentId: null,
    index: 0, bounds: { x: 0, y: 0, width: 10, height: 10 },
  });
  assert.equal(result.ok, false);
  assert.equal(result.diagnostic.code, "SECTION_NODE_FLAG_INVALID");
  assert.strictEqual(result.snapshot, snapshot);
  assert.deepEqual(result.changedIds, []);
  assert.equal(JSON.stringify(snapshot), before);
});

test("geometry-candidate-fixture-keeps-authored-bounds-separate-from-explicit-membership", () => {
  const snapshot = structuredClone(geometryFixture);
  assert.equal(snapshot.evidenceClass, "generated-logos-candidate-fixture");
  delete snapshot.evidenceClass;
  assert.equal(domain.validateSectionSnapshot(snapshot), null);

  const [outerId, overlapId, emptyId] = snapshot.pages[snapshot.pageOrder[0]].childIds;
  const outer = snapshot.nodes[outerId];
  const nestedId = outer.childIds[0];
  const frameId = outer.childIds[1];
  const rectId = outer.childIds[2];
  const frameRectId = snapshot.nodes[frameId].childIds[0];
  const groupId = snapshot.nodes[nestedId].childIds[0];
  const textId = snapshot.nodes[groupId].childIds[0];
  const originalGeometry = Object.fromEntries(
    [outerId, nestedId, frameId, frameRectId, rectId, textId, overlapId, emptyId]
      .map((id) => [id, {
        bounds: snapshot.nodes[id].bounds && structuredClone(snapshot.nodes[id].bounds),
        worldBounds: snapshot.nodes[id].worldBounds && structuredClone(snapshot.nodes[id].worldBounds),
      }]),
  );

  assert.deepEqual(outer.bounds, { x: -120, y: -80, width: 500, height: 360 });
  assert.deepEqual(snapshot.nodes[nestedId].bounds, { x: -40, y: -20, width: 160, height: 110 });
  assert.deepEqual(snapshot.nodes[frameId].worldBounds, { x: -100, y: -60, width: 220, height: 140 });
  assert.deepEqual(snapshot.nodes[frameRectId].worldBounds, { x: -90, y: -40, width: 35, height: 22 });
  assert.deepEqual(snapshot.nodes[rectId].worldBounds, { x: 280, y: 170, width: 17, height: 23 });
  assert.deepEqual(snapshot.nodes[textId].worldBounds, { x: -35, y: -12, width: 190, height: 31 });
  assert.deepEqual(snapshot.nodes[emptyId].bounds, { x: -500, y: 260, width: 0, height: 0 });
  assert.ok(outer.bounds.x < 0 && snapshot.nodes[rectId].worldBounds.x > 0, "fixture spans negative and positive coordinates");
  assert.deepEqual(snapshot.nodes[outerId].childIds, [nestedId, frameId, rectId]);
  assert.deepEqual(snapshot.nodes[overlapId].childIds, []);

  // The nested Section and a shape can be explicitly reparented in the candidate.
  // No overlap-derived membership or coordinate transform is inferred.
  const moveNested = domain.planSectionOperation(snapshot, {
    kind: "reparent", id: nestedId, parentId: overlapId, index: 0,
  });
  assert.equal(moveNested.ok, true);
  const moveRect = domain.planSectionOperation(moveNested.snapshot, {
    kind: "reparent", id: rectId, parentId: overlapId, index: 1,
  });
  assert.equal(moveRect.ok, true);
  assert.deepEqual(moveRect.snapshot.nodes[outerId].childIds, [frameId]);
  assert.deepEqual(moveRect.snapshot.nodes[overlapId].childIds, [nestedId, rectId]);
  for (const id of [outerId, nestedId, frameId, frameRectId, rectId, textId, overlapId, emptyId]) {
    assert.deepEqual({
      bounds: moveRect.snapshot.nodes[id].bounds && structuredClone(moveRect.snapshot.nodes[id].bounds),
      worldBounds: moveRect.snapshot.nodes[id].worldBounds && structuredClone(moveRect.snapshot.nodes[id].worldBounds),
    }, originalGeometry[id], `candidate hierarchy edits preserve authored geometry for ${id}`);
  }

  const addedEmpty = domain.planSectionOperation(moveRect.snapshot, {
    kind: "create-section", id: "20000000-0000-4000-8000-000000000028",
    pageId: snapshot.pageOrder[0], parentId: null, index: 3,
    bounds: { x: -700, y: -300, width: 40, height: 50 },
  });
  assert.equal(addedEmpty.ok, true);
  assert.deepEqual(addedEmpty.snapshot.nodes["20000000-0000-4000-8000-000000000028"].bounds,
    { x: -700, y: -300, width: 40, height: 50 });

  // Movement and resize have no qualified operation in this candidate.
  for (const operation of [
    { kind: "move-section", id: outerId, dx: 10, dy: -5 },
    { kind: "resize-section", id: outerId, bounds: { x: 0, y: 0, width: 1, height: 1 } },
  ]) assertRejectedUnchanged(moveRect.snapshot, operation, "SECTION_OPERATION_KIND_UNSUPPORTED");
});

test("geometry-candidate-delete-with-contents-removes-exact-subtree-and-preserve-keeps-geometry", () => {
  const snapshot = structuredClone(geometryFixture);
  delete snapshot.evidenceClass;
  const [outerId, , emptyId] = snapshot.pages[snapshot.pageOrder[0]].childIds;
  const subtreeIds = [outerId, ...snapshot.nodes[outerId].childIds];
  const nestedId = snapshot.nodes[outerId].childIds[0];
  const frameId = snapshot.nodes[outerId].childIds[1];
  const groupId = snapshot.nodes[nestedId].childIds[0];
  const textId = snapshot.nodes[groupId].childIds[0];
  subtreeIds.push(...snapshot.nodes[frameId].childIds, groupId, textId);
  const preservedWorldBounds = structuredClone(snapshot.nodes[textId].worldBounds);

  const preserveEmpty = domain.planSectionOperation(snapshot, {
    kind: "delete-section", id: emptyId, contents: "preserve",
  });
  assert.equal(preserveEmpty.ok, true);
  assert.equal(preserveEmpty.snapshot.nodes[emptyId], undefined);
  assert.deepEqual(preserveEmpty.snapshot.nodes[outerId].bounds, snapshot.nodes[outerId].bounds);

  const preserveOuter = domain.planSectionOperation(snapshot, {
    kind: "delete-section", id: outerId, contents: "preserve",
  });
  assert.equal(preserveOuter.ok, true);
  assert.deepEqual(preserveOuter.snapshot.pages[snapshot.pageOrder[0]].childIds,
    [nestedId, frameId, snapshot.nodes[outerId].childIds[2], ...snapshot.pages[snapshot.pageOrder[0]].childIds.slice(1)]);
  assert.deepEqual(preserveOuter.snapshot.nodes[nestedId].bounds, snapshot.nodes[nestedId].bounds);
  assert.deepEqual(preserveOuter.snapshot.nodes[textId].worldBounds, preservedWorldBounds);
  for (const id of subtreeIds.slice(1)) assert.ok(preserveOuter.snapshot.nodes[id], `preserved subtree member ${id}`);

  const deleted = domain.planSectionOperation(snapshot, {
    kind: "delete-section", id: outerId, contents: "delete",
  });
  assert.equal(deleted.ok, true);
  for (const id of subtreeIds) assert.equal(deleted.snapshot.nodes[id], undefined, `deleted subtree member ${id}`);
  assert.equal(deleted.snapshot.nodes[emptyId].bounds.x, -500);
  assert.deepEqual(snapshot.nodes[textId].worldBounds, preservedWorldBounds, "input fixture remains immutable");
  assert.equal(domain.validateSectionSnapshot(deleted.snapshot), null);
});

test("create-root-and-nested-sections-publish-new-immutable-snapshots-in-order", () => {
  const input = clone();
  const original = JSON.stringify(input);
  const root = domain.planSectionOperation(input, {
    kind: "create-section", id: ids.newSection, pageId: ids.page, parentId: null,
    index: 1, bounds: { x: 100, y: 100, width: 300, height: 200 },
  });
  assert.equal(root.ok, true);
  assert.notStrictEqual(root.snapshot, input);
  assert.deepEqual(root.snapshot.pages[ids.page].childIds, [ids.section, ids.newSection, ids.secondRoot]);
  assert.equal(root.snapshot.nodes[ids.newSection].id, ids.newSection);
  assert.equal(JSON.stringify(input), original);

  const nested = domain.planSectionOperation(root.snapshot, {
    kind: "create-section", id: "20000000-0000-4000-8000-000000000006", pageId: ids.page,
    parentId: ids.section, index: 1, bounds: { x: 5, y: 6, width: 7, height: 8 },
  });
  assert.equal(nested.ok, true);
  assert.deepEqual(nested.snapshot.nodes[ids.section].childIds, [ids.nested, "20000000-0000-4000-8000-000000000006", ids.frame]);
  assert.equal(domain.validateSectionSnapshot(nested.snapshot), null);
});

test("insert-node-and-reorder-preserve-sibling-order-and-identity", () => {
  const input = clone();
  const inserted = domain.planSectionOperation(input, {
    kind: "insert-node",
    parentId: ids.section,
    index: 2,
    node: node(ids.newLeaf, ids.page, "text"),
  });
  assert.equal(inserted.ok, true);
  assert.deepEqual(inserted.snapshot.nodes[ids.section].childIds, [ids.nested, ids.frame, ids.newLeaf]);
  const reordered = domain.planSectionOperation(inserted.snapshot, { kind: "reorder", id: ids.frame, index: 0 });
  assert.equal(reordered.ok, true);
  assert.deepEqual(reordered.snapshot.nodes[ids.section].childIds, [ids.frame, ids.nested, ids.newLeaf]);
  assert.equal(reordered.snapshot.nodes[ids.frame].id, ids.frame);
});

test("rejects-section-under-frame-or-group-and-component-relationships-as-unqualified", () => {
  for (const parentId of [ids.frame, ids.group]) {
    const snapshot = clone();
    const at = snapshot.nodes[parentId].childIds.length;
    assertRejectedUnchanged(snapshot, {
      kind: "create-section", id: ids.newSection, pageId: ids.page,
      parentId, index: at, bounds: { x: 0, y: 0, width: 10, height: 10 },
    }, "SECTION_CONTAINMENT_FORBIDDEN");
  }
  const snapshot = clone();
  assertRejectedUnchanged(snapshot, {
    kind: "insert-node", parentId: ids.section, index: 2,
    node: node(ids.newLeaf, ids.page, "instance"),
  }, "SECTION_RELATIONSHIP_UNQUALIFIED");
  assertRejectedUnchanged(snapshot, {
    kind: "insert-node", parentId: ids.section, index: 2,
    node: node(ids.newLeaf, ids.page, "frame", null, { childIds: [ids.rect] }),
  }, "SECTION_INSERT_REQUIRES_EMPTY_NODE");
});

test("rejects-invalid-uuid-cross-page-and-invalid-bounds-before-mutation", () => {
  const snapshot = clone();
  assertRejectedUnchanged(snapshot, {
    kind: "create-section", id: "not-a-uuid", pageId: ids.page, parentId: null,
    index: 0, bounds: { x: 0, y: 0, width: 1, height: 1 },
  }, "SECTION_INVALID_UUID");
  assertRejectedUnchanged(snapshot, {
    kind: "create-section", id: ids.newSection, pageId: "40000000-0000-4000-8000-000000000001", parentId: null,
    index: 0, bounds: { x: 0, y: 0, width: 1, height: 1 },
  }, "SECTION_PAGE_MISSING");
  assertRejectedUnchanged(snapshot, {
    kind: "create-section", id: ids.newSection, pageId: ids.page, parentId: null,
    index: 0, bounds: { x: 0, y: 0, width: -1, height: 1 },
  }, "SECTION_INVALID_BOUNDS");
});

test("reparent-allows-section-nesting-but-prevents-cycles-and-cross-page-moves", () => {
  const snapshot = clone();
  const moved = domain.planSectionOperation(snapshot, { kind: "reparent", id: ids.secondRoot, parentId: ids.nested, index: 0 });
  assert.equal(moved.ok, true);
  assert.deepEqual(moved.snapshot.pages[ids.page].childIds, [ids.section]);
  assert.deepEqual(moved.snapshot.nodes[ids.nested].childIds, [ids.secondRoot, ids.group]);
  assert.equal(moved.snapshot.nodes[ids.secondRoot].id, ids.secondRoot);

  const cycle = clone();
  assertRejectedUnchanged(cycle, { kind: "reparent", id: ids.section, parentId: ids.nested, index: 0 }, "SECTION_CYCLE");

  const otherPageId = "10000000-0000-4000-8000-000000000002";
  const crossPage = clone();
  crossPage.pages[otherPageId] = { id: otherPageId, childIds: [] };
  crossPage.pageOrder.push(otherPageId);
  assertRejectedUnchanged(crossPage, {
    kind: "reparent", id: ids.secondRoot, parentId: null, pageId: otherPageId, index: 0,
  }, "SECTION_CROSS_PAGE_OPERATION");
  const forged = clone();
  forged.pages[otherPageId] = { id: otherPageId, childIds: [] };
  forged.pageOrder.push(otherPageId);
  forged.nodes[ids.secondRoot].pageId = otherPageId;
  assert.equal(domain.validateSectionSnapshot(forged), "SECTION_PAGE_OWNERSHIP_MISMATCH");
});

test("reparent-to-page-root-preserves-page-order-and-section-bounds", () => {
  const snapshot = clone();
  const beforeBounds = structuredClone(snapshot.nodes[ids.nested].bounds);
  const result = domain.planSectionOperation(snapshot, {
    kind: "reparent", id: ids.nested, parentId: null, pageId: ids.page, index: 2,
  });
  assert.equal(result.ok, true);
  assert.deepEqual(result.snapshot.pages[ids.page].childIds, [ids.section, ids.secondRoot, ids.nested]);
  assert.equal(result.snapshot.nodes[ids.nested].parentId, null);
  assert.deepEqual(result.snapshot.nodes[ids.nested].bounds, beforeBounds);
});

test("delete-subtree-removes-all-descendant-identities-atomically", () => {
  const snapshot = clone();
  const result = domain.planSectionOperation(snapshot, { kind: "delete-section", id: ids.section, contents: "delete" });
  assert.equal(result.ok, true);
  assert.deepEqual(result.snapshot.pages[ids.page].childIds, [ids.secondRoot]);
  for (const id of [ids.section, ids.nested, ids.frame, ids.group, ids.rect]) assert.equal(result.snapshot.nodes[id], undefined);
  assert.equal(JSON.stringify(snapshot).includes(ids.section), true, "input graph remains unchanged");
  assert.equal(domain.validateSectionSnapshot(result.snapshot), null);
});

test("preserve-contents-delete-unwraps-section-and-splices-children-in-order", () => {
  const snapshot = clone();
  const frameWorldBounds = structuredClone(snapshot.nodes[ids.frame].worldBounds);
  const nestedBounds = structuredClone(snapshot.nodes[ids.nested].bounds);
  const result = domain.planSectionOperation(snapshot, { kind: "delete-section", id: ids.section, contents: "preserve" });
  assert.equal(result.ok, true);
  assert.deepEqual(result.snapshot.pages[ids.page].childIds, [ids.nested, ids.frame, ids.secondRoot]);
  assert.equal(result.snapshot.nodes[ids.section], undefined);
  assert.equal(result.snapshot.nodes[ids.nested].parentId, null);
  assert.equal(result.snapshot.nodes[ids.frame].parentId, null);
  assert.equal(result.snapshot.nodes[ids.nested].id, ids.nested);
  assert.deepEqual(result.snapshot.nodes[ids.frame].worldBounds, frameWorldBounds);
  assert.deepEqual(result.snapshot.nodes[ids.nested].bounds, nestedBounds);
  assert.equal(domain.validateSectionSnapshot(result.snapshot), null);
});

test("locked-ancestor-and-invalid-existing-graphs-reject-without-changing-input", () => {
  const locked = clone();
  locked.nodes[ids.section].locked = true;
  assertRejectedUnchanged(locked, { kind: "reorder", id: ids.nested, index: 0 }, "SECTION_LOCKED_ANCESTOR");

  const lockedDescendant = clone();
  lockedDescendant.nodes[ids.rect].locked = true;
  assertRejectedUnchanged(lockedDescendant, {
    kind: "delete-section", id: ids.section, contents: "delete",
  }, "SECTION_LOCKED_ANCESTOR");

  const duplicate = clone();
  duplicate.pages[ids.page].childIds.push(ids.frame);
  assert.equal(domain.validateSectionSnapshot(duplicate), "SECTION_DUPLICATE_OWNERSHIP");
  assertRejectedUnchanged(duplicate, { kind: "reorder", id: ids.secondRoot, index: 0 }, "SECTION_DUPLICATE_OWNERSHIP");

  const badCycle = clone();
  badCycle.nodes[ids.section].parentId = ids.nested;
  badCycle.nodes[ids.nested].childIds.push(ids.section);
  assert.equal(domain.validateSectionSnapshot(badCycle), "SECTION_CYCLE");
});

test("cycle-diagnostics-cover-self-multi-node-and-disconnected-cycles-before-ownership-errors", () => {
  const self = clone();
  self.nodes[ids.secondRoot].childIds.push(ids.secondRoot);
  assert.equal(domain.validateSectionSnapshot(self), "SECTION_CYCLE");

  const multi = clone();
  multi.nodes[ids.section].parentId = ids.nested;
  multi.nodes[ids.nested].childIds.push(ids.section);
  assert.equal(domain.validateSectionSnapshot(multi), "SECTION_CYCLE");

  const disconnected = clone();
  const a = "60000000-0000-4000-8000-000000000001";
  const b = "60000000-0000-4000-8000-000000000002";
  disconnected.nodes[a] = node(a, ids.page, "section", b, {
    childIds: [b], bounds: { x: 0, y: 0, width: 1, height: 1 },
  });
  disconnected.nodes[b] = node(b, ids.page, "section", a, {
    childIds: [a], bounds: { x: 0, y: 0, width: 1, height: 1 },
  });
  assert.equal(domain.validateSectionSnapshot(disconnected), "SECTION_CYCLE");
});

test("duplicate-child-ownership-is-distinguished-from-cycles", () => {
  const duplicateInList = clone();
  duplicateInList.nodes[ids.section].childIds.push(ids.frame);
  assert.equal(domain.validateSectionSnapshot(duplicateInList), "SECTION_DUPLICATE_OWNERSHIP");

  const duplicateAcrossOwners = clone();
  duplicateAcrossOwners.pages[ids.page].childIds.push(ids.frame);
  assert.equal(domain.validateSectionSnapshot(duplicateAcrossOwners), "SECTION_DUPLICATE_OWNERSHIP");
});

test("bounds-are-stored-separately-and-containment-edits-do-not-transform-child-geometry", () => {
  const snapshot = clone();
  const before = JSON.stringify(snapshot.nodes[ids.frame]);
  const moved = domain.planSectionOperation(snapshot, { kind: "reparent", id: ids.frame, parentId: ids.nested, index: 0 });
  assert.equal(moved.ok, true);
  assert.equal(JSON.stringify(moved.snapshot.nodes[ids.frame]), before.replace(`"parentId":"${ids.section}"`, `"parentId":"${ids.nested}"`));
  assert.deepEqual(moved.snapshot.nodes[ids.section].bounds, { x: 0, y: 0, width: 640, height: 480 });
});
