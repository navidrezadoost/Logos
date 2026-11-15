import initRust, { evaluate_document_operation_json, validate_document_json } from "../document-core/rust-wasm/logos_types";
import supportedFixture from "../document-core/fixtures/react-supported-v1.json";
import corpus from "../document-core/fixtures/g1-b9-3-compatibility-v1.json";
import { analyzeReactSnapshot } from "../document-core/compatibility-analyzer";
import { evaluateHierarchyOperationConformance } from "../document-core/operation-conformance";
import { useDocumentStore } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import type { ReactDocumentSnapshot } from "../document-core/react-adapter";
import type { InstanceMeta, Shape } from "../types/shapes";

const output = document.querySelector<HTMLPreElement>("#result")!;
const uuid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const freeze = <T>(value: T): T => {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    Object.freeze(value);
    Object.values(value as Record<string, unknown>).forEach(freeze);
  }
  return value;
};
const percentile = (values: number[], p: number) => [...values].sort((a, b) => a - b)[Math.max(0, Math.ceil(values.length * p) - 1)];

function makeCorpusCase(id: string): ReactDocumentSnapshot {
  const value = clone(supportedFixture) as unknown as ReactDocumentSnapshot;
  const [pageId] = value.pageOrder;
  const [frameId] = value.pages[pageId].rootShapeIds;
  const frame = value.shapes[frameId];
  const shape = Object.values(value.shapes).find((item) => item.type === "rect")!;
  switch (id) {
    case "canonical-nested": break;
    case "legacy-nanoid-page": {
      const page = value.pages[pageId];
      delete value.pages[pageId];
      value.pages["legacy-page-nanoid"] = { ...page, id: "legacy-page-nanoid" };
      value.pageOrder = ["legacy-page-nanoid"];
      break;
    }
    case "multiple-pages": {
      const second = uuid(930);
      value.pages[second] = { id: second, name: "Synthetic second page", rootShapeIds: [] };
      value.pageOrder.push(second);
      break;
    }
    case "vector-network":
      shape.type = "vector-network";
      Object.assign(shape, { vnAnchors: [], vnSegments: [], vnRegions: [] });
      break;
    case "component-instance":
      (shape as Shape & { instanceMeta: InstanceMeta }).instanceMeta = {
        componentId: uuid(931), variantProperties: {}, overrides: {},
      };
      break;
    case "multiple-fills":
      shape.fills.push({ type: "gradient" } as unknown as typeof shape.fills[number]);
      break;
    case "solid-multiple-fills":
      shape.fills.push({ type: "solid", color: "#ffffff", opacity: 1 });
      break;
    case "token-extension":
      (shape as Shape & { tokenBinding: unknown }).tokenBinding = { tokenId: uuid(932), property: "fill" };
      break;
    case "plugin-extension":
      (shape as Shape & { pluginPayload: unknown }).pluginPayload = { owner: "synthetic-plugin", data: [1, 2] };
      break;
    case "xd-converted":
      (shape as Shape & { sourceProvenance: unknown }).sourceProvenance = { format: "xd", sourceId: "synthetic-xd-id" };
      break;
    case "sketch-converted":
      (shape as Shape & { sourceProvenance: unknown }).sourceProvenance = { format: "sketch", sourceId: "synthetic-sketch-id" };
      break;
    case "invalid-hierarchy": {
      const group = Object.values(value.shapes).find((item) => item.type === "group")!;
      group.children = [];
      break;
    }
    case "ambiguous-page-ownership": {
      const second = uuid(933);
      value.pages[second] = { id: second, name: "Synthetic conflicting page", rootShapeIds: [frameId] };
      value.pageOrder.push(second);
      break;
    }
    case "transformed-ancestry":
      frame.rotation = 15;
      break;
    default: throw new Error(`unknown corpus case ${id}`);
  }
  return value;
}

function makeNestedBenchmark(depth: number): { snapshot: ReactDocumentSnapshot; shapeCount: number; targetId: string } {
  const pageId = uuid(940);
  const rootId = uuid(10000);
  const shapes: Record<string, Shape> = {};
  let nextId = 10000;
  const add = (id: string, type: Shape["type"], parentId: string | null, children: string[]): void => {
    shapes[id] = { id, type, name: `${type}-${id}`, bounds: { x: 0, y: 0, w: 400, h: 400 },
      transform: [1, 0, 0, 1, 0, 0], rotation: 0,
      fills: type === "group" ? [] : [{ type: "solid", color: "#336699", opacity: 1 }],
      opacity: 1, hidden: false, locked: false, parentId, children };
  };
  // Build a regular 4-ary tree while assigning each child its parent before descent.
  const build = (id: string, parentId: string | null, level: number): void => {
    const type = parentId === null ? "frame" : level === depth ? "rect" : "group";
    if (type === "rect") { add(id, type, parentId, []); return; }
    const childIds = Array.from({ length: 4 }, () => uuid(++nextId));
    add(id, type, parentId, childIds);
    for (const child of childIds) build(child, id, level + 1);
  };
  build(rootId, null, 0);
  const targetId = Object.values(shapes).find((shape) => shape.type === "rect")!.id;
  return { snapshot: { documentId: uuid(941), pages: { [pageId]: { id: pageId, name: "Nested benchmark", rootShapeIds: [rootId] } },
    pageOrder: [pageId], shapes }, shapeCount: Object.keys(shapes).length, targetId };
}

async function run(): Promise<void> {
  await initRust();
  const documentBefore = JSON.stringify(useDocumentStore.getState());
  const selectionBefore = JSON.stringify(useSelectionStore.getState().selectedIds);
  const cases = corpus.cases.map((item) => {
    const snapshot = freeze(makeCorpusCase(item.id));
    const before = JSON.stringify(snapshot);
    const analysis = analyzeReactSnapshot(snapshot, { validateDocumentJson: validate_document_json });
    if (JSON.stringify(snapshot) !== before) throw new Error(`${item.id}: input mutated`);
    if (analysis.classification !== item.expected) {
      throw new Error(`${item.id}: expected ${item.expected}, received ${analysis.classification} ${JSON.stringify(analysis)}`);
    }
    if (item.expected === "LOSSLESS_RUST_CANDIDATE" &&
      (analysis.checks.nativeRustValidation !== "PASS" || analysis.checks.semanticEquality !== "PASS" ||
       analysis.checks.identityAndMembershipStable !== "PASS")) throw new Error(`${item.id}: losslessness checks incomplete`);
    return { id: item.id, expected: item.expected, actual: analysis.classification,
      adapterIssues: analysis.adapterIssues.map((issue) => issue.code),
      rustDiagnostics: analysis.rustDiagnostics.map((diagnostic) => diagnostic.code), checks: analysis.checks };
  });
  const benchmarks = [];
  for (const workload of [{ name: "small", depth: 2 }, { name: "medium", depth: 3 }, { name: "large", depth: 4 }]) {
    const { snapshot, shapeCount, targetId } = makeNestedBenchmark(workload.depth);
    const before = JSON.stringify(snapshot);
    const operation = { kind: "rename" as const, shapeId: targetId, name: "Renamed benchmark target" };
    const evaluator = { evaluateDocumentOperationJson: evaluate_document_operation_json };
    for (let warmup = 0; warmup < 5; warmup += 1) {
      const result = evaluateHierarchyOperationConformance(snapshot, operation, evaluator);
      if (!result.ok || !result.semanticEquivalent) throw new Error(`${workload.name} warmup failed: ${JSON.stringify(result)}`);
    }
    const samples = [];
    for (let index = 0; index < 30; index += 1) {
      const result = evaluateHierarchyOperationConformance(snapshot, operation, evaluator);
      if (!result.ok || !result.semanticEquivalent) throw new Error(`${workload.name} run ${index} failed: ${JSON.stringify(result)}`);
      samples.push(result.timingsMs);
    }
    if (JSON.stringify(snapshot) !== before) throw new Error(`${workload.name} benchmark mutated its fixture`);
    const summarize = (values: number[]) => ({ p50Ms: percentile(values, 0.5), p95Ms: percentile(values, 0.95), maxMs: Math.max(...values) });
    benchmarks.push({ name: workload.name, shapes: shapeCount, nestingDepth: workload.depth + 1,
      operation: "rename one leaf", warmups: 5, measuredRuns: samples.length,
      stages: {
        reactPlanning: summarize(samples.map((sample) => sample.reactPlanningMs)),
        adapterConversionAndOperationMapping: summarize(samples.map((sample) => sample.candidateConversionMs)),
        rustWasmEvaluation: summarize(samples.map((sample) => sample.rustOperationEvaluationMs)),
        semanticProjection: summarize(samples.map((sample) => sample.reactProjectionMs)),
        endToEndPlannerToProjection: summarize(samples.map((sample) => sample.totalMs)),
      }, samples });
  }
  if (JSON.stringify(useDocumentStore.getState()) !== documentBefore) throw new Error("document store mutated");
  if (JSON.stringify(useSelectionStore.getState().selectedIds) !== selectionBefore) throw new Error("selection mutated");
  const counts = Object.fromEntries([...new Set(cases.map((item) => item.actual))].map((key) =>
    [key, cases.filter((item) => item.actual === key).length]));
  output.dataset.status = "passed";
  output.textContent = JSON.stringify({ status: "PASS", corpusVersion: corpus.version,
    provenance: corpus.provenance, populationClaim: corpus.populationClaim,
    browser: { userAgent: navigator.userAgent, platform: navigator.platform,
      hardwareConcurrency: navigator.hardwareConcurrency, viewport: { width: innerWidth, height: innerHeight },
      devicePixelRatio }, fixtureCount: cases.length, counts, cases,
    benchmarkMethod: "five warmups plus thirty sequential samples; synthetic 4-ary nested trees; one leaf rename; performance.now; renderer and Zustand commit excluded",
    benchmarks,
    documentStoreUnchanged: true, selectionUnchanged: true }, null, 2);
}

run().catch((error: unknown) => {
  output.dataset.status = "failed";
  output.textContent = JSON.stringify({ status: "FAIL", message: String(error),
    stack: error instanceof Error ? error.stack : null }, null, 2);
});
