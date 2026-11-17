import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { assertCleanSourceStatus, assertLoopbackPreviewUrl, MEMORY_OPERATION_SEQUENCE, summarizeMemoryDeltas,
  validateBuildProvenance, validateMemoryArtifacts, validateMemoryRunAuthorization,
  verifyMemoryArtifactFiles } from "./g1-b9-6-memory-evidence.mjs";
import { EXPECTED_EMSDK_COMMIT, EXPECTED_EMSCRIPTEN_VERSION, EXPECTED_PNPM_VERSION, EXPECTED_RUST_VERSION,
  RENDERER_ASSET_PATHS, assertWasmRuntimeEvidence, createRendererProvenance, validateBuildProvenanceRenderer,
  verifyPreviewEndpoints, verifyRendererAssets } from "./g1-b9-6-renderer-artifacts.mjs";

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
  record.renderer = { schema: "logos.g1-b9-6.renderer-provenance/v1", sourceIdentity: record.sourceIdentity,
    emsdkCommit: EXPECTED_EMSDK_COMMIT, emscriptenVersion: EXPECTED_EMSCRIPTEN_VERSION,
    rustVersion: EXPECTED_RUST_VERSION, cargoVersion: EXPECTED_RUST_VERSION, pnpmVersion: EXPECTED_PNPM_VERSION,
    cmakeVersion: "3.28.3", nodeVersion: "v22.22.0",
    assets: RENDERER_ASSET_PATHS.map((assetPath, index) => ({ path: assetPath, bytes: 100 + index, sha256: String(index).repeat(64) })) };
  record.buildArtifacts.push(...record.renderer.assets.map((asset) => ({ path: "js/" + asset.path, bytes: asset.bytes, sha256: asset.sha256 })));
  assert.equal(validateBuildProvenance(record, record.sourceIdentity).buildArtifacts.length, 6);
  assert.throws(() => validateBuildProvenance(record, `${"f".repeat(40)}:${treeSha}`), /does not match/);
  assert.throws(() => validateBuildProvenance({ ...record, buildArtifacts: [...record.buildArtifacts, record.buildArtifacts[1]] }, record.sourceIdentity), /duplicated/);
  assert.throws(() => validateBuildProvenanceRenderer({ ...record, renderer: { ...record.renderer, sourceIdentity: "f".repeat(40) } }), /does not identify/);
  assert.throws(() => validateBuildProvenanceRenderer({ ...record, buildArtifacts: record.buildArtifacts.filter((item) => item.path !== "js/render-wasm.wasm") }), /do not match/);
});

test("renderer artifact qualification rejects missing, empty, corrupt, or stale files", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "logos-renderer-artifacts-"));
  const sourceIdentity = "a".repeat(40) + ":" + "b".repeat(40);
  const toolchain = { emsdkCommit: EXPECTED_EMSDK_COMMIT, emscriptenVersion: EXPECTED_EMSCRIPTEN_VERSION,
    rustVersion: EXPECTED_RUST_VERSION, cargoVersion: EXPECTED_RUST_VERSION, pnpmVersion: EXPECTED_PNPM_VERSION,
    cmakeVersion: "3.28.3", nodeVersion: "v22.22.0" };
  try {
    await mkdir(path.join(directory, "worker"));
    await writeFile(path.join(directory, "render-wasm.js"), 'const wasm = "render-wasm.wasm?version=release";');
    await writeFile(path.join(directory, "render-wasm.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    await writeFile(path.join(directory, "worker/render.js"), "worker");
    await writeFile(path.join(directory, "shared.js"), "export const Renderer = 1;");
    const provenance = await createRendererProvenance(directory, sourceIdentity, toolchain);
    await writeFile(path.join(directory, "render-wasm-provenance.json"), JSON.stringify(provenance));
    assert.equal((await verifyRendererAssets(directory, sourceIdentity)).assets.length, 4);
    await assert.rejects(verifyRendererAssets(directory, "c".repeat(40) + ":" + "d".repeat(40)), /stale, or mismatched/);
    await writeFile(path.join(directory, "render-wasm.wasm"), Buffer.from("not wasm"));
    await assert.rejects(verifyRendererAssets(directory, sourceIdentity), /checksum or size mismatch/);
    await writeFile(path.join(directory, "render-wasm.wasm"), Buffer.alloc(0));
    await assert.rejects(createRendererProvenance(directory, sourceIdentity, toolchain), /empty/);
    await writeFile(path.join(directory, "render-wasm.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    await rm(path.join(directory, "shared.js"));
    await assert.rejects(createRendererProvenance(directory, sourceIdentity, toolchain), /missing/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("renderer toolchain and endpoint preflight fail closed without the pinned build", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "logos-renderer-toolchain-"));
  try {
    await assert.rejects(createRendererProvenance(directory, "a".repeat(40) + ":" + "b".repeat(40),
      { emsdkCommit: EXPECTED_EMSDK_COMMIT, emscriptenVersion: "4.0.5", rustVersion: EXPECTED_RUST_VERSION,
        cargoVersion: EXPECTED_RUST_VERSION, pnpmVersion: EXPECTED_PNPM_VERSION,
        cmakeVersion: "3.28.3", nodeVersion: "v22.22.0" }), /toolchain/);
  } finally { await rm(directory, { recursive: true, force: true }); }
  await assert.rejects(verifyPreviewEndpoints("https://127.0.0.1/", {}), /loopback HTTP/);
  await assert.rejects(verifyPreviewEndpoints("http://example.test/", {}), /loopback HTTP/);
  assert.throws(() => validateBuildProvenanceRenderer({ sourceIdentity: "source", renderer: { backend: "canvas2d" } }), /verified renderer artifacts/);
  assert.equal(assertWasmRuntimeEvidence({ status: "READY", backend: "wasm", operationSucceeded: true,
    framebufferPixels: [[1, 2, 3, 255]] }), true);
  assert.throws(() => assertWasmRuntimeEvidence({ status: "READY", backend: "canvas2d", operationSucceeded: true,
    framebufferPixels: [[1, 2, 3, 255]] }), /READY\/WASM/);
  assert.throws(() => assertWasmRuntimeEvidence({ status: "READY", backend: "wasm", operationSucceeded: true,
    framebufferPixels: [] }), /framebuffer evidence/);
});

test("local endpoint preflight verifies served bytes and never equates HTTP 200 with initialization", async () => {
  const sourceIdentity = "e".repeat(40) + ":" + "f".repeat(40);
  const html = Buffer.from("<canvas id=canvas></canvas>");
  const bodies = new Map([
    ["/g1-b9-5-editor-performance.html", html],
    ["/js/render-wasm.js", Buffer.from('const wasm = "render-wasm.wasm?version=release";')],
    ["/js/render-wasm.wasm", Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])],
    ["/js/worker/render.js", Buffer.from("worker")],
    ["/js/shared.js", Buffer.from("export const shared = true;")],
  ]);
  const assets = RENDERER_ASSET_PATHS.map((assetPath) => {
    const body = bodies.get("/js/" + assetPath);
    return { path: assetPath, bytes: body.length, sha256: createHash("sha256").update(body).digest("hex") };
  });
  const renderer = { schema: "logos.g1-b9-6.renderer-provenance/v1", sourceIdentity,
    emsdkCommit: EXPECTED_EMSDK_COMMIT, emscriptenVersion: EXPECTED_EMSCRIPTEN_VERSION,
    rustVersion: EXPECTED_RUST_VERSION, cargoVersion: EXPECTED_RUST_VERSION, pnpmVersion: EXPECTED_PNPM_VERSION,
    cmakeVersion: "3.28.3", nodeVersion: "v22.22.0", assets };
  const build = { schema: "logos.g1-b9-6.build-provenance/v1", sourceIdentity,
    entry: "g1-b9-5-editor-performance.html", renderer,
    buildArtifacts: [{ path: "g1-b9-5-editor-performance.html", bytes: html.length,
      sha256: createHash("sha256").update(html).digest("hex") },
    ...assets.map((asset) => ({ ...asset, path: "js/" + asset.path }))] };
  const server = createServer((request, response) => {
    if (request.url === "/g1-b9-6-build-provenance.json") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(build));
      return;
    }
    const body = bodies.get(request.url);
    response.writeHead(200, { "content-type": "application/octet-stream" });
    response.end(body ?? Buffer.from("HTTP 200 fallback body"));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  const previewUrl = "http://127.0.0.1:" + address.port + "/g1-b9-5-editor-performance.html";
  try {
    const result = await verifyPreviewEndpoints(previewUrl, build);
    assert.equal(result.status, "ENDPOINTS_VERIFIED_NOT_INITIALIZED");
    assert.equal(result.responses.length, 5);
    bodies.set("/g1-b9-5-editor-performance.html", Buffer.from("HTTP 200 but wrong page"));
    await assert.rejects(verifyPreviewEndpoints(previewUrl, build), /checksum mismatch/);
    bodies.set("/g1-b9-5-editor-performance.html", html);
    bodies.set("/js/render-wasm.wasm", Buffer.from("HTTP 200, but corrupt"));
    await assert.rejects(verifyPreviewEndpoints(previewUrl, build), /checksum mismatch/);
  } finally { await new Promise((resolve) => server.close(resolve)); }
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
