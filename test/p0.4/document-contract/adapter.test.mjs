import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  encodeGeometryChange,
  P04AdapterError,
  P04_ADAPTER_VERSION,
  projectLegacyFileData,
  NIL_UUID,
} from "../../../logos-app/src/test-support/p04-document-adapter.ts";

const fixturePath = fileURLToPath(new URL("../../../test/fixtures/p0-document-compat/source-document.json", import.meta.url));
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
const pageId = fixture.pages[0];
const shapeId = fixture["pages-index"][pageId].objects[NIL_UUID].shapes[0];

test("versioned adapter preserves fixture identity, root order, appearance, and extension sentinel", () => {
  const projection = projectLegacyFileData(fixture);
  assert.equal(projection.adapterVersion, P04_ADAPTER_VERSION);
  assert.equal(projection.documentId, fixture.id);
  assert.deepEqual(projection.pageOrder, [pageId]);
  assert.deepEqual(projection.pages[pageId].rootShapeIds, [shapeId]);
  assert.equal(projection.shapes[shapeId].id, shapeId);
  assert.equal(projection.shapes[shapeId].parentId, null);
  assert.deepEqual(projection.shapes[shapeId].bounds, { x: 10, y: 20, w: 80, h: 40 });
  assert.deepEqual(projection.shapes[shapeId].fills, [{ type: "solid", color: "#336699", opacity: 1 }]);
  assert.deepEqual(projection.unknownFieldsById[shapeId]["extension-p0"], { sentinel: "preserve-me" });
});

test("unsupported shape discriminants produce an explicit adapter error", () => {
  const unsupported = structuredClone(fixture);
  unsupported["pages-index"][pageId].objects[shapeId].type = "vector-network";
  assert.throws(
    () => projectLegacyFileData(unsupported),
    (error) => error instanceof P04AdapterError && error.code === "UNSUPPORTED_SHAPE",
  );
});

test("geometry edit is encoded as the existing Go mod-obj operation", () => {
  const projection = projectLegacyFileData(fixture);
  const change = encodeGeometryChange(projection, shapeId, { x: 15, y: 20, w: 80, h: 40 });
  assert.equal(change.type, "mod-obj");
  assert.equal(change.id, shapeId);
  assert.equal(change["page-id"], pageId);
  assert.deepEqual(change.operations.at(-1), {
    attr: "selrect",
    val: { x: 15, y: 20, width: 80, height: 40, x1: 15, y1: 20, x2: 95, y2: 60 },
  });
});
