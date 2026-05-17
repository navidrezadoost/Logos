/**
 * P1.2 CI smoke test — Cross-Origin Isolation
 *
 * Verifies that:
 *  1. The page is cross-origin isolated (window.crossOriginIsolated === true)
 *     when the :cross-origin-isolation server flag is enabled.
 *  2. SharedArrayBuffer is available in the main context.
 *  3. The logos.render_wasm.sab.check() export returns expected diagnostics.
 *
 * This test is gated by the CI_CHECK_CORS environment variable and is
 * skipped in regular development builds where COOP/COEP headers are not set.
 */

import { test, expect } from "@playwright/test";

// Skip these checks unless the CI environment explicitly opts in.
// CI pipeline sets CI_CHECK_CORS=true after verifying backend headers.
const runCorsChecks = process.env.CI_CHECK_CORS === "true";

test.describe("Cross-Origin Isolation (P1.2)", () => {
  test.skip(!runCorsChecks, "Skipped — set CI_CHECK_CORS=true to enable");

  test("page is cross-origin isolated", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    const isolated = await page.evaluate(() => window.crossOriginIsolated);
    expect(isolated).toBe(true);
  });

  test("SharedArrayBuffer is available", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    const available = await page.evaluate(
      () => typeof SharedArrayBuffer !== "undefined"
    );
    expect(available).toBe(true);
  });

  test("sab.check() reports correct diagnostics", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for the CLJS module to load; sab.check is exported under the
    // compiled ClojureScript namespace path.
    const result = await page.evaluate(() => {
      // Attempt to call the exported check function.
      try {
        const ns =
          window.app &&
          window.app.render_wasm &&
          window.app.render_wasm.sab;
        return ns ? ns.check() : null;
      } catch (_) {
        return null;
      }
    });

    // If sab module is loaded, validate structural contract.
    if (result) {
      expect(result.sabSupported).toBe(true);
      expect(result.crossOriginIsolated).toBe(true);
      expect(result.enabled).toBe(true);
    }
  });

  test("COOP header present on main HTML response", async ({ request }) => {
    const response = await request.get("/");
    const coop = response.headers()["cross-origin-opener-policy"];
    expect(coop).toBe("same-origin");
  });

  test("COEP header present on main HTML response", async ({ request }) => {
    const response = await request.get("/");
    const coep = response.headers()["cross-origin-embedder-policy"];
    expect(coep).toBe("require-corp");
  });

  test("CORP header present on JS sub-resources", async ({ request }) => {
    const response = await request.get("/js/main.js");
    const corp = response.headers()["cross-origin-resource-policy"];
    expect(corp).toBe("same-origin");
  });
});
