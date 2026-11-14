import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const [url, outputPath] = process.argv.slice(2);
const executablePath = process.env.CHROME_BIN;
if (!url || !outputPath || !executablePath) throw new Error("Usage: CHROME_BIN=/path/to/chrome node a10-editor-interactions-smoke.mjs <url> <output>");

const browser = await chromium.launch({ executablePath, headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-webgl", "--disable-features=WebGPU", "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const evidence = { status: "running", checks: [], interactionToReadyFrameMs: [], browserErrors: [] };
const pageErrors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 740 }, deviceScaleFactor: 1 });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto(url, { waitUntil: "load", timeout: 30000 });
  await page.waitForFunction(() => document.querySelector("#a10-status")?.dataset.status === "ready", undefined, { timeout: 90000 });
  const ready = async (label, action) => {
    const before = Number(await page.locator("[data-renderer-status]").getAttribute("data-renderer-frame-count"));
    const started = performance.now();
    await action();
    await page.waitForFunction((oldFrameCount) => {
      const root = document.querySelector("[data-renderer-status]");
      return root?.getAttribute("data-renderer-status") === "READY" &&
        Boolean(root.getAttribute("data-renderer-metrics")) &&
        Number(root.getAttribute("data-renderer-frame-count")) > oldFrameCount;
    }, before, { timeout: 20000 });
    evidence.interactionToReadyFrameMs.push({ label, milliseconds: performance.now() - started });
  };
  const getState = () => page.evaluate(() => window.__a10.state());
  const getSelection = () => page.evaluate(() => window.__a10.selection());
  const canvasPoint = async (x, y) => {
    const box = await page.locator("canvas").boundingBox();
    assert(box, "Canvas has no browser bounding box");
    return { x: box.x + x, y: box.y + y };
  };
  const dragOnCanvas = async (x1, y1, x2, y2) => {
    const start = await canvasPoint(x1, y1);
    const end = await canvasPoint(x2, y2);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.mouse.move(end.x, end.y, { steps: 8 });
    await page.mouse.up();
  };
  const openLayerMenu = async (id) => {
    const row = page.locator(`[data-layer-id="${id}"]`);
    await row.scrollIntoViewIfNeeded();
    // Native <details> menus remain open independently. Close another row's
    // menu through its visible summary before opening this one so overlays
    // cannot intercept subsequent real pointer interactions.
    for (const menu of await page.locator('[data-layer-id] details[open]').all()) {
      const menuRowId = await menu.evaluate((details) => details.closest("[data-layer-id]")?.getAttribute("data-layer-id"));
      if (menuRowId !== id) await menu.locator("summary").click();
    }
    if (!(await row.locator("details").evaluate((details) => details.open))) await row.locator("summary").click();
    return row;
  };

  await page.evaluate(() => window.__a10.stage("Canvas Frame creation and regular click selection"));
  await page.keyboard.press("f");
  await ready("Canvas Frame creation", () => dragOnCanvas(60, 50, 300, 250));
  let state = await getState();
  const frame = Object.values(state.shapes).find((shape) => shape.type === "frame");
  assert(frame, "Toolbar Frame tool did not create a Frame");
  await page.mouse.click((await canvasPoint(260, 210)).x, (await canvasPoint(260, 210)).y);
  assert.deepEqual(await getSelection(), [frame.id], "ordinary Canvas click did not select the Frame");
  assert((await page.getByTestId("inspector").textContent()).includes(frame.name), "Inspector did not reflect the Canvas selection");
  evidence.checks.push("Toolbar Frame creation, regular Canvas selection, and Inspector projection");

  await page.evaluate(() => window.__a10.stage("Layers insertion, hidden/locked behavior, rename, ordering and reparent"));
  let frameRow = await openLayerMenu(frame.id);
  await ready("child insertion through Layers", () => frameRow.getByRole("button", { name: `Add child to ${frame.name}` }).click());
  await page.waitForFunction((frameId) => window.__a10.selection().length === 1 && window.__a10.state().shapes[window.__a10.selection()[0]]?.parentId === frameId, frame.id);
  state = await getState();
  const child = Object.values(state.shapes).find((shape) => shape.parentId === frame.id);
  assert(child, "Layers did not insert a child rectangle");
  const nestedPoint = await canvasPoint(75, 65);
  await page.mouse.click(nestedPoint.x, nestedPoint.y);
  assert.deepEqual(await getSelection(), [child.id], "regular Canvas click did not select the nested child");
  assert((await page.getByTestId("inspector").textContent()).includes(child.name), "Inspector did not show nested Canvas selection");
  const childPixel = await page.evaluate(() => window.__a10.samplePixel(75, 65));
  assert.deepEqual(childPixel, [0, 0, 255, 255], "child pixel did not appear in the Rust renderer");
  assert(await page.evaluate((id) => window.__a10.rejectInvalidChild(id), frame.id), "invalid operation mutated document or selection");

  await ready("hidden and locked rendering", () => page.evaluate((id) => window.__a10.setShapeFlags(id, { hidden: true, locked: true }), frame.id));
  const outsideChild = await canvasPoint(260, 210);
  await page.mouse.click(outsideChild.x, outsideChild.y);
  assert.deepEqual(await getSelection(), [], "Canvas selected a hidden/locked Frame");
  await page.locator(`[data-layer-id="${frame.id}"]`).click();
  assert.deepEqual(await getSelection(), [frame.id], "Layers could not select hidden/locked Frame for inspection");
  await ready("restore Frame visibility and lock", () => page.evaluate((id) => window.__a10.setShapeFlags(id, { hidden: false, locked: false }), frame.id));

  frameRow = await openLayerMenu(frame.id);
  page.once("dialog", (dialog) => dialog.accept("Renamed Frame"));
  await ready("rename Frame through Layers", () => frameRow.getByRole("button", { name: `Rename ${frame.name}` }).click());
  await page.waitForFunction((id) => window.__a10.state().shapes[id]?.name === "Renamed Frame", frame.id);
  evidence.checks.push("Layers child insertion, invalid mutation rejection, hidden/locked selection policy, and rename");

  await page.keyboard.press("r");
  await ready("Canvas Rectangle creation", () => dragOnCanvas(160, 120, 210, 170));
  state = await getState();
  const rootRect = Object.values(state.shapes).find((shape) => shape.type === "rect" && shape.parentId === null);
  assert(rootRect, "Canvas Rectangle tool did not create a root rectangle");
  let row = await openLayerMenu(frame.id);
  await ready("root reorder up", () => row.getByRole("button", { name: "Move Renamed Frame up" }).click());
  state = await getState();
  assert(state.pages[state.currentPageId].rootShapeIds.indexOf(frame.id) < state.pages[state.currentPageId].rootShapeIds.indexOf(rootRect.id), "root ordering did not change");
  row = await openLayerMenu(frame.id);
  await ready("root reorder restore", () => row.getByRole("button", { name: "Move Renamed Frame down" }).click());
  row = await openLayerMenu(rootRect.id);
  await ready("reparent root rectangle", () => row.locator("select").selectOption(frame.id));
  await page.waitForFunction(([shapeId, parentId]) => window.__a10.state().shapes[shapeId]?.parentId === parentId, [rootRect.id, frame.id]);
  state = await getState();
  assert.equal(state.shapes[rootRect.id].bounds.x, 160, "reparent changed world-space x");
  assert(state.shapes[frame.id].children.includes(rootRect.id), "reparent did not update parent membership");
  evidence.checks.push("Root reordering and axis-aligned reparent with world-space bounds preserved");

  // Select a descendant in Layers, then remove its owning frame through its layer menu.
  // Close the Rectangle's still-open action menu first so it cannot cover the
  // child row and intercept a real pointer click.
  await row.locator("summary").click();
  await page.locator(`[data-layer-id="${child.id}"]`).click();
  row = await openLayerMenu(frame.id);
  await ready("delete subtree through Layers", () => row.getByRole("button", { name: "Delete Renamed Frame" }).click());
  await page.waitForFunction(([frameId, childId, rectId]) => !window.__a10.state().shapes[frameId] && !window.__a10.state().shapes[childId] && !window.__a10.state().shapes[rectId], [frame.id, child.id, rootRect.id]);
  assert.deepEqual(await getSelection(), [], "delete did not reconcile selected descendants");
  const deletedPixel = await page.evaluate(() => window.__a10.samplePixel(75, 65));
  assert.deepEqual(deletedPixel, [0, 0, 0, 0], "subtree deletion did not clear old pixels");
  await page.keyboard.press("r");
  await ready("create first-page selection for page switch", () => dragOnCanvas(360, 80, 430, 150));
  const firstPageSelection = await getSelection();
  assert(firstPageSelection.length === 1, "new page-switch target was not selected");
  await page.evaluate(() => window.__a10.addPage());
  await ready("page switch", () => page.getByTestId("a10-switch-page").click());
  await page.waitForFunction(() => window.__a10.selection().length === 0);
  const pageSwitchPixel = await page.evaluate(() => window.__a10.samplePixel(380, 100));
  assert.deepEqual(pageSwitchPixel, [0, 0, 0, 0], "page switch left stale pixels from previous page");
  evidence.checks.push("Subtree deletion, descendant selection cleanup, stale pixels, and page-switch cleanup");

  await page.keyboard.press("r");
  await ready("create page-two recovery target", () => dragOnCanvas(40, 40, 120, 120));
  const instancesBefore = await page.evaluate(() => window.__a10.rendererInstances());
  await page.evaluate(() => window.__a10.poisonRenderer());
  await page.waitForFunction(() => document.querySelector("[data-renderer-diagnostic]")?.getAttribute("data-renderer-diagnostic") === "LOGOS_SCENE_RECOVERY_FAILED", undefined, { timeout: 20000 });
  assert.equal(await page.locator("[data-renderer-status]").getAttribute("data-renderer-frame-disposition"), "unverified", "poisoned output was not marked unverified");
  await page.evaluate(() => window.__a10.clearFailure());
  await page.getByRole("button", { name: "Recreate renderer" }).click();
  await page.waitForFunction((oldCount) => window.__a10.rendererInstances() > oldCount, instancesBefore, { timeout: 30000 });
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "READY" && Boolean(document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-metrics")), undefined, { timeout: 30000 });
  const recoveredPixel = await page.evaluate(() => window.__a10.samplePixel(70, 60));
  assert(recoveredPixel[3] > 0, `recreated renderer did not redraw the retained document: ${recoveredPixel}`);
  evidence.checks.push("Poisoned WASM module rejected further uploads and recreated with fresh module/context");

  await page.evaluate(() => window.__a10.stage("Vector Network compatibility creation diagnostics"));
  const vectorId = await page.evaluate(() => window.__a10.createVectorNetworkForCompatibility());
  await page.waitForFunction(() => {
    const shapes = Object.values(window.__a10.state().shapes);
    return shapes.some((shape) => shape.type === "vector-network") &&
      document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "SCENE_REJECTED" &&
      document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-diagnostic") === "LOGOS_SCENE_UNSUPPORTED_CAPABILITY";
  }, undefined, { timeout: 20000 });
  const vectorState = await getState();
  const vector = vectorState.shapes[vectorId];
  assert(vector?.vnAnchors?.length === 2 && vector.vnSegments?.length === 1, "Vector Network creation did not retain geometry");
  assert((await page.getByTestId("inspector").textContent()).includes(vector.name), "Inspector did not retain the rejected Vector Network selection");
  await page.evaluate((id) => window.__a10.setShapeTypeForCompatibilityRecovery(id), vector.id);
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "READY", undefined, { timeout: 20000 });
  assert.equal((await getState()).shapes[vector.id].vnAnchors.length, 2, "renderer recovery discarded Vector Network geometry");

  const componentId = await page.evaluate(() => window.__a10.createUnsupportedComponentForCompatibility());
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "UNSUPPORTED_SHAPE" &&
    document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-diagnostic") === "LOGOS_RENDER_UNSUPPORTED_SHAPE", undefined, { timeout: 20000 });
  let legacyState = await getState();
  const component = legacyState.shapes[componentId];
  assert.deepEqual(component.vendorExtension, { sentinel: "preserved-by-compatibility-operation" }, "compatibility add discarded an unknown visual extension");
  assert((await page.getByTestId("inspector").textContent()).includes(component.name), "Inspector did not retain selection for unsupported component content");
  await page.evaluate((id) => window.__a10.setShapeTypeForCompatibilityRecovery(id), componentId);
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "READY", undefined, { timeout: 20000 });
  legacyState = await getState();
  assert.deepEqual(legacyState.shapes[componentId].vendorExtension, { sentinel: "preserved-by-compatibility-operation" }, "renderer recovery discarded preserved extension data");

  const replacementEvidence = await page.evaluate(() => window.__a10.replaceTwoVectorNetworks());
  legacyState = await getState();
  assert.equal(legacyState.shapes[replacementEvidence.firstId], undefined, "multi-shape replacement left its first input behind");
  assert.equal(legacyState.shapes[replacementEvidence.secondId], undefined, "multi-shape replacement left its second input behind");
  assert.equal(legacyState.shapes[replacementEvidence.replacementId].type, "vector-network", "replacement was not retained as a Vector Network");
  assert.deepEqual(replacementEvidence.rootsAfter.filter((id) => id !== replacementEvidence.replacementId), replacementEvidence.rootsBefore, "replacement changed the relative root order of unaffected shapes");
  assert.deepEqual(replacementEvidence.selection, [replacementEvidence.replacementId], "selection did not reconcile to the replacement shape");
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "UNSUPPORTED_SHAPE", undefined, { timeout: 20000 });
  evidence.checks.push("Vector Network creation rendered with retained geometry; unsupported legacy component data persisted across diagnostic and recovery; multi-vector replacement and selection/root-order reconciliation");

  evidence.status = "passed";
  evidence.environment = await page.evaluate(() => ({ userAgent: navigator.userAgent, platform: navigator.platform, logicalProcessors: navigator.hardwareConcurrency, deviceMemoryGiB: navigator.deviceMemory ?? null, viewport: { width: innerWidth, height: innerHeight, devicePixelRatio }, wasmReservedBytes: window.__a10.wasmReservedBytes() }));
  evidence.rendererInstances = await page.evaluate(() => window.__a10.rendererInstances());
  evidence.pixelSamples = { childBeforeDeletion: childPixel, afterDeletion: deletedPixel, afterPageSwitch: pageSwitchPixel, afterRecovery: recoveredPixel };
  evidence.browserErrors = [...new Set([...pageErrors, ...(await page.evaluate(() => window.__a10.errors()))])];
  assert.deepEqual(evidence.browserErrors, [], "browser emitted uncaught errors");
  evidence.notes = ["Interaction-to-frame observations are measured from action to the next completed renderer frame and are separate from isolated renderer benchmarks.", "Disposable Chromium test only. No deployment resources or credentials are used."];
  await writeFile(outputPath, JSON.stringify(evidence, null, 2));
  console.log(JSON.stringify(evidence, null, 2));
} catch (error) {
  evidence.status = "failed";
  evidence.error = error instanceof Error ? error.stack ?? error.message : String(error);
  await writeFile(outputPath, JSON.stringify(evidence, null, 2));
  console.error(JSON.stringify(evidence, null, 2));
  process.exitCode = 1;
} finally {
  await browser.close();
}
