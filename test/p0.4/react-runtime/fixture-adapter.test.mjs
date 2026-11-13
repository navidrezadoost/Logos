import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  projectP04RendererFixture,
  UnsupportedP04RendererShapeError,
} from "../../../logos-app/src/test-support/p04-renderer-fixture.ts";

const fixturePath = fileURLToPath(new URL("../../fixtures/p0-document-compat/source-document.json", import.meta.url));
const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));

test("React renderer projection keeps fixture UUID, nil-root order, geometry, fill and extension sidecar", () => {
  const projection = projectP04RendererFixture(fixture);
  const pageId = fixture.pages[0];
  const shapeId = projection.pages[pageId].rootShapeIds[0];
  assert.equal(projection.documentId, fixture.id);
  assert.deepEqual(projection.pageOrder, [pageId]);
  assert.equal(projection.shapes[shapeId].parentId, null);
  assert.deepEqual(projection.shapes[shapeId].bounds, { x: 10, y: 20, w: 80, h: 40 });
  assert.deepEqual(projection.shapes[shapeId].fills, [{ type: "solid", color: "#336699", opacity: 1 }]);
  assert.deepEqual(projection.unknownFieldsById[shapeId]["extension-p0"], { sentinel: "preserve-me" });
});

test("renderer fixture adapter rejects unsupported shapes rather than coercing them to rect", () => {
  const unsupported = structuredClone(fixture);
  const pageId = unsupported.pages[0];
  const rootId = "00000000-0000-0000-0000-000000000000";
  const shapeId = unsupported["pages-index"][pageId].objects[rootId].shapes[0];
  unsupported["pages-index"][pageId].objects[shapeId].type = "vector-network";
  assert.throws(
    () => projectP04RendererFixture(unsupported),
    (error) => error instanceof UnsupportedP04RendererShapeError && error.shapeType === "vector-network",
  );
});
