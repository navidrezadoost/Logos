import { createElement, useEffect } from "react";
import { createRoot } from "react-dom/client";
import initRust, { evaluate_document_operation_json, validate_document_json } from "../document-core/rust-wasm/logos_types";
import supportedFixture from "../document-core/fixtures/react-supported-v1.json";
import operationFixture from "../document-core/fixtures/g1-b8-operation-conformance-v1.json";
import { validateReactSnapshot, type ReactDocumentSnapshot } from "../document-core/react-adapter";
import { evaluateHierarchyOperationConformance } from "../document-core/operation-conformance";
import { planHierarchyOperation, type HierarchyDocumentSnapshot } from "../stores/hierarchyOperations";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import type { Shape } from "../types/shapes";

const resultNode = document.querySelector<HTMLPreElement>("#result")!;
const storageWrites: string[] = [];
const databaseOpens: string[] = [];
const originalSetItem = Storage.prototype.setItem;
Storage.prototype.setItem = function (key, value) {
  storageWrites.push(key);
  return originalSetItem.call(this, key, value);
};
const originalOpen = indexedDB.open.bind(indexedDB);
indexedDB.open = ((name: string, version?: number) => {
  databaseOpens.push(name);
  return originalOpen(name, version);
}) as typeof indexedDB.open;

function uuid(number: number): string {
  return `00000000-0000-4000-8000-${number.toString(16).padStart(12, "0")}`;
}

function shape(id: number, type: Shape["type"], parentId: string | null, children: string[], x: number): Shape {
  return {
    id: uuid(id), type, name: `${type}-${id}`,
    bounds: { x, y: 20, w: 100, h: 60 }, transform: [1, 0, 0, 1, x, 20],
    rotation: 0, fills: type === "group" ? [] : [{ type: "solid", color: "#336699", opacity: 0.85 }],
    opacity: 0.9, hidden: false, locked: false, parentId, children,
  };
}

function fixture(objectCount = 4): ReactDocumentSnapshot {
  const snapshot = JSON.parse(JSON.stringify(supportedFixture)) as ReactDocumentSnapshot;
  const roots = snapshot.pages[snapshot.pageOrder[0]].rootShapeIds;
  for (let index = Object.keys(snapshot.shapes).length; index < objectCount; index += 1) {
    const id = 100 + index;
    const rootId = uuid(id);
    roots.push(rootId);
    snapshot.shapes[rootId] = shape(id, "rect", null, [], index);
  }
  return snapshot;
}

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [
    key, canonical((value as Record<string, unknown>)[key]),
  ]));
}

function deepFreeze<T>(value: T): T {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const entry of Object.values(value as Record<string, unknown>)) deepFreeze(entry);
  }
  return value;
}

function percentile(values: number[], percentileValue: number): number {
  const ordered = [...values].sort((left, right) => left - right);
  return ordered[Math.max(0, Math.ceil(percentileValue * ordered.length) - 1)];
}

async function run(): Promise<void> {
  const wasmExports = await initRust();
  const stateBefore = JSON.stringify(useDocumentStore.getState());
  const selectionBefore = JSON.stringify(useSelectionStore.getState().selectedIds);
  const localStorageBefore = JSON.stringify(Object.keys(localStorage).sort());
  const liveState = useDocumentStore.getState();
  const liveSnapshot: ReactDocumentSnapshot = {
    documentId: uuid(1), pages: liveState.pages, pageOrder: liveState.pageOrder, shapes: liveState.shapes,
  };
  const liveResult = validateReactSnapshot(liveSnapshot, { validateDocumentJson: validate_document_json });
  assert(liveResult.ok,
    `fresh Zustand page UUID was not accepted by the Rust adapter: ${JSON.stringify(liveResult)}`);
  assert(JSON.stringify(canonical(liveResult.roundTrip)) === JSON.stringify(canonical(liveSnapshot)),
    "fresh Zustand page identity changed during Rust adapter round trip");
  const snapshot = deepFreeze(fixture());
  const valid = validateReactSnapshot(snapshot, { validateDocumentJson: validate_document_json },
    () => wasmExports.memory.buffer.byteLength);
  assert(valid.ok, `valid supported snapshot rejected: ${JSON.stringify(valid)}`);
  assert(JSON.stringify(canonical(valid.roundTrip)) === JSON.stringify(canonical(snapshot)),
    "supported fixture changed semantic fields during Rust round trip");
  assert(JSON.stringify(useDocumentStore.getState()) === stateBefore, "production Zustand state changed");

  const invalidParent = clone(fixture());
  const group = Object.values(invalidParent.shapes).find((item) => item.type === "group")!;
  group.children = [];
  const invalid = validateReactSnapshot(invalidParent, { validateDocumentJson: validate_document_json });
  assert(!invalid.ok && invalid.diagnostics.some((diagnostic) => diagnostic.code === "PARENT_CHILD_MISMATCH"),
    `invalid backlink was not rejected by Rust: ${JSON.stringify(invalid)}`);
  const heapMemory = () => (performance as Performance & { memory?: { usedJSHeapSize: number } })
    .memory?.usedJSHeapSize ?? null;
  const heapBeforeLifecycle = heapMemory();
  const rejectedValidationMs: number[] = [];
  for (let cycle = 0; cycle < 5; cycle += 1) {
    const rejectedStart = performance.now();
    const rejected = validateReactSnapshot(invalidParent, { validateDocumentJson: validate_document_json });
    rejectedValidationMs.push(performance.now() - rejectedStart);
    assert(!rejected.ok && rejected.diagnostics.some((diagnostic) => diagnostic.code === "PARENT_CHILD_MISMATCH"),
      "repeated rejected-document validation changed outcome");
  }
  const heapAfterLifecycle = heapMemory();

  const unsupported = clone(fixture());
  const rect = Object.values(unsupported.shapes).find((item) => item.type === "rect")!;
  (rect as Shape & { text: string }).text = "unsupported";
  const unsupportedResult = validateReactSnapshot(unsupported, { validateDocumentJson: validate_document_json });
  assert(!unsupportedResult.ok && unsupportedResult.issues.some((item) => item.code === "UNSUPPORTED_PROPERTY"),
    "unsupported text property was silently discarded");

  const translatedParent = clone(fixture());
  const translatedFrame = Object.values(translatedParent.shapes).find((item) => item.type === "frame")!;
  const translatedGroup = Object.values(translatedParent.shapes).find((item) => item.type === "group")!;
  const translatedRect = Object.values(translatedParent.shapes).find((item) => item.type === "rect")!;
  translatedFrame.bounds = { ...translatedFrame.bounds, x: 100, y: 50 };
  translatedGroup.bounds = { ...translatedGroup.bounds, x: 120, y: 70 };
  translatedRect.bounds = { ...translatedRect.bounds, x: 125, y: 75 };
  // Use importer-neutral identity matrices so this fixture tests only the
  // established page-position to parent-local translation conversion.
  translatedRect.transform = [1, 0, 0, 1, 0, 0];
  const coordinateResult = validateReactSnapshot(translatedParent, { validateDocumentJson: validate_document_json });
  assert(coordinateResult.ok, `translated nested document rejected: ${JSON.stringify(coordinateResult)}`);
  assert(JSON.stringify(canonical(coordinateResult.roundTrip)) === JSON.stringify(canonical(translatedParent)),
    "translated frame/group/rectangle did not round-trip in page coordinates");
  const translatedCandidate = coordinateResult.candidate as { shapes: Array<{ id: string; selrect: { x: number; y: number } }> };
  const localRect = translatedCandidate.shapes.find((item) => item.id === translatedRect.id)?.selrect;
  assert(localRect?.x === 5 && localRect.y === 5, "React page-space rect did not normalize to immediate-parent local coordinates");

  const rotatedParent = clone(translatedParent);
  const rotatedFrame = Object.values(rotatedParent.shapes).find((item) => item.type === "frame")!;
  rotatedFrame.rotation = 15;
  const rotatedCoordinateResult = validateReactSnapshot(rotatedParent, { validateDocumentJson: validate_document_json });
  assert(!rotatedCoordinateResult.ok && rotatedCoordinateResult.issues.some((item) => item.code === "UNVERIFIED_COORDINATE_SPACE"),
    "rotated parent with unresolved React pivot semantics did not fail closed");

  const noIdentity = clone(fixture());
  noIdentity.documentId = "caller-must-supply-uuid";
  const identityResult = validateReactSnapshot(noIdentity, { validateDocumentJson: validate_document_json });
  assert(!identityResult.ok && identityResult.issues.some((item) => item.code === "IDENTITY_REQUIRED"),
    "missing document identity was not rejected");

  const pageId = Object.keys(fixture().pages)[0];
  const badPageId = clone(fixture());
  const page = badPageId.pages[pageId];
  delete badPageId.pages[pageId];
  badPageId.pages["react-nanoid-page"] = { ...page, id: "react-nanoid-page" };
  badPageId.pageOrder = ["react-nanoid-page"];
  const pageIdResult = validateReactSnapshot(badPageId, { validateDocumentJson: validate_document_json });
  assert(!pageIdResult.ok && pageIdResult.issues.some((item) => item.code === "UNSUPPORTED_PAGE_ID"),
    "nanoid page identity was silently remapped");

  const conformanceBaseline = deepFreeze(clone(operationFixture.reactBaseline)) as unknown as ReactDocumentSnapshot;
  const conformanceBefore = JSON.stringify(conformanceBaseline);
  const cases = new Map(operationFixture.cases.map((item) => [item.name, item]));
  const conformanceResults: Array<{ name: string; outcome: string; equivalent?: boolean; diagnostic?: string;
    timingsMs?: { reactPlanningMs: number; candidateConversionMs: number; rustOperationEvaluationMs: number;
      reactProjectionMs: number; totalMs: number } }> = [];
  let rustEvaluationCalls = 0;
  const runConformance = (name: string, input: ReactDocumentSnapshot, operation: Parameters<typeof evaluateHierarchyOperationConformance>[1]) => {
    const fixtureCase = cases.get(name);
    assert(fixtureCase, `missing shared operation fixture ${name}`);
    const before = JSON.stringify(input);
    const result = evaluateHierarchyOperationConformance(input, operation, {
      evaluateDocumentOperationJson: (documentJson, operationJson) => {
        rustEvaluationCalls += 1;
        return evaluate_document_operation_json(documentJson, operationJson);
      },
    });
    assert(JSON.stringify(input) === before, `${name}: conformance evaluation mutated the React input`);
    assert(JSON.stringify(conformanceBaseline) === conformanceBefore, `${name}: shared baseline mutated`);
    if (fixtureCase.expected === "success") {
      assert(result.ok, `${name}: eligible operation rejected: ${JSON.stringify(result)}`);
      assert(result.semanticEquivalent, `${name}: full React/Rust semantic projection differs: ${JSON.stringify(result)}`);
      conformanceResults.push({ name, outcome: "equivalent", equivalent: true, timingsMs: result.timingsMs });
      return result;
    }
    assert(!result.ok, `${name}: ineligible operation unexpectedly produced a Rust candidate`);
    const expectedPhases = fixtureCase.expected === "react-reject" ? ["react-plan"]
      : fixtureCase.expected === "adapter-reject" ? ["adapter"] : ["react-plan", "adapter"];
    assert(expectedPhases.includes(result.phase), `${name}: wrong rejection boundary ${result.phase}: ${JSON.stringify(result)}`);
    const diagnostic = result.phase === "adapter" ? result.issues?.[0]?.code ?? result.code
      : result.diagnostics?.[0]?.code ?? result.code;
    assert(fixtureCase.diagnostic === diagnostic,
      `${name}: stable diagnostic changed; expected ${fixtureCase.diagnostic}, got ${diagnostic}`);
    conformanceResults.push({ name, outcome: result.phase, diagnostic });
    return result;
  };
  const conformBase = clone(conformanceBaseline);
  const [outerFrameId, secondRootId] = conformBase.pages[conformBase.pageOrder[0]].rootShapeIds;
  const [nestedGroupId, secondChildId] = conformBase.shapes[outerFrameId].children;
  const nestedRectId = conformBase.shapes[nestedGroupId].children[0];
  const newRootRect = shape(801, "rect", null, [], 0);
  newRootRect.bounds = { x: 20, y: 20, w: 40, h: 30 };
  newRootRect.transform = [1, 0, 0, 1, 0, 0];
  const newRootFrame = shape(802, "frame", null, [], 0);
  newRootFrame.bounds = { x: 20, y: 20, w: 80, h: 60 };
  newRootFrame.transform = [1, 0, 0, 1, 0, 0];
  const insertedChild = shape(803, "rect", null, [], 0);
  insertedChild.bounds = { x: 140, y: 50, w: 20, h: 20 };
  insertedChild.transform = [1, 0, 0, 1, 0, 0];
  runConformance("create-root-rectangle", conformBase, { kind: "create-root", shape: newRootRect });
  runConformance("create-root-frame", conformBase, { kind: "create-root", shape: newRootFrame });
  runConformance("insert-child", conformBase, { kind: "insert-child", parentId: outerFrameId, shape: insertedChild, index: 1 });
  runConformance("rename", conformBase, { kind: "rename", shapeId: nestedGroupId, name: "  Renamed group  " });
  runConformance("reorder-roots", conformBase, { kind: "reorder-root-list", rootShapeIds: [secondRootId, outerFrameId] });
  runConformance("reorder-siblings", conformBase, { kind: "reorder", shapeId: secondChildId, toIndex: 0 });
  runConformance("delete-subtree", conformBase, { kind: "delete", shapeId: nestedGroupId });
  runConformance("reparent-world-position", conformBase, { kind: "reparent", shapeId: nestedRectId,
    parentId: outerFrameId, index: 2, preserveWorldPosition: true });
  runConformance("reject-cycle", conformBase, { kind: "reparent", shapeId: outerFrameId,
    parentId: nestedRectId, index: 0, preserveWorldPosition: true });
  runConformance("reject-invalid-sibling-membership", conformBase, { kind: "reorder-root-list",
    rootShapeIds: [outerFrameId, outerFrameId] });
  runConformance("reject-duplicate-id", conformBase, { kind: "create-root", shape: { ...newRootRect, id: secondRootId } });
  const lockedSnapshot = clone(conformBase);
  lockedSnapshot.shapes[nestedGroupId].locked = true;
  runConformance("reject-locked-mutation", lockedSnapshot, { kind: "rename", shapeId: nestedGroupId, name: "Must reject" });
  const multiPageSnapshot = clone(conformBase);
  const secondPageId = uuid(804);
  multiPageSnapshot.pages[secondPageId] = { id: secondPageId, name: "Second page", rootShapeIds: [] };
  multiPageSnapshot.pageOrder.push(secondPageId);
  runConformance("reject-cross-page-ambiguity", multiPageSnapshot, { kind: "rename", shapeId: nestedGroupId, name: "Must stay local" });
  const unsupportedGeometry = clone(conformBase);
  unsupportedGeometry.shapes[nestedRectId].type = "vector-network";
  runConformance("reject-unsupported-geometry", unsupportedGeometry, { kind: "rename", shapeId: nestedGroupId, name: "Legacy gate" });
  const unknownExtension = clone(conformBase) as ReactDocumentSnapshot & { futureExtension?: unknown };
  unknownExtension.futureExtension = { keep: true };
  runConformance("preserve-unknown-extensions", unknownExtension, { kind: "rename", shapeId: nestedGroupId, name: "Extension gate" });
  const legacyPageIdentity = clone(conformBase);
  const canonicalPageId = legacyPageIdentity.pageOrder[0];
  const legacyPage = legacyPageIdentity.pages[canonicalPageId];
  delete legacyPageIdentity.pages[canonicalPageId];
  legacyPage.id = "legacy-nanoid-page";
  legacyPageIdentity.pages[legacyPage.id] = legacyPage;
  legacyPageIdentity.pageOrder = [legacyPage.id];
  runConformance("reject-nanoid-page-identity", legacyPageIdentity, { kind: "rename", shapeId: nestedGroupId, name: "Identity gate" });
  const noncanonicalDocumentIdentity = clone(conformBase);
  noncanonicalDocumentIdentity.documentId = "AB111111-1111-4111-8111-111111111111";
  runConformance("reject-noncanonical-document-identity", noncanonicalDocumentIdentity,
    { kind: "rename", shapeId: nestedGroupId, name: "Identity case gate" });
  const legacyCase = cases.get("retain-vector-network-compatibility");
  assert(legacyCase?.classification === "LEGACY" && legacyCase.expected === "compatibility-retained",
    "Vector Network creation must remain on the A10.1 compatibility path");
  const legacyVector = Object.assign(shape(805, "vector-network", null, [], 0), {
    vnAnchors: [{ id: uuid(806), x: 10, y: 12, handleIn: null, handleOut: null }],
    vnSegments: [{ id: uuid(807), start: uuid(806), end: uuid(806), kind: "line" }],
    vnRegions: [{ id: uuid(808), fillRule: "nonzero", segments: [uuid(807)] }],
  });
  const legacyBefore = JSON.stringify(conformBase);
  const legacySnapshot: HierarchyDocumentSnapshot = {
    pages: conformBase.pages,
    currentPageId: conformBase.pageOrder[0],
    shapes: conformBase.shapes,
    currentPageShapes: Object.values(conformBase.shapes),
  };
  const legacyPlan = planHierarchyOperation(legacySnapshot, { kind: "compat-add-shape", shape: legacyVector });
  assert(legacyPlan.ok, `legacy Vector Network compatibility operation failed: ${JSON.stringify(legacyPlan)}`);
  assert(JSON.stringify(legacyPlan.shapes[legacyVector.id]) === JSON.stringify(legacyVector),
    "legacy Vector Network payload changed across the compatibility plan");
  assert(legacyPlan.renderingDiagnostic?.code === "LOGOS_SCENE_UNSUPPORTED_SHAPE",
    "legacy Vector Network did not retain its explicit renderer limitation");
  assert(JSON.stringify(conformBase) === legacyBefore, "legacy compatibility planning mutated its source document");
  conformanceResults.push({ name: "retain-vector-network-compatibility", outcome: "legacy-retained",
    diagnostic: legacyPlan.renderingDiagnostic.code });
  assert(rustEvaluationCalls === 8, `eligible operations were evaluated an unexpected number of times: ${rustEvaluationCalls}`);
  assert(JSON.stringify(useDocumentStore.getState()) === stateBefore, "conformance proposals reached the production document store");
  assert(JSON.stringify(useSelectionStore.getState().selectedIds) === selectionBefore,
    "read-only conformance changed production selection state");

  const memory: number[] = [];
  const measurements = [];
  for (const count of [4, 100, 1_000, 10_000]) {
    const large = fixture(count);
    const validator = { validateDocumentJson: validate_document_json };
    const warmup = validateReactSnapshot(large, validator, () => wasmExports.memory.buffer.byteLength);
    assert(warmup.ok, `${count}-object warmup failed validation`);
    const samples: Array<{ mappingMs: number; geometryConversionMs: number; serializationMs: number;
      wasmValidationBoundaryMs: number; rustResponseParseMs: number; resultMaterializationMs: number; totalMs: number;
      wasmLinearMemoryReservedBytes: number | null; jsHeapUsedBytes: number | null }> = [];
    for (let sample = 0; sample < 5; sample += 1) {
      const start = performance.now();
      const measured = validateReactSnapshot(large, validator, () => wasmExports.memory.buffer.byteLength);
      const totalMs = performance.now() - start;
      assert(measured.ok, `${count}-object fixture failed validation`);
      samples.push({ mappingMs: measured.timingsMs.mapping,
        geometryConversionMs: measured.timingsMs.geometryConversion,
        serializationMs: measured.timingsMs.serialization,
        wasmValidationBoundaryMs: measured.timingsMs.wasmValidationBoundary,
        rustResponseParseMs: measured.timingsMs.rustResponseParse,
        resultMaterializationMs: measured.timingsMs.resultMaterialization,
        totalMs,
        wasmLinearMemoryReservedBytes: measured.wasmMemoryBytes, jsHeapUsedBytes: heapMemory() });
      if (measured.wasmMemoryBytes !== null) memory.push(measured.wasmMemoryBytes);
    }
    measurements.push({
      objects: count, warmupRuns: 1, measuredRuns: samples.length,
      stagesMs: Object.fromEntries([
        ["mapping", samples.map((sample) => sample.mappingMs)],
        ["geometryConversion", samples.map((sample) => sample.geometryConversionMs)],
        ["serialization", samples.map((sample) => sample.serializationMs)],
        ["wasmValidationBoundary", samples.map((sample) => sample.wasmValidationBoundaryMs)],
        ["rustResponseParse", samples.map((sample) => sample.rustResponseParseMs)],
        ["resultMaterialization", samples.map((sample) => sample.resultMaterializationMs)],
      ].map(([stage, values]) => [stage, {
        p50: percentile(values as number[], 0.5), p95: percentile(values as number[], 0.95),
      }])),
      totalMs: { p50: percentile(samples.map((sample) => sample.totalMs), 0.5),
        p95: percentile(samples.map((sample) => sample.totalMs), 0.95) },
      wasmLinearMemoryReservedBytes: samples[samples.length - 1]?.wasmLinearMemoryReservedBytes ?? null,
      jsHeapUsedBytes: samples[samples.length - 1]?.jsHeapUsedBytes ?? null,
      samples,
    });
  }
  for (let cycle = 0; cycle < 5; cycle += 1) {
    const container = document.createElement("div");
    document.body.append(container);
    let repeated: ReturnType<typeof validateReactSnapshot> | undefined;
    function AdapterLifecycleProbe(): null {
      useEffect(() => {
        repeated = validateReactSnapshot(fixture(100), { validateDocumentJson: validate_document_json });
      }, []);
      return null;
    }
    const root = createRoot(container);
    root.render(createElement(AdapterLifecycleProbe));
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert(repeated?.ok, "React mount validation did not complete successfully");
    root.unmount();
    container.remove();
  }
  assert(storageWrites.length === 0, `unexpected localStorage writes: ${storageWrites.join(",")}`);
  assert(databaseOpens.length === 0, `unexpected IndexedDB open: ${databaseOpens.join(",")}`);
  assert(JSON.stringify(Object.keys(localStorage).sort()) === localStorageBefore, "localStorage contents changed");
  assert(JSON.stringify(useDocumentStore.getState()) === stateBefore, "Zustand state changed after tests");

  const evidence = {
    status: "PASS",
    browser: navigator.userAgent,
    viewport: { width: innerWidth, height: innerHeight, devicePixelRatio },
    adapterVersion: valid.adapterVersion,
    actualZustandSnapshot: { adapterAttempted: true, accepted: true, exactRoundTrip: true },
    supportedFixture: { shapes: Object.keys(snapshot.shapes).length, exactRoundTrip: true },
    invalidParentRustDiagnostic: invalid.diagnostics.map((item) => item.code),
    rejectedValidationRuns: { count: rejectedValidationMs.length,
      totalMs: { p50: percentile(rejectedValidationMs, 0.5), p95: percentile(rejectedValidationMs, 0.95) } },
    unsupportedFieldIssue: unsupportedResult.issues.map((item) => item.code),
    translatedParentCoordinateRoundTrip: coordinateResult.ok,
    rotatedParentCoordinateIssue: rotatedCoordinateResult.ok ? [] : rotatedCoordinateResult.issues.map((item) => item.code),
    operationConformance: { fixtureVersion: operationFixture.version, cases: conformanceResults,
      fullSemanticProjectionCompared: conformanceResults.filter((item) => item.outcome === "equivalent").length,
      rustEvaluationCalls, productionStateCommitted: false, productionSelectionChanged: false },
    storeUnchanged: true,
    localStorageWrites: storageWrites.length,
    indexedDbOpens: databaseOpens.length,
    lifecycleCycles: 5,
    lifecycleObservedJsHeapBytes: { before: heapBeforeLifecycle, after: heapAfterLifecycle },
    measurements,
    wasmMemoryPageBytesObserved: [...new Set(memory)],
    warning: "WASM linear-memory reservation is not retained-resource measurement; no leak-free claim is made.",
  };
  resultNode.dataset.status = "passed";
  resultNode.textContent = JSON.stringify(evidence, null, 2);
  document.title = "G1-B3 PASS";
}

run().catch((error: unknown) => {
  resultNode.dataset.status = "failed";
  resultNode.textContent = JSON.stringify({ status: "FAIL", message: String(error),
    stack: error instanceof Error ? error.stack : null }, null, 2);
  document.title = "G1-B3 FAIL";
});
