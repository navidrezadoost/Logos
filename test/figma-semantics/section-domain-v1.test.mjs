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
