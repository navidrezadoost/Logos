import { createHash, randomUUID } from "node:crypto";
import { dirname } from "node:path";
import { link, mkdir, open, readFile, rm, unlink } from "node:fs/promises";

// CDP capture primitive only. This does NOT qualify editor memory performance.
export async function captureChromiumHeapSnapshot({ session, filePath, runId, browserSessionId, fixtureHash, checkpoint, authorizedIsolatedRun = false }) {
  if (!authorizedIsolatedRun) throw new Error("BLOCKED: independent disposable-environment authorization required");
  for (const [key, value] of Object.entries({ filePath, runId, browserSessionId, fixtureHash, checkpoint })) {
    if (typeof value !== "string" || !value.trim()) throw new Error(`BLOCKED: ${key} required`);
  }
  if (!session || !["send", "on", "off"].every((key) => typeof session[key] === "function"))
    throw new Error("BLOCKED: Chromium CDP session required");
  await mkdir(dirname(filePath), { recursive: true });
  const partial = `${filePath}.partial-${randomUUID()}`;
  let file, listener;
  try {
    await session.send("HeapProfiler.enable");
    try { await session.send("HeapProfiler.collectGarbage"); }
    catch { throw new Error("BLOCKED: explicit Chromium GC unavailable"); }
    file = await open(partial, "wx", 0o600);
    const hash = createHash("sha256");
    let chunks = 0, bytes = 0, invalid = false, writes = Promise.resolve();
    listener = ({ chunk } = {}) => {
      if (typeof chunk !== "string" || !chunk.length) { invalid = true; return; }
      const buffer = Buffer.from(chunk, "utf8");
      chunks++; bytes += buffer.length; hash.update(buffer);
      writes = writes.then(async () => {
        let offset = 0;
        while (offset < buffer.length) {
          const { bytesWritten } = await file.write(buffer, offset, buffer.length - offset);
          if (!Number.isInteger(bytesWritten) || bytesWritten <= 0) throw new Error("BLOCKED: partial snapshot write");
          offset += bytesWritten;
        }
      });
    };
    session.on("HeapProfiler.addHeapSnapshotChunk", listener);
    let error = null;
    try { await session.send("HeapProfiler.takeHeapSnapshot", { reportProgress: false }); }
    catch (e) { error = e; }
    try { await writes; } catch (e) { error ??= e; }
    session.off("HeapProfiler.addHeapSnapshotChunk", listener); listener = null;
    if (error || invalid || !chunks || !bytes) throw new Error("BLOCKED: incomplete Chromium snapshot", { cause: error });
    await file.sync(); await file.close(); file = null;
    let parsed;
    try { parsed = JSON.parse(await readFile(partial, "utf8")); }
    catch { throw new Error("BLOCKED: malformed snapshot JSON"); }
    const fields = parsed?.snapshot?.meta?.node_fields;
    const nodes = parsed?.nodes;
    if (!Array.isArray(fields) || !fields.includes("type") || !fields.includes("self_size") ||
        !Array.isArray(nodes) || !nodes.length || nodes.length % fields.length ||
        !Array.isArray(parsed.strings) || !Array.isArray(parsed.edges))
      throw new Error("BLOCKED: malformed snapshot structure");
    let snapshotSelfSizeBytes = 0;
    for (let i = fields.indexOf("self_size"); i < nodes.length; i += fields.length) {
      if (!Number.isSafeInteger(nodes[i]) || nodes[i] < 0) throw new Error("BLOCKED: invalid self_size");
      snapshotSelfSizeBytes += nodes[i];
      if (!Number.isSafeInteger(snapshotSelfSizeBytes)) throw new Error("BLOCKED: size overflow");
    }
    await link(partial, filePath); // no-clobber publish
    await unlink(partial);
    return { status: "CAPTURED_NOT_QUALIFIED", runId, browserSessionId, fixtureHash, checkpoint,
      gcPolicy: "EXPLICIT_CHROMIUM_GC_BEFORE_SNAPSHOT", gcInvoked: true,
      bytes, chunks, sha256: hash.digest("hex"), nodeCount: nodes.length / fields.length, snapshotSelfSizeBytes };
  } finally {
    if (listener) session.off("HeapProfiler.addHeapSnapshotChunk", listener);
    if (file) await file.close().catch(() => {});
    await rm(partial, { force: true });
  }
}

export function comparePostGcSnapshots(baseline, after) {
  for (const record of [baseline, after]) {
    if (record?.status !== "CAPTURED_NOT_QUALIFIED" || record.gcInvoked !== true ||
        record.gcPolicy !== "EXPLICIT_CHROMIUM_GC_BEFORE_SNAPSHOT" ||
        !Number.isSafeInteger(record.snapshotSelfSizeBytes) || record.snapshotSelfSizeBytes < 0)
      throw new Error("BLOCKED: invalid post-GC snapshot");
  }
  for (const key of ["runId", "browserSessionId", "fixtureHash"])
    if (!baseline[key] || baseline[key] !== after[key]) throw new Error(`BLOCKED: ${key} mismatch`);
  if (baseline.checkpoint === after.checkpoint || baseline.sha256 === after.sha256)
    throw new Error("BLOCKED: snapshots must be distinct");
  return { status: "OBSERVED_POST_GC_SNAPSHOT_DELTA_NOT_QUALIFIED",
    runId: baseline.runId, browserSessionId: baseline.browserSessionId, fixtureHash: baseline.fixtureHash,
    baselineCheckpoint: baseline.checkpoint, afterCheckpoint: after.checkpoint,
    deltaSnapshotSelfSizeBytes: after.snapshotSelfSizeBytes - baseline.snapshotSelfSizeBytes };
}
