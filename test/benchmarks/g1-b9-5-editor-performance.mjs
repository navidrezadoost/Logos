import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";
import { mkdir, writeFile } from "node:fs/promises";
import { chromium } from "playwright";
import { performance } from "node:perf_hooks";

const [url, outputPath, screenshotDirectory] = process.argv.slice(2);
const executablePath = process.env.CHROME_BIN;
if (!url || !outputPath || !screenshotDirectory || !executablePath) {
  throw new Error("Usage: CHROME_BIN=/path/to/chrome node g1-b9-5-editor-performance.mjs <url> <evidence.json> <screenshots-dir>");
}
const allowQuick = process.env.G1_B9_5_ALLOW_QUICK === "1";
const warmups = Number(process.env.G1_B9_5_WARMUPS ?? 5);
const repetitions = Number(process.env.G1_B9_5_REPETITIONS ?? 30);
const rounds = Number(process.env.G1_B9_5_ROUNDS ?? 2);
if (!allowQuick && (warmups < 5 || repetitions < 30 || rounds < 2)) {
  throw new Error("Qualification requires >=5 warm-ups, >=30 repetitions, and >=2 rounds");
}

const sizes = [21, 85, 341];
const operations = ["create", "insert-child", "rename", "sibling-reorder", "root-reorder", "subtree-delete", "reparent", "view-control"];
const evidence = {
  schema: "logos.g1-b9-5.editor-performance/v2",
  status: "running",
  methodology: { warmups, repetitions, rounds, fixtureSizes: sizes, operations,
    clickPath: "browser click event timestamp -> test control handler -> production Zustand operation or view-only control -> mounted production Canvas/Layers/Inspector -> browser-local matching frame publication",
    endpoint: "immutable browser-local timestamp captured in the second RAF after the matching successful renderer generation; no Playwright retrieval, readback, serialization, or assertion is in the interval" },
  environment: {
    os: { platform: process.platform, release: os.release(), architecture: process.arch },
    cpu: { model: os.cpus()[0]?.model ?? "unknown", logicalCores: os.cpus().length },
    node: process.version,
  },
  browserRuns: [],
  rawSamples: [],
  summary: [],
  functionalChecks: [],
  failures: [],
};

await mkdir(path.dirname(outputPath), { recursive: true });
await mkdir(screenshotDirectory, { recursive: true });

async function launchRun(round) {
  const browser = await chromium.launch({
    executablePath,
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-webgl", "--disable-features=WebGPU",
      "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
  });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
    const pageErrors = [];
    const rendererWasmResponses = [];
    const responseTasks = [];
    page.on("pageerror", (error) => pageErrors.push(error.message));
    page.on("response", (response) => {
      if (new URL(response.url()).pathname === "/js/render-wasm.wasm") {
        responseTasks.push((async () => {
          const body = await response.body();
          rendererWasmResponses.push({ status: response.status(), bytes: body.byteLength,
            sha256: createHash("sha256").update(body).digest("hex") });
        })());
      }
    });
    await page.goto(url, { waitUntil: "load", timeout: 30000 });
    await page.waitForFunction(() => document.querySelector("#g1b95-status")?.dataset.status === "ready", undefined, { timeout: 90000 });
    await Promise.all(responseTasks);
    assert(rendererWasmResponses.length > 0 && rendererWasmResponses.every((response) => response.status === 200 && response.bytes > 0),
      "production renderer WASM asset was not successfully fetched");
    const browserMeta = await page.evaluate(() => {
      const canvas = document.querySelector("canvas");
      const gl = canvas?.getContext("webgl2");
      const debug = gl?.getExtension("WEBGL_debug_renderer_info");
      return {
        bootstrap: JSON.parse(document.querySelector("#g1b95-status")?.textContent ?? "{}"),
        userAgent: navigator.userAgent,
        chromiumVersion: navigator.userAgent.match(/Chrome\/([0-9.]+)/)?.[1] ?? "unknown",
        viewport: { width: innerWidth, height: innerHeight },
        devicePixelRatio,
        rendererBackend: document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-backend"),
        webgl: gl ? { version: gl.getParameter(gl.VERSION), shadingLanguageVersion: gl.getParameter(gl.SHADING_LANGUAGE_VERSION),
          vendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
          renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) } : null,
      };
    });
    assert.equal(browserMeta.rendererBackend, "wasm", "measurement requires the actual Rust/WASM production Canvas renderer");
    assert(browserMeta.bootstrap.rendererInstances > 0, "instrumented production renderer loader did not instantiate WASM");
    assert(browserMeta.webgl?.version, "WebGL2 unavailable");
    evidence.browserRuns.push({ round, ...browserMeta, loadedRendererWasm: rendererWasmResponses });

    for (const size of sizes) {
      for (const operation of operations) {
        for (let index = 0; index < warmups + repetitions; index += 1) {
          const serial = round * 100000 + size * 100 + operations.indexOf(operation) * 40 + index + 1;
          const setupFrameCount = await page.locator("[data-renderer-status]").getAttribute("data-renderer-frame-count");
          const sampleId = `r${round}-s${size}-${operation}-i${index}`;
          await page.evaluate(([workloadSize, operationKind, workloadSerial, id]) =>
            window.__g1b95.prepare(workloadSize, operationKind, workloadSerial, id), [size, operation, serial, sampleId]);
          await page.waitForFunction((oldFrame) => {
            const root = document.querySelector("[data-renderer-status]");
            return root?.getAttribute("data-renderer-status") === "READY" &&
              Number(root.getAttribute("data-renderer-frame-count")) > Number(oldFrame) && Boolean(root.getAttribute("data-renderer-metrics"));
          }, setupFrameCount, { timeout: 30000 });

          const before = await page.evaluate(() => {
            const root = document.querySelector("[data-renderer-status]");
            const state = window.__g1b95.state();
            return {
              frameCount: Number(root?.getAttribute("data-renderer-frame-count")),
              metrics: root?.getAttribute("data-renderer-metrics") ?? "",
              shapeCount: Object.keys(state.shapes).length,
              pixelOverlap: window.__g1b95.samplePixel(40, 40),
              pixelCreate: window.__g1b95.samplePixel(430, 45),
              pixelInsert: window.__g1b95.samplePixel(130, 130),
              fixture: window.__g1b95.fixture(),
              state: window.__g1b95.state(),
              selection: window.__g1b95.selection(),
            };
          });
          assert.equal(before.shapeCount, size, `fixture shape count ${before.shapeCount} != ${size}`);

          const clickStartedAt = performance.now();
          await page.locator(`[data-g1b95-operation="${operation}"]`).click();
          const clickWallMs = performance.now() - clickStartedAt;
          const waitStartedAt = performance.now();
          const frameHandle = await page.waitForFunction((id) => {
            const timing = window.__g1b95.timing(id);
            return timing?.framePublishedAt !== undefined ? { timing, observedAt: performance.now() } : false;
          }, sampleId, { timeout: 30000 });
          const waitForFrameWallMs = performance.now() - waitStartedAt;
          const frameTransferStartedAt = performance.now();
          const frameObservation = await frameHandle.jsonValue();
          const frameObservationTransferWallMs = performance.now() - frameTransferStartedAt;
          await frameHandle.dispose();
          const postFrameExtractionStartedAt = performance.now();
          const after = await page.evaluate(() => {
            const root = document.querySelector("[data-renderer-status]");
            const serialized = window.__g1b95.serializedState();
            return {
              frameCount: Number(root?.getAttribute("data-renderer-frame-count")),
              rendererStatus: root?.getAttribute("data-renderer-status"),
              metrics: JSON.parse(root?.getAttribute("data-renderer-metrics") ?? "{}"),
              pixelOverlap: window.__g1b95.samplePixelMeasured(40, 40),
              pixelCreate: window.__g1b95.samplePixelMeasured(430, 45),
              pixelInsert: window.__g1b95.samplePixelMeasured(130, 130),
              result: window.__g1b95.lastResult(),
              timing: window.__g1b95.timing(window.__g1b95.lastResult()?.sampleId ?? ""),
              fixture: window.__g1b95.fixture(),
              state: window.__g1b95.state(),
              selection: window.__g1b95.selection(),
              serializedState: serialized,
              errors: window.__g1b95.errors(),
              jsHeapUsedBytes: performance.memory?.usedJSHeapSize ?? null,
            };
          });
          const postFrameExtractionWallMs = performance.now() - postFrameExtractionStartedAt;
          const diagnosticConfirmationStartedAt = performance.now();
          const immutableTiming = await page.evaluate((id) => ({
            initial: window.__g1b95.timing(id),
            extraQueries: [window.__g1b95.lastResult(), window.__g1b95.state().currentPageId, window.__g1b95.fixture().shapeCount],
            final: window.__g1b95.timing(id),
          }), sampleId);
          const diagnosticConfirmationWallMs = performance.now() - diagnosticConfirmationStartedAt;
          assert.deepEqual(immutableTiming.final, immutableTiming.initial, "post-frame diagnostic reads changed the browser-local timing record");
          const pixelReadback = [after.pixelOverlap, after.pixelCreate, after.pixelInsert];
          const plainPixels = pixelReadback.map((pixel) => pixel.rgba);
          const assertionsStartedAt = performance.now();
          assert(after.result?.ok, `${operation} rejected: ${after.result?.diagnostic ?? "no result"}`);
          assert.equal(after.rendererStatus, "READY", `${operation} renderer status was not READY at observation`);
          assert(Object.keys(after.metrics).length > 0, `${operation} renderer metrics were unavailable at observation`);
          assert(after.frameCount > before.frameCount, `${operation} did not render a later frame`);
          assert.equal(await page.locator("[data-renderer-status]").getAttribute("data-renderer-backend"), "wasm");
          assert.deepEqual(after.errors, [], `browser runtime errors: ${after.errors.join(" | ")}`);
          if (operation !== "view-control") assert(after.result.candidate?.semanticEquivalent, `${operation} Rust candidate was not equivalent`);

          const f = before.fixture;
          const state = after.state;
          switch (operation) {
            case "create":
              assert.equal(Object.keys(state.shapes).length, size + 1);
              assert.deepEqual(after.pixelCreate.rgba, [0, 255, 0, 255]);
              break;
            case "insert-child": {
              const inserted = Object.values(state.shapes).find((shape) => shape.name.startsWith("inserted-"));
              assert(inserted && inserted.parentId === f.rootA && state.shapes[f.rootA].children.includes(inserted.id));
              assert.deepEqual(after.pixelInsert.rgba, [255, 0, 255, 255]);
              break;
            }
            case "rename":
              assert(state.shapes[f.redLeaf].name.startsWith("renamed-"));
              assert.deepEqual(after.pixelOverlap.rgba, before.pixelOverlap);
              break;
            case "sibling-reorder":
              assert.deepEqual(state.shapes[f.rootA].children.slice(0, 2), [f.branchB, f.branchA]);
              assert.notDeepEqual(after.pixelOverlap.rgba, before.pixelOverlap, "sibling reorder did not change overlap pixels");
              break;
            case "root-reorder":
              assert.deepEqual(state.page.rootShapeIds, [f.rootB, f.rootA]);
              assert.notDeepEqual(after.pixelOverlap.rgba, before.pixelOverlap, "root reorder did not change overlap pixels");
              break;
            case "subtree-delete":
              assert(!state.shapes[f.branchA] && !state.shapes[f.redLeaf], "subtree delete left descendant records");
              assert(!after.result.selectionIds.includes(f.redLeaf), "selection still includes a removed descendant");
              break;
            case "reparent":
              assert.equal(state.shapes[f.redLeaf].parentId, f.branchB);
              assert.deepEqual(state.shapes[f.redLeaf].bounds, before.state.shapes[f.redLeaf].bounds, "reparent changed page-space geometry");
              break;
            case "view-control":
              assert.deepEqual(state, before.state, "view-only causal control mutated document state");
              assert.deepEqual(after.selection, before.selection, "view-only causal control mutated selection state");
              assert.deepEqual(plainPixels[0], before.pixelOverlap, "view-only pan changed overlap pixel content");
              break;
          }
          evidence.functionalChecks.push({ round, size, operation, frameCountAdvanced: true, pixels: {
            overlapBefore: before.pixelOverlap, overlapAfter: after.pixelOverlap.rgba,
            create: operation === "create" ? after.pixelCreate.rgba : undefined,
            childInsert: operation === "insert-child" ? after.pixelInsert.rgba : undefined,
          } });
          const functionalAssertionWallMs = performance.now() - assertionsStartedAt;

          if (index >= warmups) {
            const result = after.result;
            const browserTiming = frameObservation.timing;
            assert.equal(browserTiming.sampleId, sampleId);
            assert.equal(browserTiming.actualFrameGeneration, browserTiming.expectedFrameGeneration,
              `${operation} frame generation did not match its sample`);
            const frameMetrics = after.metrics;
            evidence.rawSamples.push({
              round, size, shapeCount: size, operation,
              browserLocalTiming: browserTiming,
              harnessOverhead: {
                playwrightClickWallMs: clickWallMs, waitForFramePollingWallMs: waitForFrameWallMs,
                frameObservationTransferWallMs,
                frameStatusObservationDelayMs: frameObservation.observedAt - browserTiming.framePublishedAt,
                postFrameStateSerializationMs: after.serializedState.serializationMs,
                postFrameExtractionAndBrowserToNodeTransferWallMs: postFrameExtractionWallMs,
                postFrameDiagnosticQueriesWallMs: diagnosticConfirmationWallMs,
                functionalAssertionWallMs,
                pixelWebglFinishTotalMs: pixelReadback.reduce((sum, pixel) => sum + pixel.finishMs, 0),
                pixelReadPixelsTotalMs: pixelReadback.reduce((sum, pixel) => sum + pixel.readPixelsMs, 0),
              },
              userInteractionHandling: { eventDispatchDelayMs: result.eventDispatchDelayMs,
                storeActionWallMs: result.storeActionMs,
                uiSelectionUpdateMs: result.uiSelectionUpdateMs,
                handlerStartedAt: result.handlerStartedAt, handlerCompletedAt: result.handlerCompletedAt },
              reactOwnedStoreStages: result.stages,
              renderer: {
                sceneProjectionValidationMs: frameMetrics.projectionValidationMs,
                wasmSceneUploadMs: frameMetrics.wasmUploadMs,
                synchronousNativeRendererMs: frameMetrics.nativeRendererMs,
                totalSynchronousScenePathMs: frameMetrics.synchronousMainThreadMs,
                browserSchedulingToRafMs: frameMetrics.browserFrameLatencyMs,
                rendererWasmReservedBytes: frameMetrics.wasmReservedBytes,
                observedJsHeapBytes: after.jsHeapUsedBytes,
              },
              rustCandidatePath: result.candidate,
            });
            if (round === 1 && index === warmups && operation !== "view-control") {
              const screenshot = path.join(screenshotDirectory, `g1-b9-5-${size}-${operation}.png`);
              const screenshotStartedAt = performance.now();
              await page.screenshot({ path: screenshot });
              evidence.rawSamples.at(-1).harnessOverhead.screenshotCaptureWallMs = performance.now() - screenshotStartedAt;
            }
          }
        }
      }
    }
    assert.deepEqual(pageErrors, [], `uncaught page errors: ${pageErrors.join(" | ")}`);
  } finally {
    await browser.close();
  }
}

try {
  for (let round = 1; round <= rounds; round += 1) await launchRun(round);
  const metricValues = new Map();
  for (const sample of evidence.rawSamples) {
    const add = (name, value) => {
      if (typeof value !== "number" || !Number.isFinite(value)) return;
      const key = `${sample.size}/${sample.operation}/${name}`;
      const values = metricValues.get(key) ?? [];
      values.push(value);
      metricValues.set(key, values);
    };
    add("interactionToFrameMs", sample.browserLocalTiming.interactionToFrameMs);
    for (const name of ["eventDispatchDelayMs", "storeActionWallMs", "uiSelectionUpdateMs"]) add(name, sample.userInteractionHandling[name]);
    for (const stage of sample.reactOwnedStoreStages) add(stage.stage, stage.milliseconds);
    for (const [name, value] of Object.entries(sample.renderer)) add(name, value);
    if (sample.rustCandidatePath) {
      for (const [name, value] of Object.entries(sample.rustCandidatePath.timingsMs)) add(`rustCandidate.${name}`, value);
      add("rustCandidateWasmReservedBytes", sample.rustCandidatePath.wasmReservedBytes);
    }
    for (const [name, value] of Object.entries(sample.harnessOverhead)) add(`harness.${name}`, value);
  }
  for (const [key, values] of metricValues) {
    const sorted = [...values].sort((a, b) => a - b);
    evidence.summary.push({ workload: key.split("/")[0], operation: key.split("/")[1], metric: key.split("/").slice(2).join("/"),
      sampleCount: values.length, p50: sorted[Math.floor((sorted.length - 1) * 0.5)],
      p95: sorted[Math.ceil(sorted.length * 0.95) - 1], maximum: sorted[sorted.length - 1], mean: values.reduce((sum, value) => sum + value, 0) / values.length });
  }
  evidence.status = "passed";
} catch (error) {
  evidence.status = "failed";
  evidence.failures.push(error instanceof Error ? error.stack ?? error.message : String(error));
  await writeFile(outputPath, JSON.stringify(evidence, null, 2));
  throw error;
} finally {
  await writeFile(outputPath, JSON.stringify(evidence, null, 2));
}
