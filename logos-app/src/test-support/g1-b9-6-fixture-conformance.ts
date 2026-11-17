import { evaluateHierarchyOperationConformance, type OperationConformanceResult, type RustOperationEvaluator } from "../document-core/operation-conformance";
import type { ReactDocumentSnapshot } from "../document-core/react-adapter";
import type { HierarchyOperation } from "../stores/hierarchyOperations";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
type SupportedOperation = Exclude<HierarchyOperation, { readonly kind: `compat-${string}` }>;
type FixtureProjection = Omit<ReactDocumentSnapshot, "documentId">;

/**
 * Attach a deterministic test-owned document UUID to a projection for the Rust
 * candidate adapter. This is deliberately separate from Zustand hydration:
 * the smoke fixture is not a persisted/offline document and must not claim
 * production identity eligibility.
 */
export function withFixtureDocumentIdentity(documentId: unknown, projection: FixtureProjection): ReactDocumentSnapshot {
  if (typeof documentId !== "string" || !UUID.test(documentId)) {
    throw new Error("G1B96_FIXTURE_DOCUMENT_ID_INVALID");
  }
  return { ...projection, documentId };
}

/** Evaluate every fixture operation in Rust and fail closed on any missing or mismatched candidate. */
export function evaluateFixtureOperation(
  documentId: unknown,
  projection: FixtureProjection,
  operation: SupportedOperation,
  rust: RustOperationEvaluator,
): Extract<OperationConformanceResult, { ok: true }> {
  const snapshot = withFixtureDocumentIdentity(documentId, projection);
  const result = evaluateHierarchyOperationConformance(snapshot, operation, rust);
  if (!result.ok) throw new Error(`G1B96_RUST_CANDIDATE_REJECTED:${result.phase}:${result.code}`);
  if (!result.semanticEquivalent) throw new Error("G1B96_RUST_CANDIDATE_MISMATCH");
  return result;
}
