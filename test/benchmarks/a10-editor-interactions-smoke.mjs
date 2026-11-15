import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { chromium } from "playwright";

const [url, outputPath] = process.argv.slice(2);
const executablePath = process.env.CHROME_BIN;
if (!url || !outputPath || !executablePath) throw new Error("Usage: CHROME_BIN=/path/to/chrome node a10-editor-interactions-smoke.mjs <url> <output>");

const browser = await chromium.launch({ executablePath, headless: true, args: ["--no-sandbox", "--disable-dev-shm-usage", "--enable-webgl", "--disable-features=WebGPU", "--ignore-gpu-blocklist", "--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"] });
const evidence = { status: "running", checks: [], interactionToReadyFrameMs: [], browserErrors: [] };
const pageErrors = [];
let page;
try {
  page = await browser.newPage({ viewport: { width: 1280, height: 740 }, deviceScaleFactor: 1 });
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
  let commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("Canvas Frame creation", () => dragOnCanvas(60, 50, 300, 250));
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "Frame creation must commit exactly once");
  let state = await getState();
  const frame = Object.values(state.shapes).find((shape) => shape.type === "frame");
  assert(frame, "Toolbar Frame tool did not create a Frame");
  await page.mouse.click((await canvasPoint(260, 210)).x, (await canvasPoint(260, 210)).y);
  assert.deepEqual(await getSelection(), [frame.id], "ordinary Canvas click did not select the Frame");
  assert((await page.getByTestId("inspector").textContent()).includes(frame.name), "Inspector did not reflect the Canvas selection");
  evidence.checks.push("Toolbar Frame creation, regular Canvas selection, and Inspector projection");

  await page.evaluate(() => window.__a10.stage("Layers insertion, hidden/locked behavior, rename, ordering and reparent"));
  let frameRow = await openLayerMenu(frame.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("child insertion through Layers", () => frameRow.getByRole("button", { name: `Add child to ${frame.name}` }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "child insertion must commit exactly once");
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
  frameRow = await openLayerMenu(frame.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("second child insertion through Layers", () => frameRow.getByRole("button", { name: `Add child to ${frame.name}` }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "second child insertion must commit exactly once");
  state = await getState();
  const secondChildId = state.shapes[frame.id].children.find((id) => id !== child.id);
  assert(secondChildId, "Layers did not create the second child needed for sibling reorder");
  const secondChild = state.shapes[secondChildId];
  const siblingOrderBefore = [...state.shapes[frame.id].children];
  const secondChildRow = await openLayerMenu(secondChild.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("sibling reorder through Layers", () => secondChildRow.getByRole("button", { name: `Move ${secondChild.name} down` }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "sibling reorder must commit exactly once");
  state = await getState();
  assert.deepEqual(state.shapes[frame.id].children, [...siblingOrderBefore].reverse(), "sibling reorder did not change order");

  await ready("hidden and locked rendering", () => page.evaluate((id) => window.__a10.setShapeFlags(id, { hidden: true, locked: true }), frame.id));
  assert(await page.evaluate((id) => window.__a10.rejectLockedRename(id), frame.id), "locked-object rename changed document or selection");
  assert(await page.evaluate(([parentId, descendantId]) => window.__a10.rejectHierarchyCycle(parentId, descendantId), [frame.id, child.id]),
    "cycle reparent changed document or selection");
  const outsideChild = await canvasPoint(260, 210);
  await page.mouse.click(outsideChild.x, outsideChild.y);
  assert.deepEqual(await getSelection(), [], "Canvas selected a hidden/locked Frame");
  await page.locator(`[data-layer-id="${frame.id}"]`).click();
  assert.deepEqual(await getSelection(), [frame.id], "Layers could not select hidden/locked Frame for inspection");
  await ready("restore Frame visibility and lock", () => page.evaluate((id) => window.__a10.setShapeFlags(id, { hidden: false, locked: false }), frame.id));

  frameRow = await openLayerMenu(frame.id);
  page.once("dialog", (dialog) => dialog.accept("Renamed Frame"));
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("rename Frame through Layers", () => frameRow.getByRole("button", { name: `Rename ${frame.name}` }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "rename must commit exactly once");
  await page.waitForFunction((id) => window.__a10.state().shapes[id]?.name === "Renamed Frame", frame.id);
  evidence.checks.push("Layers child insertion, invalid mutation rejection, hidden/locked selection policy, and rename");

  await page.keyboard.press("r");
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("Canvas Rectangle creation", () => dragOnCanvas(160, 120, 210, 170));
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "Rectangle creation must commit exactly once");
  state = await getState();
  const rootRect = Object.values(state.shapes).find((shape) => shape.type === "rect" && shape.parentId === null);
  assert(rootRect, "Canvas Rectangle tool did not create a root rectangle");
  const runFaultProbe = async (mode, category) => {
    const evidenceBefore = await page.evaluate(() => window.__a10.shadowEvidence().length);
    const commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
    if (mode) await page.evaluate((value) => window.__a10.setShadowFault(value), mode);
    await ready(`shadow fault isolation: ${mode ?? "WASM unavailable"}`, () => page.evaluate((value) =>
      value ? window.__a10.runFaultedRename() : window.__a10.runUnavailableEvaluator(), mode));
    await page.waitForFunction(([oldLength, expectedCategory]) =>
      window.__a10.shadowEvidence().slice(oldLength).some((item) => item.category === expectedCategory), [evidenceBefore, category]);
    const commitsAfter = await page.evaluate(() => window.__a10.shadowCommitCount());
    assert.equal(commitsAfter - commitsBefore, 1, `shadow ${mode ?? "unavailable"} failure caused an extra React document commit`);
  };
  await runFaultProbe(null, "SHADOW_EVALUATION_FAILED");
  await runFaultProbe("rust-rejected", "RUST_REJECTED");
  await runFaultProbe("invalid-response", "SHADOW_EVALUATION_FAILED");
  await runFaultProbe("throw", "SHADOW_EVALUATION_FAILED");
  await runFaultProbe("mismatch", "ELIGIBLE_MISMATCH");
  await page.evaluate(() => window.__a10.runRepeatedRename());
  await page.waitForFunction(() => window.__a10.shadowEvidence().some((item) => item.category === "ELIGIBLE_EQUIVALENT" && item.operation === "rename"));
  let row = await openLayerMenu(frame.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("root reorder up", () => row.getByRole("button", { name: "Move Renamed Frame up" }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "root reorder must commit exactly once");
  state = await getState();
  assert(state.pages[state.currentPageId].rootShapeIds.indexOf(frame.id) < state.pages[state.currentPageId].rootShapeIds.indexOf(rootRect.id), "root ordering did not change");
  row = await openLayerMenu(frame.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("root reorder restore", () => row.getByRole("button", { name: "Move Renamed Frame down" }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "root reorder restore must commit exactly once");
  row = await openLayerMenu(rootRect.id);
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("reparent root rectangle", () => row.locator("select").selectOption(frame.id));
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "reparent must commit exactly once");
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
  commitsBefore = await page.evaluate(() => window.__a10.shadowCommitCount());
  await ready("delete subtree through Layers", () => row.getByRole("button", { name: "Delete Renamed Frame" }).click());
  assert.equal(await page.evaluate(() => window.__a10.shadowCommitCount()) - commitsBefore, 1, "subtree delete must commit exactly once");
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
  const pageTwoEvidenceStart = await page.evaluate(() => window.__a10.shadowEvidence().length);
  await ready("create page-two recovery target", () => dragOnCanvas(40, 40, 120, 120));
  await page.waitForFunction((start) => window.__a10.shadowEvidence().slice(start).some((item) => item.category === "ADAPTER_UNSUPPORTED"), pageTwoEvidenceStart);
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
  const rendererStatusBeforeVector = await page.locator("[data-renderer-status]").evaluate((element) => ({
    state: element.getAttribute("data-renderer-status"),
    diagnostic: element.getAttribute("data-renderer-diagnostic"),
    frameCount: element.getAttribute("data-renderer-frame-count"),
  }));
  const vectorId = await page.evaluate(() => window.__a10.createVectorNetworkForCompatibility());
  await page.waitForFunction((id) => window.__a10.state().shapes[id]?.type === "vector-network", vectorId, { timeout: 5000 });
  await page.waitForFunction((previous) => {
    const status = document.querySelector("[data-renderer-status]");
    const state = status?.getAttribute("data-renderer-status");
    return state !== previous.state && state !== "READY" && state !== "RENDERING" && state !== "RECOVERING";
  }, rendererStatusBeforeVector, { timeout: 20000 });
  const vectorRendererStatus = await page.locator("[data-renderer-status]").evaluate((element) => ({
    state: element.getAttribute("data-renderer-status"),
    diagnostic: element.getAttribute("data-renderer-diagnostic"),
    frameCount: element.getAttribute("data-renderer-frame-count"),
  }));
  assert.deepEqual(vectorRendererStatus.state, "SCENE_REJECTED", `Vector Network renderer status did not report scene rejection: ${JSON.stringify(vectorRendererStatus)}`);
  assert.deepEqual(vectorRendererStatus.diagnostic, "LOGOS_SCENE_UNSUPPORTED_SHAPE", `Vector Network renderer diagnostic did not report the A9 unsupported-shape limit: ${JSON.stringify(vectorRendererStatus)}`);
  const vectorState = await getState();
  const vector = vectorState.shapes[vectorId];
  assert(vector?.vnAnchors?.length === 2 && vector.vnSegments?.length === 1, "Vector Network creation did not retain geometry");
  assert((await page.getByTestId("inspector").textContent()).includes(vector.name), "Inspector did not retain the rejected Vector Network selection");
  await page.evaluate((id) => window.__a10.setShapeTypeForCompatibilityRecovery(id), vector.id);
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "READY", undefined, { timeout: 20000 });
  assert.equal((await getState()).shapes[vector.id].vnAnchors.length, 2, "renderer recovery discarded Vector Network geometry");

  const componentId = await page.evaluate(() => window.__a10.createUnsupportedComponentForCompatibility());
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "SCENE_REJECTED" &&
    document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-diagnostic") === "LOGOS_SCENE_UNSUPPORTED_SHAPE", undefined, { timeout: 20000 });
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
  await page.waitForFunction(() => document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-status") === "SCENE_REJECTED" &&
    document.querySelector("[data-renderer-status]")?.getAttribute("data-renderer-diagnostic") === "LOGOS_SCENE_UNSUPPORTED_SHAPE", undefined, { timeout: 20000 });
  evidence.checks.push("Vector Network creation rendered with retained geometry; unsupported legacy component data persisted across diagnostic and recovery; multi-vector replacement and selection/root-order reconciliation");

  await ready("return to first page for stale-result probe", () => page.evaluate(() => window.__a10.returnToFirstPage()));
  const discardBeforePageSwitch = await page.evaluate(() => window.__a10.shadowDiscardCount());
  await ready("stale shadow result discarded after page switch", () => page.evaluate(() => window.__a10.exerciseStalePageSwitch()));
  await page.waitForFunction((oldCount) => window.__a10.shadowDiscardCount() > oldCount, discardBeforePageSwitch);
  const discardBeforeUnmount = await page.evaluate(() => window.__a10.shadowDiscardCount());
  await ready("return to first page before unmount probe", () => page.evaluate(() => window.__a10.returnToFirstPage()));
  await page.evaluate(() => window.__a10.unmountDuringShadow());
  await page.waitForFunction((oldCount) => window.__a10.shadowDiscardCount() > oldCount, discardBeforeUnmount);
  const shadowEvidence = await page.evaluate(() => window.__a10.shadowEvidence());
  const equivalentOperations = new Set(shadowEvidence.filter((item) => item.category === "ELIGIBLE_EQUIVALENT").map((item) => item.operation));
  for (const operation of ["create-root", "insert-child", "rename", "reorder", "reparent", "delete"]) {
    assert(equivalentOperations.has(operation), `no real editor ${operation} operation reached Rust equivalence`);
  }
  assert(shadowEvidence.some((item) => item.category === "REACT_REJECTED"), "React rejection was not classified by the shadow boundary");
  assert(shadowEvidence.some((item) => item.category === "LEGACY_COMPATIBILITY"), "legacy compatibility operation was not classified");
  assert(shadowEvidence.some((item) => item.category === "ADAPTER_UNSUPPORTED"), "multi-page/nanoid compatibility boundary was not classified");
  assert.equal(shadowEvidence.filter((item) => item.category === "ELIGIBLE_MISMATCH").length, 1,
    "injected semantic mismatch was not uniquely classified or an unexpected mismatch occurred");
  for (const item of shadowEvidence.filter((entry) => entry.reactStateUnchanged !== undefined)) {
    assert.equal(item.reactStateUnchanged, true, `shadow evaluation changed React state for ${item.operation}`);
    assert.equal(item.selectionUnchanged, true, `shadow evaluation changed selection for ${item.operation}`);
  }
  const commitCountAfterShadow = await page.evaluate(() => window.__a10.shadowCommitCount());
  evidence.rustShadow = {
    classifications: shadowEvidence,
    equivalentOperations: [...equivalentOperations],
    documentStoreStructuralCommits: commitCountAfterShadow,
    staleComparisonsDiscarded: await page.evaluate(() => window.__a10.shadowDiscardCount()),
    jsHeapUsedBytes: await page.evaluate(() => performance.memory?.usedJSHeapSize ?? null),
    wasmReservedBytes: await page.evaluate(() => window.__a10.shadowWasmReservedBytes()),
    memoryNote: "Point observations only; no retained-memory or leak claim.",
  };
  evidence.checks.push("Rust shadow evaluation used the actual React hierarchy actions; stale page-switch and editor-unmount results were discarded without a Rust commit");

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
  evidence.failureContext = await page?.locator("[data-renderer-status]").evaluate((element) => ({
    renderer: {
      state: element.getAttribute("data-renderer-status"),
      backend: element.getAttribute("data-renderer-backend"),
      diagnostic: element.getAttribute("data-renderer-diagnostic"),
      frameCount: element.getAttribute("data-renderer-frame-count"),
      metrics: Boolean(element.getAttribute("data-renderer-metrics")),
      frameDisposition: element.getAttribute("data-renderer-frame-disposition"),
    },
    shapes: window.__a10?.state ? Object.values(window.__a10.state().shapes).map(({ id, type, parentId, children }) => ({ id, type, parentId, children })) : [],
    selection: window.__a10?.selection?.() ?? [],
  })).catch(() => null);
  await writeFile(outputPath, JSON.stringify(evidence, null, 2));
  console.error(JSON.stringify(evidence, null, 2));
  process.exitCode = 1;
} finally {
  await browser.close();
}
