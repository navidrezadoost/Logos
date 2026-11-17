import assert from "node:assert/strict";
import test from "node:test";
import { assertLoopbackPreviewUrl, MEMORY_OPERATION_SEQUENCE, summarizeMemoryDeltas,
  validateMemoryArtifacts, validateMemoryRunAuthorization } from "./g1-b9-6-memory-evidence.mjs";

const now = Date.parse("2026-11-17T08:00:00.000Z");
const authorization = {
  owner: "Authorized test-environment owner", approvalRecord: "OPS-1234",
  environmentClassification: "isolated-disposable", scope: "G1-B9.6.2 browser memory qualification",
  approvedAt: "2026-11-17T07:00:00.000Z", expiresAt: "2026-11-17T09:00:00.000Z",
};

test("owner approval must explicitly classify an isolated disposable environment and valid scope/window", () => {
  assert.equal(validateMemoryRunAuthorization(authorization, now).approvalRecord, "OPS-1234");
  assert.throws(() => validateMemoryRunAuthorization(null, now), /authorization record required/);
  assert.throws(() => validateMemoryRunAuthorization({ ...authorization, environmentClassification: "loopback" }, now), /isolated-disposable/);
  assert.throws(() => validateMemoryRunAuthorization({ ...authorization, scope: "G1-B9.5" }, now), /scope/);
  assert.throws(() => validateMemoryRunAuthorization({ ...authorization, expiresAt: "2026-11-17T07:30:00Z" }, now), /expired/);
});

test("the deterministic memory cycle covers semantic create, insert, edit, reorder, reparent and delete operations", () => {
  assert.deepEqual(MEMORY_OPERATION_SEQUENCE, ["sibling-reorder", "root-reorder", "create", "insert-child", "rename", "reparent", "subtree-delete"]);
});

test("snapshot delta summaries preserve negative values without clipping", () => {
  assert.deepEqual(summarizeMemoryDeltas([-25, 10, 40, 100]), {
    sampleCount: 4, p50: 10, p95: 100, minimum: -25, maximum: 100, mean: 31.25,
  });
  assert.throws(() => summarizeMemoryDeltas([5]), /at least two/);
});

test("only a loopback HTTP preview URL is accepted", () => {
  assert.equal(assertLoopbackPreviewUrl("http://127.0.0.1:4180/g1-b9-5-editor-performance.html").port, "4180");
  assert.throws(() => assertLoopbackPreviewUrl("https://127.0.0.1/test"), /HTTP loopback/);
  assert.throws(() => assertLoopbackPreviewUrl("http://example.test/test"), /HTTP loopback/);
  assert.throws(() => assertLoopbackPreviewUrl("http://127.0.0.1:4180/other.html"), /performance smoke page/);
  assert.throws(() => assertLoopbackPreviewUrl("http://user:secret@127.0.0.1:4180/g1-b9-5-editor-performance.html"), /credentials/);
});

test("missing run, fixture, renderer or raw snapshot evidence cannot pass artifact validation", () => {
  const checkpoint = { status: "CAPTURED_NOT_QUALIFIED", file: "raw.heap", sha256: "a".repeat(64), bytes: 123, gcInvoked: true };
  const cycle = { operations: MEMORY_OPERATION_SEQUENCE.map((operation, index) => ({ operation,
    rustSemanticEquivalent: true, sampleId: `sample-${index}`, expectedFrameGeneration: index + 1,
    stateBeforeSha256: "before", stateAfterSha256: "after", selectedShapeIds: [],
    observedPixels: { before: {}, after: {} } })) };
  const run = { status: "CAPTURED_NOT_QUALIFIED", browserSessionId: "browser", renderer: { backend: "wasm", wasmSha256: "renderer" },
    fixtures: [{ size: 21, checkpoints: [checkpoint, { ...checkpoint, file: "after.heap" }],
      cycleEvidence: [cycle], snapshotSelfSizeDeltaBytes: { sampleCount: 1 } }] };
  assert.equal(validateMemoryArtifacts([run], [21], 1, 1), true);
  assert.throws(() => validateMemoryArtifacts([], [21], 1, 1), /browser launch/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, renderer: { backend: "canvas2d", wasmSha256: "renderer" } }], [21], 1, 1), /renderer evidence/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, fixtures: [{ size: 21, checkpoints: [checkpoint] }] }], [21], 1, 1), /baseline or measured/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, fixtures: [{ ...run.fixtures[0], checkpoints: [checkpoint, { ...checkpoint, sha256: "" }] }] }], [21], 1, 1), /raw snapshot/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, fixtures: [{ ...run.fixtures[0], cycleEvidence: [{ operations: [] }] }] }], [21], 1, 1), /deterministic operation/);
});
