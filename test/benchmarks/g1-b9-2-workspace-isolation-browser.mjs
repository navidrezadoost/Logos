import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const [url, outputPath] = process.argv.slice(2);
const executablePath = process.env.CHROME_BIN;
if (!url || !outputPath || !executablePath) throw new Error("Usage: CHROME_BIN=/path/to/chrome node g1-b9-2-workspace-isolation-browser.mjs <url> <output>");

const browser = await chromium.launch({ executablePath, headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-webgl", "--disable-features=WebGPU", "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const evidence = { status: "running", checks: [], browserErrors: [] };
let page;
try {
  page = await browser.newPage({ viewport: { width: 1280, height: 740 }, deviceScaleFactor: 1 });
  page.on("pageerror", (error) => evidence.browserErrors.push(error.message));
  await page.goto(url, { waitUntil: "load", timeout: 30000 });
  await page.waitForFunction(() => document.querySelector("#workspace-isolation-status")?.dataset.status === "ready", undefined, { timeout: 30000 });
  await page.waitForFunction(() => window.__g1b92?.isAPending(), undefined, { timeout: 15000 });
  await page.waitForFunction((id) => {
    const state = window.__g1b92.state();
    return state.documentId === id && Object.hasOwn(state.shapes, window.__g1b92.ids.shapeA) &&
      Boolean(document.querySelector('[data-testid="workspace-loading"]'));
  }, "10000000-0000-4000-8000-000000000001", { timeout: 30000 });
  evidence.checks.push("production WorkspacePage restored cached A while withholding the editor pending its server identity response");

  await page.evaluate(() => window.__g1b92.navigateToB());
  await page.waitForFunction((id) => {
    const state = window.__g1b92.state();
    const root = document.querySelector("[data-renderer-status]");
    return state.documentId === id && state.currentPageId === window.__g1b92.ids.pageB && state.selectedIds.length === 0 &&
      Object.hasOwn(state.shapes, window.__g1b92.ids.shapeB) && !Object.hasOwn(state.shapes, window.__g1b92.ids.shapeA) &&
      root?.getAttribute("data-canvas-page-id") === window.__g1b92.ids.pageB &&
      root?.getAttribute("data-renderer-status") === "READY" && Number(root.getAttribute("data-renderer-frame-count")) > 0;
  }, "20000000-0000-4000-8000-000000000001", { timeout: 30000 });
  evidence.checks.push("route B identity, page, shape graph and production Canvas frame agree");

  await page.evaluate(() => window.__g1b92.releaseA());
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const finalState = await page.evaluate(() => ({
    ...window.__g1b92.state(),
    canvasPageId: document.querySelector("[data-canvas-page-id]")?.getAttribute("data-canvas-page-id"),
    rendererStatus: document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status"),
  }));
  assert.equal(finalState.documentId, "20000000-0000-4000-8000-000000000001");
  assert.equal(finalState.identitySource, "server");
  assert.equal(finalState.currentPageId, "20000000-0000-4000-8000-000000000002");
  assert.equal(finalState.canvasPageId, "20000000-0000-4000-8000-000000000002");
  assert.deepEqual(finalState.selectedIds, []);
  assert.ok(Object.hasOwn(finalState.shapes, "20000000-0000-4000-8000-000000000003"));
  assert.ok(!Object.hasOwn(finalState.shapes, "10000000-0000-4000-8000-000000000003"));
  assert.deepEqual(await page.evaluate(() => window.__g1b92.errors()), []);
  assert.deepEqual(evidence.browserErrors, []);
  evidence.checks.push("late A identity response cannot contaminate B document, selection projection, or renderer page");
  evidence.status = "passed";
} catch (error) {
  evidence.status = "failed";
  evidence.error = String(error?.stack ?? error);
  evidence.dom = page ? await page.content() : "";
  throw error;
} finally {
  if (page) {
    evidence.finalDom = await page.content();
    await writeFile(outputPath, JSON.stringify(evidence, null, 2));
  }
  await browser.close();
}
