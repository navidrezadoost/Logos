import { createElement, useEffect } from "react";
import { createRoot } from "react-dom/client";
import initRust, { validate_document_json } from "../document-core/rust-wasm/logos_types";
import supportedFixture from "../document-core/fixtures/react-supported-v1.json";
import { validateReactSnapshot, type ReactDocumentSnapshot } from "../document-core/react-adapter";
import { useDocumentStore } from "../stores/documentStore";
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
  const localStorageBefore = JSON.stringify(Object.keys(localStorage).sort());
  const liveState = useDocumentStore.getState();
  const liveSnapshot: ReactDocumentSnapshot = {
    documentId: uuid(1), pages: liveState.pages, pageOrder: liveState.pageOrder, shapes: liveState.shapes,
  };
  const liveResult = validateReactSnapshot(liveSnapshot, { validateDocumentJson: validate_document_json });
  assert(!liveResult.ok && liveResult.issues.some((item) => item.code === "UNSUPPORTED_PAGE_ID"),
    "real initial Zustand page nanoid was unexpectedly remapped or accepted");
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
  for (let cycle = 0; cycle < 5; cycle += 1) {
    const rejected = validateReactSnapshot(invalidParent, { validateDocumentJson: validate_document_json });
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
  const translatedGroup = Object.values(translatedParent.shapes).find((item) => item.type === "group")!;
  translatedGroup.bounds.x = 10;
  const coordinateResult = validateReactSnapshot(translatedParent, { validateDocumentJson: validate_document_json });
  assert(!coordinateResult.ok && coordinateResult.issues.some((item) => item.code === "UNVERIFIED_COORDINATE_SPACE"),
    "canvas-local bounds under a translated parent were reinterpreted as parent-local");

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

  const memory: number[] = [];
  const measurements = [];
  for (const count of [4, 100, 1_000, 10_000]) {
    const large = fixture(count);
    const validator = { validateDocumentJson: validate_document_json };
    const warmup = validateReactSnapshot(large, validator, () => wasmExports.memory.buffer.byteLength);
    assert(warmup.ok, `${count}-object warmup failed validation`);
    const samples: Array<{ serializeMs: number; rustValidationMs: number; totalMs: number;
      wasmLinearMemoryReservedBytes: number | null; jsHeapUsedBytes: number | null }> = [];
    for (let sample = 0; sample < 5; sample += 1) {
      const start = performance.now();
      const measured = validateReactSnapshot(large, validator, () => wasmExports.memory.buffer.byteLength);
      const totalMs = performance.now() - start;
      assert(measured.ok, `${count}-object fixture failed validation`);
      samples.push({ serializeMs: measured.timingsMs.serialize,
        rustValidationMs: measured.timingsMs.rustValidation, totalMs,
        wasmLinearMemoryReservedBytes: measured.wasmMemoryBytes, jsHeapUsedBytes: heapMemory() });
      if (measured.wasmMemoryBytes !== null) memory.push(measured.wasmMemoryBytes);
    }
    measurements.push({
      objects: count, warmupRuns: 1, measuredRuns: samples.length,
      serializeMs: { p50: percentile(samples.map((sample) => sample.serializeMs), 0.5),
        p95: percentile(samples.map((sample) => sample.serializeMs), 0.95) },
      rustValidationMs: { p50: percentile(samples.map((sample) => sample.rustValidationMs), 0.5),
        p95: percentile(samples.map((sample) => sample.rustValidationMs), 0.95) },
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
    actualZustandSnapshot: { adapterAttempted: true, accepted: false,
      issue: liveResult.ok ? null : liveResult.issues[0]?.code },
    supportedFixture: { shapes: Object.keys(snapshot.shapes).length, exactRoundTrip: true },
    invalidParentRustDiagnostic: invalid.diagnostics.map((item) => item.code),
    unsupportedFieldIssue: unsupportedResult.issues.map((item) => item.code),
    translatedParentCoordinateIssue: coordinateResult.issues.map((item) => item.code),
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
