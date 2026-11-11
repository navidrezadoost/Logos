import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { projectLegacyFileData } from "../../../logos-app/src/test-support/p04-document-adapter.ts";

const dir = process.argv[2];
if (!dir) throw new Error("usage: verify.mjs <temporary-output-directory>");
const expected = JSON.parse(readFileSync(`${dir}/expected.json`, "utf8"));
const beforeDetail = JSON.parse(readFileSync(`${dir}/before.json`, "utf8"));
const reloadedDetail = JSON.parse(readFileSync(`${dir}/reload.json`, "utf8"));

assert.equal(beforeDetail.id, expected.fileId);
assert.equal(beforeDetail.revn, 0);
assert.equal(reloadedDetail.id, expected.fileId);
assert.equal(reloadedDetail.revn, 1, "authoritative Go API must return incremented revision");

const before = projectLegacyFileData(beforeDetail.data);
const after = projectLegacyFileData(reloadedDetail.data);
assert.equal(before.documentId, after.documentId);
assert.deepEqual(after.pageOrder, before.pageOrder);
assert.deepEqual(after.pages[expected.pageId].rootShapeIds, expected.expectedRootOrder);
assert.equal(after.shapes[expected.shapeId].id, expected.shapeId);
assert.equal(after.shapes[expected.shapeId].parentId, null);
assert.deepEqual(after.shapes[expected.shapeId].bounds, expected.updatedBounds);
assert.deepEqual(after.shapes[expected.shapeId].fills[0], expected.expectedFill);
assert.deepEqual(after.unknownFieldsById[expected.shapeId]["extension-p0"], expected.expectedExtension);

console.log(JSON.stringify({
  adapter: after.adapterVersion,
  fileId: after.documentId,
  pageId: expected.pageId,
  shapeId: expected.shapeId,
  revision: reloadedDetail.revn,
  before: before.shapes[expected.shapeId].bounds,
  after: after.shapes[expected.shapeId].bounds,
  fill: after.shapes[expected.shapeId].fills[0],
  extension: after.unknownFieldsById[expected.shapeId]["extension-p0"],
}, null, 2));
