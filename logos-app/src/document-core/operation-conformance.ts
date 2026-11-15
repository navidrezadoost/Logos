import { planHierarchyOperation, type HierarchyDocumentSnapshot, type HierarchyOperation } from "../stores/hierarchyOperations";
import { fromRustDocument, toRustCandidate, type AdapterIssue, type ReactDocumentSnapshot, type RustDiagnostic } from "./react-adapter";

export interface RustOperationEvaluator {
  evaluateDocumentOperationJson(documentJson: string, operationJson: string): string;
}

export interface OperationConformanceTimings {
  reactPlanningMs: number;
  candidateConversionMs: number;
  rustOperationEvaluationMs: number;
  reactProjectionMs: number;
  totalMs: number;
}

export type OperationConformanceResult =
  | { ok: false; phase: "react-plan" | "adapter" | "rust" | "projection"; code: string; issues?: AdapterIssue[]; diagnostics?: RustDiagnostic[] }
  | { ok: true; reactNext: ReactDocumentSnapshot; rustNext: ReactDocumentSnapshot; semanticEquivalent: boolean;
      operationJson: string; timingsMs: OperationConformanceTimings };

export type PreplannedReactOperation = {
  pageId: string;
  result: ReturnType<typeof planHierarchyOperation>;
  planningMs: number;
};

function currentPageSnapshot(snapshot: ReactDocumentSnapshot, requestedPageId = snapshot.pageOrder[0]): HierarchyDocumentSnapshot {
  const pageId = requestedPageId;
  const page = snapshot.pages[pageId];
  const reachable = new Set<string>();
  const pending = [...(page?.rootShapeIds ?? [])];
  while (pending.length) {
    const id = pending.pop()!;
    if (reachable.has(id)) continue;
    reachable.add(id);
    pending.push(...(snapshot.shapes[id]?.children ?? []));
  }
  return { pages: snapshot.pages, currentPageId: pageId, shapes: snapshot.shapes,
    currentPageShapes: [...reachable].flatMap((id) => snapshot.shapes[id] ? [snapshot.shapes[id]] : []) };
}

function projectPlan(snapshot: ReactDocumentSnapshot, plan: ReturnType<typeof planHierarchyOperation>): ReactDocumentSnapshot | null {
  if (!plan.ok) return null;
  const pageId = plan.pageId;
  const page = plan.pages[pageId];
  if (!page) return null;
  const reachable = new Set<string>();
  const pending = [...page.rootShapeIds];
  while (pending.length) {
    const id = pending.pop()!;
    if (reachable.has(id)) continue;
    reachable.add(id);
    pending.push(...(plan.shapes[id]?.children ?? []));
  }
  return { documentId: snapshot.documentId, pageOrder: [pageId], pages: { [pageId]: page },
    shapes: Object.fromEntries([...reachable].flatMap((id) => plan.shapes[id] ? [[id, plan.shapes[id]]] : [])) };
}

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical((value as Record<string, unknown>)[key])]));
}

function toRustOperation(
  before: ReactDocumentSnapshot,
  after: ReactDocumentSnapshot,
  operation: Exclude<HierarchyOperation, { kind: `compat-${string}` }>,
  pageId: string,
): Record<string, unknown> {
  const source = before.shapes;
  const next = after.shapes;
  const createRecord = (id: string): Record<string, unknown> => {
    const converted = toRustCandidate({ ...after, shapes: next });
    if (!converted.ok) throw new Error(`planned-document-not-adaptable:${converted.issues[0]?.code ?? "unknown"}`);
    const records = converted.candidate.shapes as Array<Record<string, unknown>>;
    const record = records.find((item) => item.id === id);
    if (!record) throw new Error("planned-shape-record-missing");
    return record;
  };
  switch (operation.kind) {
    case "create-root": {
      const id = operation.shape.id;
      return { kind: "createShape", record: createRecord(id), parentId: null, index: after.pages[pageId].rootShapeIds.indexOf(id) };
    }
    case "insert-child": {
      const id = operation.shape.id;
      return { kind: "createShape", record: createRecord(id), parentId: operation.parentId,
        index: after.shapes[operation.parentId].children.indexOf(id) };
    }
    case "rename":
      return { kind: "updateShape", shapeId: operation.shapeId, expectedParentId: source[operation.shapeId].parentId,
        name: after.shapes[operation.shapeId].name, selrect: null, transform: null, opacity: null };
    case "delete":
      return { kind: "deleteSubtree", shapeId: operation.shapeId, expectedParentId: source[operation.shapeId].parentId };
    case "reorder": {
      const shape = source[operation.shapeId];
      const beforeIds = shape.parentId ? source[shape.parentId].children : before.pages[pageId].rootShapeIds;
      const afterIds = shape.parentId ? next[shape.parentId].children : after.pages[pageId].rootShapeIds;
      return { kind: "reorderChildren", pageId, parentId: shape.parentId, mutationId: shape.id,
        expectedIds: beforeIds, orderedIds: afterIds };
    }
    case "reorder-root-list":
      return { kind: "reorderChildren", pageId, parentId: null,
        mutationId: null, expectedIds: before.pages[pageId].rootShapeIds, orderedIds: after.pages[pageId].rootShapeIds };
    case "reparent": {
      const shape = source[operation.shapeId];
      const moved = { kind: "reparentShape", shapeId: shape.id, expectedParentId: shape.parentId,
        newParentId: operation.parentId, newPageId: pageId,
        index: operation.parentId ? after.shapes[operation.parentId].children.indexOf(shape.id) : after.pages[pageId].rootShapeIds.indexOf(shape.id) };
      const world = shape.bounds;
      const target = operation.parentId ? after.shapes[operation.parentId] : null;
      const local = { x: world.x - (target?.bounds.x ?? 0), y: world.y - (target?.bounds.y ?? 0), width: world.w, height: world.h };
      // Rust ReparentShape preserves parent-local coordinates. React stores page
      // coordinates, so this explicit follow-up mapping preserves React's world position.
      return { kind: "sequence", operations: [moved, {
        kind: "updateShape", shapeId: shape.id, expectedParentId: operation.parentId,
        name: null, selrect: local, transform: null, opacity: null,
      }] };
    }
  }
}

/** Evaluate a strict React hierarchy proposal in Rust and compare the full React semantic projection.
 * This function never writes Zustand state or commits a Rust result.
 */
export function evaluateHierarchyOperationConformance(
  snapshot: ReactDocumentSnapshot,
  operation: Exclude<HierarchyOperation, { kind: `compat-${string}` }>,
  rust: RustOperationEvaluator,
  preplanned?: PreplannedReactOperation,
): OperationConformanceResult {
  const totalStart = performance.now();
  const reactStart = performance.now();
  const pageSnapshot = currentPageSnapshot(snapshot, preplanned?.pageId);
  const reactPlan = preplanned?.pageId === pageSnapshot.currentPageId
    ? preplanned.result
    : planHierarchyOperation(pageSnapshot, operation);
  if (!reactPlan.ok) return { ok: false, phase: "react-plan", code: reactPlan.diagnostic.code };
  const reactPlanningMs = preplanned?.pageId === pageSnapshot.currentPageId
    ? preplanned.planningMs
    : performance.now() - reactStart;
  const reactNext = projectPlan(snapshot, reactPlan);
  if (!reactNext) return { ok: false, phase: "projection", code: "PLANNED_DOCUMENT_PROJECTION_FAILED" };
  const conversionStart = performance.now();
  const before = toRustCandidate(snapshot);
  const after = toRustCandidate(reactNext);
  if (!before.ok || !after.ok) return { ok: false, phase: "adapter", code: "UNSUPPORTED_DOCUMENT",
    issues: before.ok ? (after.ok ? [] : after.issues) : before.issues };
  let operationJson: string;
  try {
    operationJson = JSON.stringify(toRustOperation(snapshot, reactNext, operation, reactNext.pageOrder[0]));
  } catch (error) {
    return { ok: false, phase: "adapter", code: error instanceof Error ? error.message : "OPERATION_MAPPING_FAILED" };
  }
  const candidateConversionMs = performance.now() - conversionStart;
  const rustStart = performance.now();
  let raw: string;
  try {
    raw = rust.evaluateDocumentOperationJson(JSON.stringify(before.candidate), operationJson);
  } catch {
    return { ok: false, phase: "rust", code: "INVALID_RUST_RESPONSE" };
  }
  let response: { ok?: boolean; document?: Record<string, unknown>; diagnostics?: RustDiagnostic[] };
  try { response = JSON.parse(raw) as typeof response; }
  catch { return { ok: false, phase: "rust", code: "INVALID_RUST_RESPONSE" }; }
  const rustOperationEvaluationMs = performance.now() - rustStart;
  if (response.ok !== true || !response.document) return {
    ok: false, phase: "rust", code: "RUST_OPERATION_REJECTED", diagnostics: response.diagnostics ?? [],
  };
  // Use the existing adapter reverse projection; never normalize a mismatch
  // away. Unsupported data remains in the untouched React snapshot.
  let projected: ReactDocumentSnapshot;
  const projectionStart = performance.now();
  try { projected = fromRustDocument(response.document); }
  catch { return { ok: false, phase: "projection", code: "RUST_RESULT_PROJECTION_FAILED" }; }
  return { ok: true, reactNext, rustNext: projected,
    semanticEquivalent: JSON.stringify(canonical(reactNext)) === JSON.stringify(canonical(projected)), operationJson,
    timingsMs: { reactPlanningMs, candidateConversionMs, rustOperationEvaluationMs,
      reactProjectionMs: performance.now() - projectionStart,
      totalMs: performance.now() - totalStart + (preplanned ? reactPlanningMs : 0) } };
}
