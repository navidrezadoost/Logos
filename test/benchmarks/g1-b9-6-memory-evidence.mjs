export const MEMORY_OPERATION_SEQUENCE = Object.freeze([
  "sibling-reorder", "root-reorder", "create", "insert-child", "rename", "reparent", "subtree-delete",
]);

export function validateMemoryRunAuthorization(record, now = Date.now()) {
  if (!record || typeof record !== "object") throw new Error("BLOCKED: owner authorization record required");
  const requiredStrings = ["owner", "approvalRecord", "environmentClassification", "scope", "approvedAt", "expiresAt"];
  for (const key of requiredStrings) {
    if (typeof record[key] !== "string" || !record[key].trim()) throw new Error(`BLOCKED: authorization ${key} required`);
  }
  if (record.environmentClassification !== "isolated-disposable") throw new Error("BLOCKED: environment must be classified isolated-disposable by its owner");
  if (!/^G1-B9\.6(?:\.2)?(?:\b|$)/.test(record.scope.trim())) throw new Error("BLOCKED: authorization scope must include G1-B9.6");
  const approvedAt = Date.parse(record.approvedAt);
  const expiresAt = Date.parse(record.expiresAt);
  if (!Number.isFinite(approvedAt) || !Number.isFinite(expiresAt) || approvedAt > now || expiresAt <= now || expiresAt <= approvedAt)
    throw new Error("BLOCKED: authorization validity window is invalid or expired");
  return Object.freeze({
    owner: record.owner.trim(), approvalRecord: record.approvalRecord.trim(),
    environmentClassification: record.environmentClassification, scope: record.scope.trim(),
    approvedAt: new Date(approvedAt).toISOString(), expiresAt: new Date(expiresAt).toISOString(),
  });
}

export function summarizeMemoryDeltas(values) {
  if (!Array.isArray(values) || values.length < 2 || values.some((value) => !Number.isSafeInteger(value)))
    throw new Error("BLOCKED: at least two integer snapshot deltas required");
  const sorted = [...values].sort((a, b) => a - b);
  return {
    sampleCount: sorted.length,
    p50: sorted[Math.floor((sorted.length - 1) * 0.5)],
    p95: sorted[Math.ceil(sorted.length * 0.95) - 1],
    minimum: sorted[0], maximum: sorted.at(-1),
    mean: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
  };
}

export function assertLoopbackPreviewUrl(input) {
  let url;
  try { url = new URL(input); } catch { throw new Error("BLOCKED: valid local preview URL required"); }
  if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname))
    throw new Error("BLOCKED: memory runner accepts only an HTTP loopback preview URL");
  if (url.pathname !== "/g1-b9-5-editor-performance.html" || url.username || url.password || url.search || url.hash)
    throw new Error("BLOCKED: URL must address the production Canvas performance smoke page without credentials or query data");
  return url;
}

export function validateMemoryArtifacts(runRecords, expectedFixtures, expectedRounds, measuredCycles) {
  if (!Array.isArray(runRecords) || runRecords.length !== expectedRounds) throw new Error("BLOCKED: missing browser launch records");
  for (const run of runRecords) {
    if (run.status !== "CAPTURED_NOT_QUALIFIED" || !run.browserSessionId || !run.renderer?.wasmSha256 || run.renderer.backend !== "wasm")
      throw new Error("BLOCKED: browser launch or production renderer evidence missing");
    if (!Array.isArray(run.fixtures) || run.fixtures.length !== expectedFixtures.length) throw new Error("BLOCKED: missing fixture records");
    for (let index = 0; index < expectedFixtures.length; index += 1) {
      const fixture = run.fixtures[index];
      if (fixture.size !== expectedFixtures[index] || fixture.checkpoints?.length !== measuredCycles + 1 ||
          fixture.cycleEvidence?.length !== measuredCycles || fixture.snapshotSelfSizeDeltaBytes?.sampleCount !== measuredCycles)
        throw new Error("BLOCKED: missing baseline or measured checkpoints");
      if (fixture.checkpoints.some((checkpoint) => checkpoint.status !== "CAPTURED_NOT_QUALIFIED" ||
          !checkpoint.file || !/^[a-f0-9]{64}$/.test(checkpoint.sha256 ?? "") ||
          !Number.isSafeInteger(checkpoint.bytes) || checkpoint.bytes <= 0 || checkpoint.gcInvoked !== true))
        throw new Error("BLOCKED: incomplete raw snapshot evidence");
      for (const cycle of fixture.cycleEvidence) {
        if (!Array.isArray(cycle.operations) || cycle.operations.length !== MEMORY_OPERATION_SEQUENCE.length ||
            cycle.operations.some((step, operationIndex) => step.operation !== MEMORY_OPERATION_SEQUENCE[operationIndex] ||
              step.rustSemanticEquivalent !== true || !step.sampleId || !Number.isSafeInteger(step.expectedFrameGeneration) ||
              !step.stateBeforeSha256 || !step.stateAfterSha256 || !step.selectedShapeIds || !step.observedPixels?.before || !step.observedPixels?.after))
          throw new Error("BLOCKED: deterministic operation or visible-output evidence missing");
      }
    }
  }
  return true;
}
