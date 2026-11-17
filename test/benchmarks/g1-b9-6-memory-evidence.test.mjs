import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { assertCleanSourceStatus, assertLoopbackPreviewUrl, MEMORY_OPERATION_SEQUENCE, summarizeMemoryDeltas,
  validateBuildProvenance, validateMemoryArtifacts, validateMemoryRunAuthorization,
  verifyMemoryArtifactFiles } from "./g1-b9-6-memory-evidence.mjs";

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

test("preview provenance must match the exact source commit and tree and enumerate a unique build", () => {
  const commitSha = "a".repeat(40), treeSha = "b".repeat(40);
  const record = { schema: "logos.g1-b9-6.build-provenance/v1", commitSha, treeSha,
    sourceIdentity: `${commitSha}:${treeSha}`, entry: "g1-b9-5-editor-performance.html",
    buildArtifacts: [{ path: "g1-b9-5-editor-performance.html", bytes: 20, sha256: "c".repeat(64) },
      { path: "assets/app.js", bytes: 100, sha256: "d".repeat(64) }] };
  assert.equal(validateBuildProvenance(record, record.sourceIdentity).buildArtifacts.length, 2);
  assert.throws(() => validateBuildProvenance(record, `${"f".repeat(40)}:${treeSha}`), /does not match/);
  assert.throws(() => validateBuildProvenance({ ...record, buildArtifacts: [...record.buildArtifacts, record.buildArtifacts[1]] }, record.sourceIdentity), /duplicated/);
});

test("memory build and execution require a clean source worktree", () => {
  assert.equal(assertCleanSourceStatus(""), true);
  assert.throws(() => assertCleanSourceStatus(" M logos-app/src/Canvas.tsx\n"), /worktree must be clean/);
});

function evidenceFixture() {
  const runId = "run-1", browserSessionId = "browser-1", fixtureHash = "fixture-1";
  const checkpoint = (checkpointName, bytes, selfSize) => ({ status: "CAPTURED_NOT_QUALIFIED", runId,
    browserSessionId, fixtureHash, checkpoint: checkpointName, artifactId: `${checkpointName}.heapsnapshot`,
    file: `snapshots/${checkpointName}.heapsnapshot`, sha256: "a".repeat(64), bytes, gcInvoked: true, snapshotSelfSizeBytes: selfSize });
  const baseline = checkpoint("baseline", 123, 100);
  const after = checkpoint("after", 124, 110);
  const cycle = { operations: MEMORY_OPERATION_SEQUENCE.map((operation, index) => ({ operation,
    rustSemanticEquivalent: true, sampleId: `sample-${index}`, expectedFrameGeneration: index + 1,
    stateBeforeSha256: "before", stateAfterSha256: "after", selectedShapeIds: [],
    observedPixels: { before: {}, after: {} } })) };
  const run = { status: "CAPTURED_NOT_QUALIFIED", runId, browserSessionId,
    renderer: { backend: "wasm", wasmSha256: "renderer" },
    fixtures: [{ size: 21, fixtureHash, checkpoints: [baseline, after], cycleEvidence: [cycle],
      snapshotSelfSizeDeltaBytes: { sampleCount: 1 } }] };
  const refs = [baseline, after].map(({ file, sha256, bytes, ...association }) => ({ file, sha256, bytes, ...association }));
  return { run, refs, baseline, after };
}

test("missing, duplicated, modified or incorrectly associated snapshots cannot validate", () => {
  const { run, refs } = evidenceFixture();
  assert.equal(validateMemoryArtifacts([run], [21], 1, 1, refs), true);
  assert.throws(() => validateMemoryArtifacts([], [21], 1, 1), /browser launch/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, renderer: { backend: "canvas2d", wasmSha256: "renderer" } }], [21], 1, 1), /renderer evidence/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, fixtures: [{ ...run.fixtures[0], checkpoints: [] }] }], [21], 1, 1, refs), /baseline or measured/);
  assert.throws(() => validateMemoryArtifacts([run], [21], 1, 1, refs.slice(1)), /does not match captured/);
  assert.throws(() => validateMemoryArtifacts([run], [21], 1, 1, [refs[0], refs[0]]), /duplicate or incorrectly associated/);
  assert.throws(() => validateMemoryArtifacts([run], [21], 1, 1, [{ ...refs[0], browserSessionId: "wrong" }, refs[1]]), /incorrectly associated/);
  assert.throws(() => validateMemoryArtifacts([{ ...run, fixtures: [{ ...run.fixtures[0], cycleEvidence: [{ operations: [] }] }] }], [21], 1, 1, refs), /deterministic operation/);
});

test("final snapshot checksums are recomputed and missing or mutated files block finalization", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "logos-mem-artifacts-"));
  try {
    await mkdir(path.join(directory, "snapshots"));
    const file = "snapshots/one.heap", payload = Buffer.from("snapshot bytes");
    await writeFile(path.join(directory, file), payload);
    const reference = { file, bytes: payload.length, sha256: createHash("sha256").update(payload).digest("hex") };
    assert.equal(await verifyMemoryArtifactFiles(directory, [reference]), true);
    await assert.rejects(verifyMemoryArtifactFiles(directory, [reference, reference]), /duplicated/);
    await writeFile(path.join(directory, file), "changed snapshot bytes");
    await assert.rejects(verifyMemoryArtifactFiles(directory, [reference]), /size differs|checksum differs/);
    await assert.rejects(verifyMemoryArtifactFiles(directory, [{ ...reference, file: "snapshots/missing.heap" }]), /ENOENT/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
