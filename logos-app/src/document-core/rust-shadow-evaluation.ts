import {
  evaluateHierarchyOperationConformance,
  type OperationConformanceResult,
  type OperationConformanceTimings,
  type RustOperationEvaluator,
} from "./operation-conformance";
import type { ReactDocumentSnapshot } from "./react-adapter";
import type { AdapterIssue, RustDiagnostic } from "./react-adapter";
import type { HierarchyOperation, HierarchyOperationResult } from "../stores/hierarchyOperations";

declare const __LOGOS_G1_B9_SHADOW_BUILD__: boolean | undefined;

export type RustShadowCategory =
  | "ELIGIBLE_EQUIVALENT"
  | "ELIGIBLE_MISMATCH"
  | "RUST_REJECTED"
  | "REACT_REJECTED"
  | "ADAPTER_UNSUPPORTED"
  | "LEGACY_COMPATIBILITY"
  | "SHADOW_EVALUATION_FAILED";

export interface RustShadowEvidence {
  category: RustShadowCategory;
  operation: string;
  diagnosticCode?: string;
  rustDiagnostics?: RustDiagnostic[];
  adapterIssues?: AdapterIssue[];
  semanticEquivalent?: boolean;
  timingsMs?: OperationConformanceTimings;
  jsHeapUsedBytes?: number | null;
  wasmReservedBytes?: number | null;
  reactStateUnchanged?: boolean;
  selectionUnchanged?: boolean;
}

export interface RustShadowRuntime {
  documentId: string;
  evaluator?: RustOperationEvaluator;
  report: (evidence: RustShadowEvidence) => void;
  discard?: (reason: "stale-snapshot" | "runtime-disposed") => void;
}

let runtime: RustShadowRuntime | null = null;
let generation = 0;

/** This setter only exists for the dedicated conformance harness. */
export function installRustShadowRuntime(candidate: RustShadowRuntime): () => void {
  runtime = candidate;
  const installedGeneration = ++generation;
  return () => {
    if (generation === installedGeneration) {
      runtime = null;
      generation += 1;
    }
  };
}

function enabled(): boolean {
  return typeof __LOGOS_G1_B9_SHADOW_BUILD__ !== "undefined" &&
    __LOGOS_G1_B9_SHADOW_BUILD__ === true &&
    (globalThis as typeof globalThis & { __LOGOS_G1_B9_SHADOW_OPT_IN__?: boolean }).__LOGOS_G1_B9_SHADOW_OPT_IN__ === true;
}

function report(evidence: RustShadowEvidence): void {
  try { runtime?.report(evidence); } catch { /* Evidence reporting cannot affect the editor. */ }
}

function reportDiscard(target: RustShadowRuntime, reason: "stale-snapshot" | "runtime-disposed"): void {
  try { target.discard?.(reason); } catch { /* Diagnostics cannot affect the editor. */ }
}

function failureCategory(result: Extract<OperationConformanceResult, { ok: false }>): RustShadowCategory {
  if (result.phase === "adapter") return "ADAPTER_UNSUPPORTED";
  if (result.phase === "rust") return result.code === "RUST_OPERATION_REJECTED" ? "RUST_REJECTED" : "SHADOW_EVALUATION_FAILED";
  if (result.phase === "react-plan") return "REACT_REJECTED";
  return "SHADOW_EVALUATION_FAILED";
}

/** Schedule a read-only candidate comparison after React's one and only commit. */
export function scheduleRustShadowEvaluation(
  snapshot: ReactDocumentSnapshot,
  operation: HierarchyOperation,
  plan: HierarchyOperationResult,
  planningMs: number,
  committedPages: object,
  committedShapes: object,
  getCurrent: () => { pages: object; shapes: object; currentPageId: string },
  getSelection?: () => readonly string[],
): void {
  if (!enabled()) return;
  if (operation.kind.startsWith("compat-")) {
    report({ category: "LEGACY_COMPATIBILITY", operation: operation.kind });
    return;
  }
  if (!plan.ok) {
    report({ category: "REACT_REJECTED", operation: operation.kind, diagnosticCode: plan.diagnostic.code });
    return;
  }
  const activeRuntime = runtime;
  if (!activeRuntime) {
    report({ category: "SHADOW_EVALUATION_FAILED", operation: operation.kind, diagnosticCode: "EVALUATOR_UNAVAILABLE" });
    return;
  }
  const activeGeneration = generation;
  const evaluator = activeRuntime.evaluator;
  if (!evaluator) {
    report({ category: "SHADOW_EVALUATION_FAILED", operation: operation.kind, diagnosticCode: "EVALUATOR_UNAVAILABLE" });
    return;
  }
  let captured: ReactDocumentSnapshot;
  let capturedOperation: HierarchyOperation;
  let expectedPlan: HierarchyOperationResult;
  try {
    captured = structuredClone({ ...snapshot, documentId: activeRuntime.documentId });
    capturedOperation = structuredClone(operation);
    expectedPlan = structuredClone(plan);
  } catch {
    report({ category: "SHADOW_EVALUATION_FAILED", operation: operation.kind, diagnosticCode: "SNAPSHOT_CAPTURE_FAILED" });
    return;
  }
  const pageId = plan.pageId;
  setTimeout(() => {
    if (generation !== activeGeneration || runtime !== activeRuntime) {
      reportDiscard(activeRuntime, "runtime-disposed");
      return;
    }
    const current = getCurrent();
    if (current.pages !== committedPages || current.shapes !== committedShapes || current.currentPageId !== pageId) {
      reportDiscard(activeRuntime, "stale-snapshot");
      return;
    }
    const beforeEvaluation = getCurrent();
    const selectionBefore = getSelection?.();
    try {
      const result = evaluateHierarchyOperationConformance(
        captured,
        capturedOperation as Exclude<HierarchyOperation, { kind: `compat-${string}` }>,
        evaluator,
        { pageId, result: expectedPlan, planningMs },
      );
      const afterEvaluation = getCurrent();
      const selectionAfter = getSelection?.();
      const stateUnchanged = beforeEvaluation.pages === afterEvaluation.pages &&
        beforeEvaluation.shapes === afterEvaluation.shapes && beforeEvaluation.currentPageId === afterEvaluation.currentPageId;
      const selectionUnchanged = selectionBefore === undefined || selectionBefore === selectionAfter;
      if (!result.ok) {
        report({ category: failureCategory(result), operation: operation.kind, diagnosticCode: result.code,
          reactStateUnchanged: stateUnchanged, selectionUnchanged,
          ...(result.diagnostics ? { rustDiagnostics: result.diagnostics } : {}),
          ...(result.issues ? { adapterIssues: result.issues } : {}) });
      } else {
        report({ category: result.semanticEquivalent ? "ELIGIBLE_EQUIVALENT" : "ELIGIBLE_MISMATCH",
          operation: operation.kind, semanticEquivalent: result.semanticEquivalent, timingsMs: result.timingsMs,
          reactStateUnchanged: stateUnchanged, selectionUnchanged });
      }
    } catch {
      report({ category: "SHADOW_EVALUATION_FAILED", operation: operation.kind, diagnosticCode: "SHADOW_EVALUATOR_EXCEPTION" });
    }
  }, 0);
}
