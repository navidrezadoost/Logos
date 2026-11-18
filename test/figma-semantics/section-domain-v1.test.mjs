import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const appRoot = fileURLToPath(new URL("../../logos-app/", import.meta.url));
const fixtureUrl = new URL("../fixtures/figma-semantics/section-domain-v1.json", import.meta.url);
const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
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

test("matrix-allows-page-and-section-containment-and-supported-leaves", () => {
  const { sectionContainment } = domain;
  const leaves = ["rect", "circle", "ellipse", "path", "text", "svg-raw", "vector-network"];
  for (const kind of ["section", "frame", "group", ...leaves]) {
    assert.equal(sectionContainment("page", kind), "allowed", `Page -> ${kind}`);
    assert.equal(sectionContainment("section", kind), "allowed", `Section -> ${kind}`);
  }
  for (const kind of ["frame", "group"]) {
    assert.equal(sectionContainment(kind, "section"), "forbidden", `${kind} -> Section`);
    assert.equal(sectionContainment(kind, "rect"), "allowed", `${kind} -> Rectangle`);
  }
  for (const kind of leaves) assert.equal(sectionContainment(kind, "rect"), "forbidden", `${kind} is a leaf`);
  for (const kind of ["component", "component-set", "instance", "bool"]) {
    assert.equal(sectionContainment("page", kind), "unqualified", `Page -> ${kind}`);
    assert.equal(sectionContainment("section", kind), "unqualified", `Section -> ${kind}`);
    assert.equal(sectionContainment("section", "instance"), "unqualified");
  }
  assert.equal(sectionContainment("rect", "rect"), "forbidden");
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
  assert.equal(domain.validateSectionSnapshot(badCycle), "SECTION_DUPLICATE_OWNERSHIP");
});

test("bounds-are-stored-separately-and-containment-edits-do-not-transform-child-geometry", () => {
  const snapshot = clone();
  const before = JSON.stringify(snapshot.nodes[ids.frame]);
  const moved = domain.planSectionOperation(snapshot, { kind: "reparent", id: ids.frame, parentId: ids.nested, index: 0 });
  assert.equal(moved.ok, true);
  assert.equal(JSON.stringify(moved.snapshot.nodes[ids.frame]), before.replace(`"parentId":"${ids.section}"`, `"parentId":"${ids.nested}"`));
  assert.deepEqual(moved.snapshot.nodes[ids.section].bounds, { x: 0, y: 0, width: 640, height: 480 });
});
