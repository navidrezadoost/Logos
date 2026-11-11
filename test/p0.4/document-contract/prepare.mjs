import { randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { encodeGeometryChange, projectLegacyFileData, NIL_UUID } from "../../../logos-app/src/test-support/p04-document-adapter.ts";

const outDir = process.argv[2];
if (!outDir) throw new Error("usage: prepare.mjs <temporary-output-directory>");
const fixturePath = fileURLToPath(new URL("../../../test/fixtures/p0-document-compat/source-document.json", import.meta.url));
const source = JSON.parse(readFileSync(fixturePath, "utf8"));
const fileId = randomUUID();
source.id = fileId;

const before = projectLegacyFileData(source);
const pageId = before.currentPageId;
const shapeId = before.pages[pageId].rootShapeIds[0];
const oldShape = before.shapes[shapeId];
const updatedBounds = { ...oldShape.bounds, x: oldShape.bounds.x + 5 };
const change = encodeGeometryChange(before, shapeId, updatedBounds);
const staleBounds = { ...oldShape.bounds, x: oldShape.bounds.x + 10 };
const staleChange = encodeGeometryChange(before, shapeId, staleBounds);

const request = {
  id: fileId,
  "session-id": randomUUID(),
  revn: 0,
  vern: 0,
  changes: [change],
};
const staleRequest = {
  ...request,
  "session-id": randomUUID(),
  changes: [staleChange],
};
const input = {
  fileId,
  name: "P04 fixture persistence proof",
  request,
  staleRequest,
  browserFileId: randomUUID(),
  browserName: "P05 browser persistence proof",
};
const expected = {
  fileId,
  pageId,
  shapeId,
  oldBounds: oldShape.bounds,
  finalBounds: staleBounds,
  expectedFill: oldShape.fills[0],
  expectedExtension: before.unknownFieldsById[shapeId]["extension-p0"],
  expectedRootOrder: before.pages[pageId].rootShapeIds,
};

writeFileSync(`${outDir}/request.json`, JSON.stringify(input));
writeFileSync(`${outDir}/expected.json`, JSON.stringify(expected));
