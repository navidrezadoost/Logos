import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { captureChromiumHeapSnapshot, comparePostGcSnapshots } from "./g1-b9-6-memory-capture.mjs";

const sample = JSON.stringify({
  snapshot: { meta: { node_fields: ["type", "name", "id", "self_size", "edge_count"] } },
  nodes: [0, 0, 1, 24, 0, 0, 0, 3, 16, 0], edges: [], strings: ["object"],
});
class FakeCDP extends EventEmitter {
  constructor({ chunks = [sample.slice(0, 23), sample.slice(23)], failGC = false, failSnapshot = false } = {}) {
    super(); this.chunks = chunks; this.failGC = failGC; this.failSnapshot = failSnapshot; this.calls = [];
  }
  async send(method) {
    this.calls.push(method);
    if (method === "HeapProfiler.collectGarbage" && this.failGC) throw Error("unsupported");
    if (method === "HeapProfiler.takeHeapSnapshot") {
      for (const chunk of this.chunks) this.emit("HeapProfiler.addHeapSnapshotChunk", { chunk });
      if (this.failSnapshot) throw Error("disconnected");
    }
  }
}
const options = (session, filePath) => ({
  session, filePath, runId: "run-1", browserSessionId: "browser-1", fixtureHash: "fixture-sha256",
  checkpoint: "post-warmup", authorizedIsolatedRun: true,
});
async function withTemp(fn) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "logos-b96-"));
  try { await fn(dir); } finally { await rm(dir, { recursive: true, force: true }); }
}
test("captures complete chunks, validates self sizes, hashes and publishes without overwriting", async () => withTemp(async (dir) => {
  const filePath = path.join(dir, "raw", "baseline.heapsnapshot");
  const cdp = new FakeCDP();
  const result = await captureChromiumHeapSnapshot(options(cdp, filePath));
  assert.equal(result.status, "CAPTURED_NOT_QUALIFIED");
  assert.equal(result.gcInvoked, true);
  assert.equal(result.nodeCount, 2);
  assert.equal(result.snapshotSelfSizeBytes, 40);
  assert.equal(result.chunks, 2);
  assert.equal(result.sha256, createHash("sha256").update(sample).digest("hex"));
  assert.equal(await readFile(filePath, "utf8"), sample);
  assert.deepEqual(cdp.calls, ["HeapProfiler.enable", "HeapProfiler.collectGarbage", "HeapProfiler.takeHeapSnapshot"]);
  assert.equal(cdp.listenerCount("HeapProfiler.addHeapSnapshotChunk"), 0);
  await assert.rejects(captureChromiumHeapSnapshot(options(new FakeCDP(), filePath)), /EEXIST/);
  assert.equal(await readFile(filePath, "utf8"), sample);
}));
test("refuses capture without authorization", async () => withTemp(async (dir) => {
  const cdp = new FakeCDP();
  await assert.rejects(captureChromiumHeapSnapshot({ ...options(cdp, path.join(dir, "heap")), authorizedIsolatedRun: false }), /authorization/);
  assert.deepEqual(cdp.calls, []);
}));
test("unavailable GC blocks capture", async () => withTemp(async (dir) => {
  const cdp = new FakeCDP({ failGC: true });
  await assert.rejects(captureChromiumHeapSnapshot(options(cdp, path.join(dir, "heap"))), /GC unavailable/);
  assert.equal(cdp.calls.includes("HeapProfiler.takeHeapSnapshot"), false);
  assert.deepEqual(await readdir(dir), []);
}));
test("missing chunks, malformed snapshots and CDP failure clean up", async () => withTemp(async (dir) => {
  for (const [name, cdp] of [
    ["empty", new FakeCDP({ chunks: [] })],
    ["malformed", new FakeCDP({ chunks: ['{"snapshot":{}'] })],
    ["cdp-error", new FakeCDP({ failSnapshot: true })],
    ["invalid-size", new FakeCDP({ chunks: [sample.replace('24', '-2')] })],
  ]) {
    await assert.rejects(captureChromiumHeapSnapshot(options(cdp, path.join(dir, name))), /BLOCKED/);
    assert.equal(cdp.listenerCount("HeapProfiler.addHeapSnapshotChunk"), 0);
  }
  assert.deepEqual(await readdir(dir), []);
}));
test("compares only distinct post-GC checkpoints in one run/browser/fixture", () => {
  const baseline = { status: "CAPTURED_NOT_QUALIFIED", gcInvoked: true,
    gcPolicy: "EXPLICIT_CHROMIUM_GC_BEFORE_SNAPSHOT", runId: "r", browserSessionId: "b",
    fixtureHash: "f", checkpoint: "baseline", sha256: "a", snapshotSelfSizeBytes: 200 };
  const after = { ...baseline, checkpoint: "after-cycle", sha256: "b", snapshotSelfSizeBytes: 175 };
  assert.equal(comparePostGcSnapshots(baseline, after).deltaSnapshotSelfSizeBytes, -25);
  assert.throws(() => comparePostGcSnapshots(baseline, { ...after, fixtureHash: "other" }), /fixtureHash mismatch/);
  assert.throws(() => comparePostGcSnapshots(baseline, { ...after, browserSessionId: "other" }), /browserSessionId mismatch/);
  assert.throws(() => comparePostGcSnapshots(baseline, { ...after, sha256: "a" }), /distinct/);
  assert.throws(() => comparePostGcSnapshots(baseline, { ...after, gcInvoked: false }), /invalid/);
});
