import { createHash, randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createReadStream } from "node:fs";
import { chmod, mkdir, readFile, realpath, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { captureChromiumHeapSnapshot, comparePostGcSnapshots } from "./g1-b9-6-memory-capture.mjs";
import { assertCleanSourceStatus, assertLoopbackPreviewUrl, MEMORY_OPERATION_SEQUENCE, summarizeMemoryDeltas,
  validateBuildProvenance, validateMemoryArtifacts, validateMemoryRunAuthorization,
  verifyMemoryArtifactFiles } from "./g1-b9-6-memory-evidence.mjs";

const [previewInput, artifactsInput, authorizationInput] = process.argv.slice(2);
const sizes = [21, 85, 341];
const warmupCycles = Number(process.env.LOGOS_MEMORY_WARMUP_CYCLES ?? 5);
const measuredCycles = Number(process.env.LOGOS_MEMORY_MEASURED_CYCLES ?? 3);
const browserRounds = 2;
const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const runId = randomUUID();
let runDirectory;
let report;
let sourceProvenance;
let verifiedBuildFiles = new Map();

function sha256(value) { return createHash("sha256").update(value).digest("hex"); }
function stateHash(state) { return sha256(JSON.stringify(state)); }
function writeJson(filePath, value) {
  return writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600, flag: "wx" });
}
function fail(message) { throw new Error(message); }

async function configurePrivateOutput(outputInput) {
  if (!path.isAbsolute(outputInput ?? "")) fail("BLOCKED: absolute artifact directory outside the repository required");
  const output = path.resolve(outputInput);
  const relative = path.relative(repoRoot, output);
  if (!relative || (!relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative)))
    fail("BLOCKED: raw heap artifacts must be outside the Git repository");
  await mkdir(output, { recursive: true, mode: 0o700 });
  const actualOutput = await realpath(output);
  const actualRelative = path.relative(await realpath(repoRoot), actualOutput);
  if (!actualRelative || (!actualRelative.startsWith(`..${path.sep}`) && actualRelative !== ".." && !path.isAbsolute(actualRelative)))
    fail("BLOCKED: resolved artifact directory must remain outside the Git repository");
  await chmod(actualOutput, 0o700);
  return actualOutput;
}

async function waitForCanvas(page, previousGeneration) {
  await page.waitForFunction((previous) => {
    const root = document.querySelector("[data-renderer-status]");
    return root?.getAttribute("data-renderer-status") === "READY" &&
      root.getAttribute("data-renderer-backend") === "wasm" &&
      Number(root.getAttribute("data-renderer-frame-count")) > Number(previous) &&
      Boolean(root.getAttribute("data-renderer-metrics"));
  }, previousGeneration, { timeout: 30000 });
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) hash.update(chunk);
  return hash.digest("hex");
}

function getCleanSourceProvenance() {
  const status = execFileSync("git", ["status", "--porcelain", "--untracked-files=all"], { cwd: repoRoot, encoding: "utf8" });
  assertCleanSourceStatus(status);
  const commitSha = execFileSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" }).trim();
  const treeSha = execFileSync("git", ["rev-parse", "HEAD^{tree}"], { cwd: repoRoot, encoding: "utf8" }).trim();
  if (!/^[a-f0-9]{40}$/i.test(commitSha) || !/^[a-f0-9]{40}$/i.test(treeSha)) fail("BLOCKED: Git source identity is invalid");
  return { commitSha, treeSha, sourceIdentity: `${commitSha}:${treeSha}` };
}

async function verifyPreviewBuild(previewUrl) {
  const provenanceResponse = await fetch(new URL("/g1-b9-6-build-provenance.json", previewUrl));
  if (!provenanceResponse.ok) fail("BLOCKED: source-bound preview build provenance is unavailable");
  let parsed;
  try { parsed = await provenanceResponse.json(); }
  catch { fail("BLOCKED: preview build provenance is malformed"); }
  const provenance = validateBuildProvenance(parsed, sourceProvenance.sourceIdentity);
  for (const artifact of provenance.buildArtifacts) {
    const artifactPath = artifact.path.split("/").map(encodeURIComponent).join("/");
    const response = await fetch(new URL(`/${artifactPath}`, previewUrl));
    if (!response.ok) fail("BLOCKED: a preview build artifact cannot be fetched for source verification");
    const body = Buffer.from(await response.arrayBuffer());
    if (body.length !== artifact.bytes || sha256(body) !== artifact.sha256)
      fail("BLOCKED: served preview artifact does not match its build checksum");
  }
  verifiedBuildFiles = new Map(provenance.buildArtifacts.map((artifact) => [artifact.path, artifact]));
  return { summary: { commitSha: provenance.commitSha, treeSha: provenance.treeSha, sourceIdentity: provenance.sourceIdentity,
    entry: provenance.entry, artifactCount: provenance.buildArtifacts.length,
    artifactSetSha256: sha256(JSON.stringify(provenance.buildArtifacts)) }, files: verifiedBuildFiles };
}

async function verifyFinalArtifactSet(directory, runRecords, references) {
  validateMemoryArtifacts(runRecords, sizes, browserRounds, measuredCycles, references);
  await verifyMemoryArtifactFiles(directory, references);
}

async function setupFixture(page, size, fixtureSerial, sampleId) {
  const oldGeneration = await page.locator("[data-renderer-status]").getAttribute("data-renderer-frame-count");
  await page.evaluate(([workloadSize, serial, id]) => window.__g1b95.prepare(workloadSize, "view-control", serial, id),
    [size, fixtureSerial, sampleId]);
  await waitForCanvas(page, oldGeneration);
  const setup = await page.evaluate(() => {
    const root = document.querySelector("[data-renderer-status]");
    const fixture = window.__g1b95.fixture();
    const state = window.__g1b95.state();
    return {
      generation: Number(root?.getAttribute("data-renderer-frame-count")),
      backend: root?.getAttribute("data-renderer-backend"),
      rendererStatus: root?.getAttribute("data-renderer-status"),
      metrics: root?.getAttribute("data-renderer-metrics"),
      fixture, state, selection: window.__g1b95.selection(),
      pixels: { overlap: window.__g1b95.samplePixel(40, 40), create: window.__g1b95.samplePixel(430, 45), insert: window.__g1b95.samplePixel(130, 130) },
      errors: window.__g1b95.errors(),
    };
  });
  if (setup.rendererStatus !== "READY" || setup.backend !== "wasm" || !setup.metrics) fail("BLOCKED: production Rust/WASM Canvas did not reach READY");
  if (Object.keys(setup.state.shapes).length !== size) fail(`BLOCKED: fixture shape count mismatch for ${size}`);
  if (setup.state.documentId !== null) fail("FAILED: test fixture must not claim production document identity");
  if (setup.errors.length) fail("BLOCKED: browser runtime error during fixture setup");
  return { ...setup, stateDigest: stateHash(setup.state), fixtureHash: sha256(JSON.stringify({ size, fixture: setup.fixture, state: setup.state })) };
}

async function prepareNextOperation(page, operation, serial, sampleId) {
  await page.evaluate(([kind, operationSerial, id]) => window.__g1b95.prepareNext(kind, operationSerial, id),
    [operation, serial, sampleId]);
}

function assertPixelEqual(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) fail(`FAILED: ${label} pixel mismatch`);
}

async function executeOperation(page, { round, size, cycle, operation, operationIndex, fixture }) {
  const sampleId = `mem-r${round}-s${size}-c${cycle}-${operation}`;
  const serial = round * 10_000_000 + size * 10_000 + cycle * 100 + operationIndex + 1;
  const before = await page.evaluate(() => {
    const root = document.querySelector("[data-renderer-status]");
    const state = window.__g1b95.state();
    return { generation: Number(root?.getAttribute("data-renderer-frame-count")), state,
      selection: window.__g1b95.selection(),
      pixels: { overlap: window.__g1b95.samplePixel(40, 40), create: window.__g1b95.samplePixel(430, 45), insert: window.__g1b95.samplePixel(130, 130) } };
  });
  await prepareNextOperation(page, operation, serial, sampleId);
  await page.locator(`[data-g1b95-operation="${operation}"]`).click();
  const frameHandle = await page.waitForFunction((id) => {
    const timing = window.__g1b95.timing(id);
    return timing?.framePublishedAt !== undefined ? timing : false;
  }, sampleId, { timeout: 30000 });
  const observedTiming = await frameHandle.jsonValue();
  await frameHandle.dispose();
  const after = await page.evaluate((id) => {
    const root = document.querySelector("[data-renderer-status]");
    const firstTiming = window.__g1b95.timing(id);
    const result = window.__g1b95.lastResult();
    const state = window.__g1b95.state();
    const selection = window.__g1b95.selection();
    const pixels = { overlap: window.__g1b95.samplePixel(40, 40), create: window.__g1b95.samplePixel(430, 45), insert: window.__g1b95.samplePixel(130, 130) };
    const extraDiagnostics = [window.__g1b95.fixture(), window.__g1b95.errors(), window.__g1b95.state().currentPageId];
    return { frame: Number(root?.getAttribute("data-renderer-frame-count")), status: root?.getAttribute("data-renderer-status"),
      backend: root?.getAttribute("data-renderer-backend"), metrics: root?.getAttribute("data-renderer-metrics"),
      result, state, selection, pixels, errors: window.__g1b95.errors(), firstTiming, finalTiming: window.__g1b95.timing(id), extraDiagnostics };
  }, sampleId);
  await page.evaluate((id) => window.__g1b95.discardTiming(id), sampleId);
  if (JSON.stringify(observedTiming) !== JSON.stringify(after.firstTiming) || JSON.stringify(after.firstTiming) !== JSON.stringify(after.finalTiming))
    fail(`FAILED: post-frame diagnostics changed frozen timing for ${operation}`);
  if (after.status !== "READY" || after.backend !== "wasm" || !after.metrics || after.errors.length) fail(`FAILED: renderer/browser evidence missing after ${operation}`);
  if (!after.result?.ok || after.result.operation !== operation || after.result.sampleId !== sampleId || !after.result.candidate?.semanticEquivalent)
    fail(`FAILED: React/Rust operation conformance for ${operation}`);
  if (after.firstTiming.sampleId !== sampleId || after.firstTiming.actualFrameGeneration !== after.firstTiming.expectedFrameGeneration ||
      after.firstTiming.expectedFrameGeneration !== before.generation + 1) fail(`FAILED: renderer generation attribution for ${operation}`);

  const f = fixture.fixture;
  switch (operation) {
    case "sibling-reorder":
      if (JSON.stringify(after.state.shapes[f.rootA].children.slice(0, 2)) !== JSON.stringify([f.branchB, f.branchA])) fail("FAILED: sibling order assertion");
      if (JSON.stringify(before.pixels.overlap) === JSON.stringify(after.pixels.overlap)) fail("FAILED: sibling reorder pixels unchanged");
      break;
    case "root-reorder":
      if (JSON.stringify(after.state.page.rootShapeIds) !== JSON.stringify([f.rootB, f.rootA])) fail("FAILED: root order assertion");
      if (JSON.stringify(before.pixels.overlap) === JSON.stringify(after.pixels.overlap)) fail("FAILED: root reorder pixels unchanged");
      break;
    case "create": assertPixelEqual(after.pixels.create, [0, 255, 0, 255], "create"); break;
    case "insert-child": {
      const inserted = Object.values(after.state.shapes).find((shape) => shape.name.startsWith("inserted-"));
      if (!inserted || inserted.parentId !== f.rootA || !after.state.shapes[f.rootA].children.includes(inserted.id)) fail("FAILED: child insertion membership");
      assertPixelEqual(after.pixels.insert, [255, 0, 255, 255], "insert-child");
      break;
    }
    case "rename":
      if (!after.state.shapes[f.redLeaf]?.name.startsWith("renamed-")) fail("FAILED: rename state");
      assertPixelEqual(after.pixels.overlap, before.pixels.overlap, "rename");
      break;
    case "reparent":
      if (after.state.shapes[f.redLeaf]?.parentId !== f.branchB ||
          JSON.stringify(after.state.shapes[f.redLeaf]?.bounds) !== JSON.stringify(before.state.shapes[f.redLeaf]?.bounds)) fail("FAILED: reparent identity/geometry");
      break;
    case "subtree-delete":
      if (after.state.shapes[f.branchA] || !after.state.shapes[f.redLeaf] || after.state.shapes[f.redLeaf].parentId !== f.branchB)
        fail("FAILED: subtree deletion or reparented identity");
      if (after.selection.includes(f.branchA)) fail("FAILED: selection retained deleted shape");
      break;
  }
  for (const rgba of Object.values(after.pixels)) {
    if (!Array.isArray(rgba) || rgba.length !== 4 || rgba.some((channel) => !Number.isInteger(channel) || channel < 0 || channel > 255))
      fail(`FAILED: invalid observed framebuffer pixel after ${operation}`);
  }
  return {
    operation, sampleId, expectedFrameGeneration: after.firstTiming.expectedFrameGeneration,
    stateBeforeSha256: stateHash(before.state), stateAfterSha256: stateHash(after.state),
    rustSemanticEquivalent: true, selectedShapeIds: after.selection,
    observedPixels: { before: before.pixels, after: after.pixels },
  };
}

async function runCycle(page, { round, size, cycle, fixture, fixtureSerial, sourceLabel }) {
  const steps = [];
  for (let i = 0; i < MEMORY_OPERATION_SEQUENCE.length; i += 1) {
    const operation = MEMORY_OPERATION_SEQUENCE[i];
    steps.push(await executeOperation(page, { round, size, cycle, operation, operationIndex: i, fixture }));
  }
  const currentGeneration = await page.locator("[data-renderer-status]").getAttribute("data-renderer-frame-count");
  await page.evaluate(([workloadSize, serial, sampleId]) => window.__g1b95.prepare(workloadSize, "view-control", serial, sampleId),
    [size, fixtureSerial, `${sourceLabel}-reset`]);
  await waitForCanvas(page, currentGeneration);
  const reset = await page.evaluate(() => {
    const state = window.__g1b95.state();
    return { state, selection: window.__g1b95.selection(),
      pixels: { overlap: window.__g1b95.samplePixel(40, 40), create: window.__g1b95.samplePixel(430, 45), insert: window.__g1b95.samplePixel(130, 130) },
      status: document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status"),
      backend: document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-backend"),
      errors: window.__g1b95.errors() };
  });
  if (stateHash(reset.state) !== fixture.stateDigest || reset.state.documentId !== null || reset.status !== "READY" || reset.backend !== "wasm" || reset.errors.length)
    fail(`FAILED: fixture reset did not restore the verified ${size}-shape baseline`);
  assertPixelEqual(reset.pixels.overlap, fixture.pixels.overlap, "reset overlap");
  assertPixelEqual(reset.pixels.create, fixture.pixels.create, "reset create");
  assertPixelEqual(reset.pixels.insert, fixture.pixels.insert, "reset insert");
  return { operations: steps, resetStateSha256: stateHash(reset.state), selection: reset.selection, resetPixels: reset.pixels };
}

async function captureCheckpoint({ session, runDirectory, runId, browserSessionId, fixtureHash, checkpoint }) {
  const artifactId = `${checkpoint}.heapsnapshot`;
  const file = `snapshots/${artifactId}`;
  const filePath = path.join(runDirectory, file);
  const record = await captureChromiumHeapSnapshot({ session, filePath, artifactId, runId, browserSessionId,
    fixtureHash, checkpoint, authorizedIsolatedRun: true }); // owner record was validated before launching Chromium
  const details = await stat(filePath);
  if (!details.isFile() || details.size !== record.bytes || record.sha256.length !== 64 || await sha256File(filePath) !== record.sha256)
    fail("BLOCKED: raw heap snapshot integrity verification failed");
  const checkpointRecord = { ...record, file };
  report.artifacts.push({ file, sha256: record.sha256, bytes: record.bytes, status: record.status,
    runId: record.runId, browserSessionId: record.browserSessionId, fixtureHash: record.fixtureHash, checkpoint: record.checkpoint });
  return checkpointRecord;
}

async function launchBrowserRound({ round, previewUrl, runDirectory }) {
  const executablePath = process.env.CHROME_BIN;
  if (!executablePath) fail("BLOCKED: CHROME_BIN must identify the authorized Chromium binary");
  const browser = await chromium.launch({ executablePath, headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-webgl", "--disable-features=WebGPU",
      "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
    const pageErrors = [];
    const wasmResponses = [];
    const responseTasks = [];
    const loadedBuildFiles = new Set();
    page.on("pageerror", (error) => pageErrors.push(error.message));
    page.on("response", (response) => {
      const url = new URL(response.url());
      if (url.origin !== previewUrl.origin) return;
      const assetPath = decodeURIComponent(url.pathname.slice(1));
      const resourceType = response.request().resourceType();
      if (["document", "script", "stylesheet", "worker"].includes(resourceType)) responseTasks.push((async () => {
        const expected = verifiedBuildFiles.get(assetPath);
        if (!expected) fail("BLOCKED: browser loaded a build asset absent from source provenance");
        const body = await response.body();
        if (response.status() !== 200 || body.byteLength !== expected.bytes || sha256(body) !== expected.sha256)
          fail("BLOCKED: browser-executed asset differs from the verified source build");
        loadedBuildFiles.add(assetPath);
      })());
      if (url.pathname === "/js/render-wasm.wasm") responseTasks.push((async () => {
        const body = await response.body();
        wasmResponses.push({ status: response.status(), bytes: body.byteLength, sha256: sha256(body) });
      })());
    });
    await page.goto(previewUrl.href, { waitUntil: "load", timeout: 30000 });
    await page.waitForFunction(() => document.querySelector("#g1b95-status")?.dataset.status === "ready", undefined, { timeout: 90000 });
    await Promise.all(responseTasks);
    if (!loadedBuildFiles.has("g1-b9-5-editor-performance.html") ||
        ![...loadedBuildFiles].some((file) => file.startsWith("assets/") && file.endsWith(".js")))
      fail("BLOCKED: actual browser document and application bundle were not verified against the source build");
    if (pageErrors.length || !wasmResponses.length || wasmResponses.some((item) => item.status !== 200 || item.bytes <= 0))
      fail("BLOCKED: production application or renderer WASM failed to load cleanly");
    const browserMeta = await page.evaluate(() => {
      const canvas = document.querySelector("canvas");
      const gl = canvas?.getContext("webgl2");
      const debug = gl?.getExtension("WEBGL_debug_renderer_info");
      const root = document.querySelector("[data-renderer-status]");
      return { userAgent: navigator.userAgent, chromiumVersion: navigator.userAgent.match(/Chrome\/([0-9.]+)/)?.[1] ?? "unknown",
        viewport: { width: innerWidth, height: innerHeight }, devicePixelRatio,
        rendererStatus: root?.getAttribute("data-renderer-status"), backend: root?.getAttribute("data-renderer-backend"),
        buildSourceIdentity: window.__g1b95.buildSourceIdentity(),
        bootstrap: JSON.parse(document.querySelector("#g1b95-status")?.textContent ?? "{}"),
        webgl: gl ? { version: gl.getParameter(gl.VERSION), vendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
          renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) } : null };
    });
    if (browserMeta.buildSourceIdentity !== sourceProvenance.sourceIdentity ||
        browserMeta.bootstrap.buildSourceIdentity !== sourceProvenance.sourceIdentity)
      fail("BLOCKED: executing browser bundle is not bound to the verified source revision");
    if (browserMeta.rendererStatus !== "READY" || browserMeta.backend !== "wasm" || browserMeta.bootstrap.rendererInstances < 1 || !browserMeta.webgl?.version)
      fail("BLOCKED: actual production Rust/WASM renderer evidence is missing");
    const browserSessionId = randomUUID();
    const cdp = await page.context().newCDPSession(page);
    const browserRun = { status: "CAPTURED_NOT_QUALIFIED", runId, round, browserSessionId,
      browser: { ...browserMeta, platform: process.platform, architecture: process.arch, osRelease: os.release(), node: process.version },
      renderer: { backend: browserMeta.backend, wasmSha256: wasmResponses[0].sha256, wasmBytes: wasmResponses[0].bytes,
        wasmReservedBytes: browserMeta.bootstrap.candidateWasmReservedBytes, wasmResponses }, fixtures: [] };
    for (const size of sizes) {
      const fixtureSerial = 960000 + size;
      const fixture = await setupFixture(page, size, fixtureSerial, `memory-r${round}-s${size}-start`);
      const fixtureRecord = { size, fixtureHash: fixture.fixtureHash, baselineStateSha256: fixture.stateDigest,
        maxDepth: fixture.fixture.idsByLevel.length - 1, levelCounts: fixture.fixture.idsByLevel.map((ids) => ids.length),
        warmupCycles, measuredCycles, checkpoints: [], cycleEvidence: [] };
      for (let warmup = 1; warmup <= warmupCycles; warmup += 1) {
        if (warmup > 1) await setupFixture(page, size, fixtureSerial, `memory-r${round}-s${size}-warmup-reset-${warmup}`);
        await runCycle(page, { round, size, cycle: warmup, fixture, fixtureSerial, sourceLabel: `memory-r${round}-s${size}-warmup-${warmup}` });
      }
      const baselineFixture = await setupFixture(page, size, fixtureSerial, `memory-r${round}-s${size}-baseline`);
      if (baselineFixture.fixtureHash !== fixture.fixtureHash || baselineFixture.stateDigest !== fixture.stateDigest)
        fail("BLOCKED: deterministic fixture identity changed before baseline capture");
      const baseline = await captureCheckpoint({ session: cdp, runDirectory, runId, browserSessionId,
        fixtureHash: fixture.fixtureHash, checkpoint: `round-${round}-size-${size}-baseline-after-warmup` });
      fixtureRecord.checkpoints.push(baseline);
      const deltas = [];
      for (let cycle = 1; cycle <= measuredCycles; cycle += 1) {
        const evidence = await runCycle(page, { round, size, cycle, fixture, fixtureSerial, sourceLabel: `memory-r${round}-s${size}-measured-${cycle}` });
        fixtureRecord.cycleEvidence.push(evidence);
        const checkpoint = await captureCheckpoint({ session: cdp, runDirectory, runId, browserSessionId,
          fixtureHash: fixture.fixtureHash, checkpoint: `round-${round}-size-${size}-after-cycle-${cycle}` });
        const delta = comparePostGcSnapshots(baseline, checkpoint);
        fixtureRecord.checkpoints.push(checkpoint);
        deltas.push(delta.deltaSnapshotSelfSizeBytes);
      }
      fixtureRecord.snapshotSelfSizeDeltaBytes = summarizeMemoryDeltas(deltas);
      browserRun.fixtures.push(fixtureRecord);
    }
    await cdp.detach();
    if (pageErrors.length) fail("FAILED: uncaught browser runtime errors during memory workload");
    return browserRun;
  } finally {
    await browser.close();
  }
}

async function main() {
  if (!previewInput || !artifactsInput || !authorizationInput) fail("Usage: node g1-b9-6-memory-qualification.mjs <loopback-preview-url> <absolute-artifact-directory-outside-repository> <owner-authorization.json>");
  if (!Number.isInteger(warmupCycles) || warmupCycles < 5 || !Number.isInteger(measuredCycles) || measuredCycles < 2)
    fail("BLOCKED: require at least five warmup cycles and two measured cycles");
  sourceProvenance = getCleanSourceProvenance();
  const previewUrl = assertLoopbackPreviewUrl(previewInput);
  const outputRoot = await configurePrivateOutput(artifactsInput);
  runDirectory = path.join(outputRoot, runId);
  await mkdir(runDirectory, { recursive: false, mode: 0o700 });
  report = { schema: "logos.g1-b9-6.memory-evidence/v1", status: "BLOCKED", runId,
    sourceSha: sourceProvenance.commitSha, sourceTreeSha: sourceProvenance.treeSha,
    sourceIdentity: sourceProvenance.sourceIdentity,
    methodology: { fixtureSizes: sizes, operations: MEMORY_OPERATION_SEQUENCE,
      warmupCycles, measuredCycles, browserRounds, gcPolicy: "EXPLICIT_CHROMIUM_GC_BEFORE_SNAPSHOT",
      endpointPolicy: "memory-only; no performance qualification or retained-memory threshold" },
    host: { platform: process.platform, architecture: process.arch, release: os.release(), node: process.version },
    browserRuns: [], artifacts: [], limitations: ["V8 snapshot self-size excludes WASM linear memory, GPU/native memory, and backend RSS.",
      "No product memory budget is authorized; observed deltas are not leak verdicts or qualification passes."] };
  report.authorization = validateMemoryRunAuthorization(JSON.parse(await readFile(authorizationInput, "utf8")));
  report.status = "RUNNING";
  const buildProvenance = await verifyPreviewBuild(previewUrl);
  report.buildProvenance = buildProvenance.summary;
  for (let round = 1; round <= browserRounds; round += 1)
    report.browserRuns.push(await launchBrowserRound({ round, previewUrl, runDirectory }));
  const artifactRefs = report.artifacts;
  await verifyFinalArtifactSet(runDirectory, report.browserRuns, artifactRefs);
  report.artifacts = artifactRefs;
  report.status = "OBSERVED_NOT_QUALIFIED";
  report.completedAt = new Date().toISOString();
  const reportContents = `${JSON.stringify(report, null, 2)}\n`;
  const reportPath = path.join(runDirectory, "memory-report.json");
  await writeFile(reportPath, reportContents, { mode: 0o600, flag: "wx" });
  // Recheck private snapshots immediately before final manifest publication.
  // The failure handler rewrites both outputs as BLOCKED on any mismatch.
  await verifyFinalArtifactSet(runDirectory, report.browserRuns, artifactRefs);
  const publishedReport = await readFile(reportPath, "utf8");
  await writeJson(path.join(runDirectory, "manifest.json"), { schema: "logos.g1-b9-6.memory-manifest/v1", runId,
    status: report.status, sourceSha: sourceProvenance.commitSha, sourceTreeSha: sourceProvenance.treeSha,
    sourceIdentity: sourceProvenance.sourceIdentity, buildProvenance: report.buildProvenance,
    report: "memory-report.json", reportSha256: sha256(publishedReport), artifacts: artifactRefs });
  process.stdout.write(`OBSERVED_NOT_QUALIFIED ${runId}\n`);
}

try {
  await main();
} catch (error) {
  const message = error instanceof Error ? error.message : "BLOCKED: unknown qualification failure";
  if (runDirectory) {
    report ??= { schema: "logos.g1-b9-6.memory-evidence/v1", runId,
      ...(sourceProvenance ? { sourceSha: sourceProvenance.commitSha, sourceTreeSha: sourceProvenance.treeSha,
        sourceIdentity: sourceProvenance.sourceIdentity } : {}) };
    report.status = message.startsWith("FAILED:") ? "FAILED" : "BLOCKED";
    report.failure = message.slice(0, 500);
    report.completedAt = new Date().toISOString();
    await writeFile(path.join(runDirectory, "memory-report.json"), `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600 }).catch(() => {});
    await writeFile(path.join(runDirectory, "manifest.json"), `${JSON.stringify({ schema: "logos.g1-b9-6.memory-manifest/v1",
      runId, ...(sourceProvenance ? { sourceSha: sourceProvenance.commitSha, sourceTreeSha: sourceProvenance.treeSha,
        sourceIdentity: sourceProvenance.sourceIdentity } : {}), status: report.status,
      report: "memory-report.json", artifacts: report.artifacts ?? [] }, null, 2)}\n`, { mode: 0o600 }).catch(() => {});
  }
  process.stderr.write(`${report?.status ?? "BLOCKED"}: ${message}\n`);
  process.exitCode = 1;
}
