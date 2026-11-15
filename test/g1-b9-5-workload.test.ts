import assert from "node:assert/strict";
import test from "node:test";
import { buildG1B95Fixture, makeG1B95Operation, type G1B95OperationKind } from "../logos-app/src/test-support/g1-b9-5-workload";

test("G1-B9.5 fixtures have exact size, depth, symmetric membership, and deterministic IDs", () => {
  for (const [size, expectedCounts] of [[21, [2, 8, 11]], [85, [2, 8, 32, 43]], [341, [2, 8, 32, 128, 171]]] as const) {
    const first = buildG1B95Fixture(size);
    const second = buildG1B95Fixture(size);
    assert.equal(Object.keys(first.shapes).length, size);
    assert.equal(first.idsByLevel.length, expectedCounts.length);
    assert.deepEqual(first.idsByLevel.map((level) => level.length), expectedCounts);
    assert.deepEqual(first, second);
    const visited = new Set<string>();
    const pending = [...first.page.rootShapeIds];
    while (pending.length) {
      const id = pending.pop()!;
      assert(!visited.has(id), `duplicate/cyclic membership at ${id}`);
      visited.add(id);
      const shape = first.shapes[id];
      assert(shape, `missing shape ${id}`);
      for (const childId of shape.children) {
        assert.equal(first.shapes[childId]?.parentId, id);
        pending.push(childId);
      }
    }
    assert.equal(visited.size, size);
  }
});

test("all seven G1-B9.5 operations have deterministic supported operation shapes", () => {
  const fixture = buildG1B95Fixture(21);
  const kinds: G1B95OperationKind[] = ["create", "insert-child", "rename", "sibling-reorder", "root-reorder", "subtree-delete", "reparent"];
  for (const kind of kinds) assert(!makeG1B95Operation(fixture, kind, 7).kind.startsWith("compat-"));
});
