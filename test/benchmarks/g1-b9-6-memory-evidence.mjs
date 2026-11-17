import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { realpath, stat } from "node:fs/promises";
import path from "node:path";
import { validateBuildProvenanceRenderer } from "./g1-b9-6-renderer-artifacts.mjs";

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

export function validateBuildProvenance(record, expectedSourceIdentity) {
  if (!record || record.schema !== "logos.g1-b9-6.build-provenance/v1" ||
      !/^[a-f0-9]{40}$/i.test(record.commitSha ?? "") || !/^[a-f0-9]{40}$/i.test(record.treeSha ?? "") ||
      record.sourceIdentity !== `${record.commitSha}:${record.treeSha}` || record.sourceIdentity !== expectedSourceIdentity ||
      record.entry !== "g1-b9-5-editor-performance.html" || !Array.isArray(record.buildArtifacts) || !record.buildArtifacts.length)
    throw new Error("BLOCKED: preview build provenance is missing or does not match the clean source revision");
  const paths = new Set();
  for (const artifact of record.buildArtifacts) {
    if (!artifact || typeof artifact.path !== "string" || artifact.path.startsWith("/") ||
        artifact.path.split("/").some((part) => part === ".." || part === "") || paths.has(artifact.path) ||
        !Number.isSafeInteger(artifact.bytes) || artifact.bytes < 0 || !/^[a-f0-9]{64}$/i.test(artifact.sha256 ?? ""))
      throw new Error("BLOCKED: preview build artifact provenance is malformed or duplicated");
    paths.add(artifact.path);
  }
  if (!paths.has(record.entry)) throw new Error("BLOCKED: preview build entry is not in the artifact set");
  validateBuildProvenanceRenderer(record);
  return { commitSha: record.commitSha, treeSha: record.treeSha, sourceIdentity: record.sourceIdentity,
    entry: record.entry, renderer: record.renderer, buildArtifacts: record.buildArtifacts };
}

export function assertCleanSourceStatus(statusOutput) {
  if (typeof statusOutput !== "string" || statusOutput.trim())
    throw new Error("BLOCKED: source worktree must be clean before memory build or execution");
  return true;
}

export function validateMemoryArtifacts(runRecords, expectedFixtures, expectedRounds, measuredCycles, artifactReferences) {
  if (!Array.isArray(runRecords) || runRecords.length !== expectedRounds) throw new Error("BLOCKED: missing browser launch records");
  const expectedFiles = new Map();
  const expectedArtifactIds = new Set();
  for (const run of runRecords) {
    if (run.status !== "CAPTURED_NOT_QUALIFIED" || !run.runId || !run.browserSessionId || !run.renderer?.wasmSha256 || run.renderer.backend !== "wasm")
      throw new Error("BLOCKED: browser launch or production renderer evidence missing");
    if (!Array.isArray(run.fixtures) || run.fixtures.length !== expectedFixtures.length) throw new Error("BLOCKED: missing fixture records");
    for (let index = 0; index < expectedFixtures.length; index += 1) {
      const fixture = run.fixtures[index];
      if (fixture.size !== expectedFixtures[index] || fixture.checkpoints?.length !== measuredCycles + 1 ||
          fixture.cycleEvidence?.length !== measuredCycles || fixture.snapshotSelfSizeDeltaBytes?.sampleCount !== measuredCycles)
        throw new Error("BLOCKED: missing baseline or measured checkpoints");
      if (fixture.checkpoints.some((checkpoint) => checkpoint.status !== "CAPTURED_NOT_QUALIFIED" ||
          checkpoint.runId !== run.runId || checkpoint.browserSessionId !== run.browserSessionId ||
          checkpoint.fixtureHash !== fixture.fixtureHash || !checkpoint.gcInvoked ||
          !checkpoint.file || !/^[a-f0-9]{64}$/.test(checkpoint.sha256 ?? "") ||
          !Number.isSafeInteger(checkpoint.bytes) || checkpoint.bytes <= 0 || checkpoint.gcInvoked !== true))
        throw new Error("BLOCKED: incomplete raw snapshot evidence");
      for (const checkpoint of fixture.checkpoints) {
        if (checkpoint.file.startsWith("/") || checkpoint.file.split("/").some((part) => part === ".." || part === ""))
          throw new Error("BLOCKED: raw snapshot path is invalid");
        if (!checkpoint.artifactId || path.posix.basename(checkpoint.file) !== checkpoint.artifactId ||
            expectedFiles.has(checkpoint.file) || expectedArtifactIds.has(checkpoint.artifactId))
          throw new Error("BLOCKED: raw snapshot artifact is duplicated or misidentified");
        expectedArtifactIds.add(checkpoint.artifactId);
        expectedFiles.set(checkpoint.file, checkpoint);
      }
      for (const cycle of fixture.cycleEvidence) {
        if (!Array.isArray(cycle.operations) || cycle.operations.length !== MEMORY_OPERATION_SEQUENCE.length ||
            cycle.operations.some((step, operationIndex) => step.operation !== MEMORY_OPERATION_SEQUENCE[operationIndex] ||
              step.rustSemanticEquivalent !== true || !step.sampleId || !Number.isSafeInteger(step.expectedFrameGeneration) ||
              !step.stateBeforeSha256 || !step.stateAfterSha256 || !step.selectedShapeIds || !step.observedPixels?.before || !step.observedPixels?.after))
          throw new Error("BLOCKED: deterministic operation or visible-output evidence missing");
      }
    }
  }
  if (!Array.isArray(artifactReferences) || artifactReferences.length !== expectedFiles.size)
    throw new Error("BLOCKED: artifact manifest does not match captured snapshots");
  const referencePaths = new Set();
  for (const reference of artifactReferences) {
    const checkpoint = expectedFiles.get(reference?.file);
    if (!checkpoint || referencePaths.has(reference.file) || reference.sha256 !== checkpoint.sha256 ||
        reference.bytes !== checkpoint.bytes || reference.runId !== checkpoint.runId ||
        reference.browserSessionId !== checkpoint.browserSessionId || reference.fixtureHash !== checkpoint.fixtureHash ||
        reference.checkpoint !== checkpoint.checkpoint || reference.status !== checkpoint.status)
      throw new Error("BLOCKED: artifact manifest contains a duplicate or incorrectly associated snapshot");
    referencePaths.add(reference.file);
  }
  return true;
}
export async function verifyMemoryArtifactFiles(directory, references) {
  if (!Array.isArray(references) || !references.length) throw new Error("BLOCKED: snapshot artifact references are missing");
  const root = await realpath(directory);
  const seen = new Set();
  for (const reference of references) {
    if (!reference || typeof reference.file !== "string" || reference.file.startsWith("/") ||
        reference.file.split("/").some((part) => part === ".." || part === "") || seen.has(reference.file) ||
        !Number.isSafeInteger(reference.bytes) || reference.bytes <= 0 || !/^[a-f0-9]{64}$/.test(reference.sha256 ?? ""))
      throw new Error("BLOCKED: snapshot artifact reference is malformed or duplicated");
    seen.add(reference.file);
    const artifactPath = path.resolve(root, reference.file);
    if (!artifactPath.startsWith(`${root}${path.sep}`)) throw new Error("BLOCKED: snapshot path escaped the private run directory");
    const actualPath = await realpath(artifactPath);
    if (!actualPath.startsWith(`${root}${path.sep}`)) throw new Error("BLOCKED: snapshot symlink escaped the private run directory");
    const details = await stat(actualPath);
    if (!details.isFile() || details.size !== reference.bytes) throw new Error("BLOCKED: snapshot size differs from manifest");
    const hash = createHash("sha256");
    for await (const chunk of createReadStream(actualPath)) hash.update(chunk);
    if (hash.digest("hex") !== reference.sha256) throw new Error("BLOCKED: snapshot checksum differs from manifest");
  }
  return true;
}
