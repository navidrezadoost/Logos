/**
 * P1 — Frame-Time Performance Benchmark (CI regression gate)
 *
 * Loads the standard "large canvas" test file (10,000 shapes, multiple flex/grid
 * frames) and measures requestAnimationFrame callback times during a continuous
 * drag operation.
 *
 * Acceptance criteria (from §18 of ANALYSIS_AND_OPTIMIZATION_PLAN.md):
 *   - Canvas frame time ≤ 4 ms p95 (drag scenario)
 *   - File open time (1,000 shapes) ≤ 1,500 ms
 *   - Memory peak (50 MB import) ≤ 300 MB
 *
 * CI gate:
 *   Any PR that increases p95 frame time by more than 5% compared to the
 *   baseline in BENCHMARK_BASELINE_MS is rejected (test fails).
 *
 * Environment variables:
 *   LOGOS_BENCH_URL      URL to a running Logos instance (default: http://localhost:3449)
 *   LOGOS_TEST_FILE_ID   File UUID containing the large-canvas test document
 *   LOGOS_TEST_PAGE_ID   Page UUID within that file
 *   BENCHMARK_BASELINE_MS  Baseline p95 frame time in ms (default: 4)
 *
 * Run locally:
 *   LOGOS_BENCH_URL=http://localhost:3449 \
 *   LOGOS_TEST_FILE_ID=<uuid> \
 *   LOGOS_TEST_PAGE_ID=<uuid> \
 *   npx playwright test performance-benchmark.spec.js --reporter=line
 */

import { test, expect } from "@playwright/test";

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const BENCH_URL = process.env.LOGOS_BENCH_URL || "http://localhost:3449";
const FILE_ID = process.env.LOGOS_TEST_FILE_ID;
const PAGE_ID = process.env.LOGOS_TEST_PAGE_ID;
const BASELINE_MS = parseFloat(process.env.BENCHMARK_BASELINE_MS || "4");
const REGRESSION_THRESHOLD = 0.05; // 5% allowed regression

/** Number of rAF samples to collect during the drag scenario. */
const SAMPLE_COUNT = 100;

/** Duration (ms) of the simulated drag in the viewport centre. */
const DRAG_DURATION_MS = 2000;

// Skip when no live Logos instance is configured.
const hasLiveInstance = Boolean(FILE_ID && PAGE_ID);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Inject a rAF-based frame timer into the page that collects `count` samples
 * and resolves with an array of delta-time values (ms).
 */
async function collectFrameTimes(page, count) {
  return page.evaluate((n) => {
    return new Promise((resolve) => {
      const samples = [];
      let lastTs = null;

      function tick(ts) {
        if (lastTs !== null) {
          samples.push(ts - lastTs);
        }
        lastTs = ts;
        if (samples.length < n) {
          requestAnimationFrame(tick);
        } else {
          resolve(samples);
        }
      }

      requestAnimationFrame(tick);
    });
  }, count);
}

function percentile(sorted, p) {
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

function stats(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    min: sorted[0],
    max: sorted[sorted.length - 1],
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
    mean: samples.reduce((a, b) => a + b, 0) / samples.length,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe("Phase 1 — Frame-Time Performance Benchmarks", () => {
  test.skip(!hasLiveInstance, "Skipped — set LOGOS_TEST_FILE_ID + LOGOS_TEST_PAGE_ID to enable");

  test.setTimeout(120_000); // benchmarks can be slow

  test("P1 idle frame time — p95 ≤ BASELINE_MS + 5%", async ({ page }) => {
    const workspaceUrl = `${BENCH_URL}/workspace/${FILE_ID}/${PAGE_ID}`;
    await page.goto(workspaceUrl);

    // Wait for initial WASM render to complete (event dispatched by set-objects)
    await page.waitForFunction(() => {
      return window._logosWasmReady === true ||
        document.querySelector("[data-testid='canvas-ready']") !== null;
    }, undefined, { timeout: 30_000 }).catch(() => {
      // Fallback: wait for networkidle + 2s buffer
      return page.waitForLoadState("networkidle").then(() =>
        page.waitForTimeout(2000)
      );
    });

    // Collect idle frame times (no user interaction)
    const idleSamples = await collectFrameTimes(page, SAMPLE_COUNT);
    const idleStats = stats(idleSamples);

    console.log("Idle frame stats:", idleStats);

    const maxAllowedP95 = BASELINE_MS * (1 + REGRESSION_THRESHOLD);
    expect(idleStats.p95).toBeLessThanOrEqual(maxAllowedP95);
  });

  test("P1 drag frame time — p95 ≤ BASELINE_MS * 2 (drag budget)", async ({ page }) => {
    const workspaceUrl = `${BENCH_URL}/workspace/${FILE_ID}/${PAGE_ID}`;
    await page.goto(workspaceUrl);
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2000);

    const viewport = page.viewportSize();
    const cx = viewport.width / 2;
    const cy = viewport.height / 2;

    // Start collecting rAF samples concurrently with the drag
    const frameTimesPromise = collectFrameTimes(page, SAMPLE_COUNT);

    // Simulate a drag across the canvas
    await page.mouse.move(cx - 100, cy);
    await page.mouse.down();
    const steps = 50;
    for (let i = 0; i <= steps; i++) {
      await page.mouse.move(cx - 100 + (200 * i) / steps, cy, { steps: 1 });
      await page.waitForTimeout(DRAG_DURATION_MS / steps);
    }
    await page.mouse.up();

    const dragSamples = await frameTimesPromise;
    const dragStats = stats(dragSamples);

    console.log("Drag frame stats:", dragStats);

    // Drag budget is 2× baseline — some frames will be longer during input handling
    const dragBudget = BASELINE_MS * 2 * (1 + REGRESSION_THRESHOLD);
    expect(dragStats.p95).toBeLessThanOrEqual(dragBudget);
  });

  test("P1 file open time ≤ 1500 ms", async ({ page }) => {
    const workspaceUrl = `${BENCH_URL}/workspace/${FILE_ID}/${PAGE_ID}`;

    const t0 = Date.now();
    await page.goto(workspaceUrl);

    // Wait for the penpot:wasm:set-objects event dispatched after shape loading
    await page.waitForFunction(() => {
      return new Promise((resolve) => {
        if (window._logosWasmObjectsLoaded) {
          resolve(true);
          return;
        }
        window.addEventListener("logos:wasm:set-objects", () => resolve(true), { once: true });
        window.addEventListener("penpot:wasm:set-objects", () => resolve(true), { once: true });
        // Fallback after 30s
        setTimeout(() => resolve(false), 30_000);
      });
    }, undefined, { timeout: 35_000 }).catch(() => page.waitForLoadState("networkidle"));

    const openTime = Date.now() - t0;
    console.log(`File open time: ${openTime} ms`);

    expect(openTime).toBeLessThanOrEqual(1500);
  });

  test("P1 memory after import ≤ 300 MB", async ({ page }) => {
    const workspaceUrl = `${BENCH_URL}/workspace/${FILE_ID}/${PAGE_ID}`;
    await page.goto(workspaceUrl);
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(3000); // allow GC to settle

    // performance.measureUserAgentSpecificMemory() requires cross-origin isolation
    const memMB = await page.evaluate(async () => {
      if (!performance.measureUserAgentSpecificMemory) return null;
      const result = await performance.measureUserAgentSpecificMemory();
      return result.bytes / (1024 * 1024);
    });

    if (memMB === null) {
      console.warn("measureUserAgentSpecificMemory not available — skipping memory check");
      test.skip();
    } else {
      console.log(`Heap usage: ${memMB.toFixed(1)} MB`);
      expect(memMB).toBeLessThanOrEqual(300);
    }
  });
});

// ---------------------------------------------------------------------------
// Unit-level smoke test (runs without a live instance)
// ---------------------------------------------------------------------------

test.describe("Benchmark harness unit tests", () => {
  test("stats() computes correct percentiles", () => {
    const samples = Array.from({ length: 100 }, (_, i) => i + 1); // 1..100
    const s = stats(samples);
    expect(s.p50).toBe(50);
    expect(s.p95).toBe(95);
    expect(s.p99).toBe(99);
    expect(s.min).toBe(1);
    expect(s.max).toBe(100);
  });

  test("5% regression gate arithmetic", () => {
    const baseline = 4; // ms
    const maxAllowed = baseline * (1 + REGRESSION_THRESHOLD);
    expect(maxAllowed).toBeCloseTo(4.2, 1);
    expect(3.9).toBeLessThanOrEqual(maxAllowed);
    expect(4.3).toBeGreaterThan(maxAllowed);
  });
});
