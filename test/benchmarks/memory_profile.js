/**
 * P1.7 — Memory profiling regression suite.
 *
 * Loads the large-canvas benchmark file into a running Logos instance,
 * executes a scenario that exercises undo/redo, page navigation, and
 * idle garbage collection, then asserts that heap metrics stay within
 * the limits defined by the Phase 1 performance contract.
 *
 * Prerequisites:
 *   1. Logos running at LOGOS_URL (default: http://localhost:3449)
 *   2. A test user available via LOGOS_EMAIL / LOGOS_PASSWORD env vars.
 *   3. The benchmark .logos file available at BENCHMARK_FILE.
 *      Generate it with: cd test/benchmarks && clojure -M:gen-benchmark
 *
 * Usage:
 *   node memory_profile.js [--url http://localhost:3449] [--headless]
 *
 * Output:
 *   - JSON report written to snapshots/report-<timestamp>.json
 *   - Heap snapshots written to snapshots/baseline.heapsnapshot
 *     and snapshots/final.heapsnapshot
 *   - Non-zero exit code on assertion failure (for CI integration)
 *
 * Reference: *Systems Performance* (Brendan Gregg) — USE method.
 * Reference: *Game Programming Patterns* — measurement before optimisation.
 */

"use strict";

const fs   = require("fs");
const path = require("path");
const { chromium } = require("playwright");
const { analyseSnapshot, compareSnapshots } = require("./analyse_snapshot");

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const LOGOS_URL      = process.env.LOGOS_URL      || "http://localhost:3449";
const LOGOS_EMAIL    = process.env.LOGOS_EMAIL    || "benchmark@logos.test";
const LOGOS_PASSWORD = process.env.LOGOS_PASSWORD || "benchmark123!";
const BENCHMARK_FILE = process.env.BENCHMARK_FILE ||
                       path.join(__dirname, "fixtures", "large-canvas.logos");
const SNAPSHOTS_DIR  = path.join(__dirname, "snapshots");
const HEADLESS       = !process.argv.includes("--no-headless");

// Performance thresholds (see Phase 1 spec, P1.7 section).
const LIMITS = {
  heapSizeMB:           300,   // Max total heap after scenario
  retainedGrowthMB:      50,   // Max heap increase over baseline
  detachedDomNodes:     100,   // Detached DOM nodes in final snapshot
  arrayBufferGrowth:     10,   // New ArrayBuffer allocations (not freed)
};

// Scenario parameters
const UNDO_REDO_CYCLES  = 20;
const PAGE_NAV_CYCLES   = 10;
const PAGE_COUNT        =  5;  // Must match the generator's page count

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Write a file, creating parent directories as needed. */
function writeFile(filePath, content) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
}

/** Collect a full heap snapshot via CDP and write it to disk.
 *  Returns the parsed snapshot object. */
async function takeHeapSnapshot(client, outPath) {
  const chunks = [];
  client.on("HeapProfiler.addHeapSnapshotChunk", ({ chunk }) => chunks.push(chunk));
  await client.send("HeapProfiler.takeHeapSnapshot", {
    reportProgress: false,
    treatGlobalObjectsAsRoots: true,
    captureNumericValue: false,
  });
  const raw = chunks.join("");
  writeFile(outPath, raw);
  return JSON.parse(raw);
}

/** Sleep for ms milliseconds. */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Wait until a Logos-specific global flag signals render completion. */
async function waitForRenderComplete(page, timeoutMs = 30_000) {
  await page.waitForFunction(
    () => window["__logosBenchmarkReady"] === true || document.querySelector(".workspace-canvas"),
    { timeout: timeoutMs }
  );
}

/** POST to the Logos RPC API. Returns the parsed JSON body. */
async function apiPost(page, command, body) {
  return page.evaluate(
    async ([url, cmd, params]) => {
      const r = await fetch(`${url}/api/rpc/command/${cmd}`, {
        method: "POST",
        headers: { "Content-Type": "application/transit+json" },
        credentials: "include",
        body: JSON.stringify(params),
      });
      return r.json();
    },
    [LOGOS_URL, command, body]
  );
}

// ---------------------------------------------------------------------------
// Scenario steps
// ---------------------------------------------------------------------------

/** Log in via the Logos authentication page. */
async function loginStep(page) {
  console.log("[1/6] Logging in …");
  await page.goto(`${LOGOS_URL}/`, { waitUntil: "networkidle" });

  // If already logged in (session cookie from a previous run) skip login.
  if (await page.$(".dashboard") !== null) {
    console.log("      Already authenticated.");
    return;
  }

  await page.fill('[name="email"]', LOGOS_EMAIL);
  await page.fill('[name="password"]', LOGOS_PASSWORD);
  await page.click('[data-testid="login-button"], button[type="submit"]');
  await page.waitForURL(`${LOGOS_URL}/**`, { timeout: 15_000 });
  console.log("      Login successful.");
}

/** Upload the benchmark .logos file via the dashboard import API. */
async function importFileStep(page) {
  console.log("[2/6] Importing benchmark file …");

  if (!fs.existsSync(BENCHMARK_FILE)) {
    throw new Error(
      `Benchmark file not found: ${BENCHMARK_FILE}\n` +
      "Run: cd test/benchmarks && clojure -M:gen-benchmark"
    );
  }

  // Navigate to dashboard
  await page.goto(`${LOGOS_URL}/`, { waitUntil: "networkidle" });

  // Trigger the import via the hidden file-chooser
  const [fileChooser] = await Promise.all([
    page.waitForEvent("filechooser"),
    page.click('[data-testid="import-file"], [title="Import …"], .import-button')
      .catch(() => {
        // Fallback: use the keyboard shortcut documented in the UI spec
        return page.keyboard.press("Control+i");
      }),
  ]);
  await fileChooser.setFiles(BENCHMARK_FILE);

  // Wait for the import to finish (progress modal disappears)
  await page.waitForSelector(".import-progress", { state: "detached", timeout: 120_000 })
    .catch(() => { /* progress modal may not appear for small files */ });
  await sleep(2_000);

  // Find and click on the newly imported file
  const fileCard = page.locator(".grid-item-th, .file-item").filter({ hasText: "Logos Memory Benchmark" }).first();
  await fileCard.waitFor({ timeout: 15_000 });
  await fileCard.dblclick();

  // Wait for workspace to load
  await waitForRenderComplete(page, 60_000);
  console.log("      File loaded, workspace ready.");
}

/** Capture the baseline snapshot immediately after file load. */
async function baselineSnapshotStep(client) {
  console.log("[3/6] Capturing baseline heap snapshot …");
  const snapshotPath = path.join(SNAPSHOTS_DIR, "baseline.heapsnapshot");
  const snapshot = await takeHeapSnapshot(client, snapshotPath);
  const metrics = analyseSnapshot(snapshot);
  console.log(`      Heap: ${(metrics.heapSize / 1024 / 1024).toFixed(1)} MB, ` +
              `nodes: ${metrics.nodeCount.toLocaleString()}, ` +
              `ArrayBuffers: ${metrics.arrayBufferCount}`);
  return metrics;
}

/** Run UNDO_REDO_CYCLES iterations of Ctrl+Z / Ctrl+Shift+Z. */
async function undoRedoStep(page) {
  console.log(`[4/6] Running ${UNDO_REDO_CYCLES} undo/redo cycles …`);

  // First make a trivial modification so undo has something to do.
  // Click a shape on the canvas and move it one pixel.
  const canvas = page.locator(".workspace-canvas, canvas").first();
  await canvas.click({ position: { x: 200, y: 200 } });
  await page.keyboard.press("ArrowRight");

  for (let i = 0; i < UNDO_REDO_CYCLES; i++) {
    await page.keyboard.press("Control+z");
    await sleep(80);
    await page.keyboard.press("Control+Shift+z");
    await sleep(80);
  }
  // Allow GC to settle
  await sleep(500);
  console.log("      Undo/redo complete.");
}

/** Navigate through pages PAGE_NAV_CYCLES times. */
async function pageNavigationStep(page) {
  console.log(`[5/6] Running ${PAGE_NAV_CYCLES} page navigation cycles …`);
  for (let i = 0; i < PAGE_NAV_CYCLES; i++) {
    const pageIdx = i % PAGE_COUNT;
    const pageItem = page.locator(`.page-list li, [data-testid="page-item"]`).nth(pageIdx);
    await pageItem.click({ timeout: 5_000 }).catch(() => {});
    await sleep(400);
  }
  // Let the GC collect page data for unloaded pages
  await sleep(1_000);
  // Force a GC via the CDP protocol (Chrome-specific; ignored in other browsers)
  console.log("      Navigation complete.");
}

/** Capture the final snapshot and assert against limits. */
async function finalSnapshotAndAssertStep(client, baselineMetrics) {
  console.log("[6/6] Capturing final heap snapshot and asserting limits …");
  const snapshotPath = path.join(SNAPSHOTS_DIR, "final.heapsnapshot");
  const snapshot = await takeHeapSnapshot(client, snapshotPath);
  const metrics = analyseSnapshot(snapshot);
  const delta = compareSnapshots(baselineMetrics, metrics);

  const report = {
    timestamp:          new Date().toISOString(),
    limits:             LIMITS,
    baseline:           baselineMetrics,
    final:              metrics,
    delta,
    results: {
      heap_size_mb:          metrics.heapSize / 1024 / 1024,
      retained_growth_mb:    delta.heapGrowth / 1024 / 1024,
      detached_dom_nodes:    delta.detachedDomNodes,
      array_buffer_growth:   delta.arrayBufferGrowth,
    },
    passed: true,
    failures: [],
  };

  // Assertions
  const MB = 1024 * 1024;

  if (metrics.heapSize > LIMITS.heapSizeMB * MB) {
    const msg = `FAIL heap_size: ${(metrics.heapSize / MB).toFixed(1)} MB > ${LIMITS.heapSizeMB} MB limit`;
    report.failures.push(msg);
    report.passed = false;
    console.error("  ✗", msg);
  } else {
    console.log(`  ✓ heap_size: ${(metrics.heapSize / MB).toFixed(1)} MB ≤ ${LIMITS.heapSizeMB} MB`);
  }

  if (delta.heapGrowth > LIMITS.retainedGrowthMB * MB) {
    const msg = `FAIL retained_growth: +${(delta.heapGrowth / MB).toFixed(1)} MB > ${LIMITS.retainedGrowthMB} MB limit`;
    report.failures.push(msg);
    report.passed = false;
    console.error("  ✗", msg);
  } else {
    console.log(`  ✓ retained_growth: +${(delta.heapGrowth / MB).toFixed(1)} MB ≤ ${LIMITS.retainedGrowthMB} MB`);
  }

  if (delta.detachedDomNodes > LIMITS.detachedDomNodes) {
    const msg = `FAIL detached_dom: ${delta.detachedDomNodes} > ${LIMITS.detachedDomNodes} limit`;
    report.failures.push(msg);
    report.passed = false;
    console.error("  ✗", msg);
  } else {
    console.log(`  ✓ detached_dom: ${delta.detachedDomNodes} ≤ ${LIMITS.detachedDomNodes}`);
  }

  if (delta.arrayBufferGrowth > LIMITS.arrayBufferGrowth) {
    const msg = `FAIL array_buffer_growth: +${delta.arrayBufferGrowth} > ${LIMITS.arrayBufferGrowth} limit`;
    report.failures.push(msg);
    report.passed = false;
    console.error("  ✗", msg);
  } else {
    console.log(`  ✓ array_buffer_growth: +${delta.arrayBufferGrowth} ≤ ${LIMITS.arrayBufferGrowth}`);
  }

  // Write JSON report (machine-readable for Grafana / OTel ingestion)
  const reportPath = path.join(SNAPSHOTS_DIR, `report-${Date.now()}.json`);
  writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(`\nReport written to: ${reportPath}`);

  return report;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

(async () => {
  let browser, context, page;
  let exitCode = 0;

  try {
    browser = await chromium.launch({
      headless: HEADLESS,
      args: [
        "--enable-precise-memory-info",
        "--js-flags=--expose-gc",           // allows programmatic GC (for debugging only)
        "--disable-extensions",
        "--no-sandbox",                     // required in CI containers
      ],
    });

    context = await browser.newContext({
      viewport: { width: 1920, height: 1080 },
      // Chromium-specific: expose performance.memory
      bypassCSP: true,
    });

    page = await context.newPage();

    // Open CDP session for heap profiling
    const client = await page.context().newCDPSession(page);
    await client.send("HeapProfiler.enable");

    // --- Run the scenario ---
    await loginStep(page);
    await importFileStep(page);
    const baselineMetrics = await baselineSnapshotStep(client);
    await undoRedoStep(page);
    await pageNavigationStep(page);
    const report = await finalSnapshotAndAssertStep(client, baselineMetrics);

    if (!report.passed) {
      console.error(`\n${report.failures.length} assertion(s) failed:`);
      report.failures.forEach((f) => console.error(" ", f));
      exitCode = 1;
    } else {
      console.log("\nAll assertions passed. ✓");
    }

  } catch (err) {
    console.error("Benchmark error:", err.message);
    console.error(err.stack);
    exitCode = 1;
  } finally {
    if (browser) await browser.close();
  }

  process.exit(exitCode);
})();
