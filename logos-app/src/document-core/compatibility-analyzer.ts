import {
  fromRustDocument,
  toRustCandidate,
  type AdapterIssue,
  type ReactDocumentSnapshot,
  type RustDiagnostic,
  type RustDocumentValidator,
} from "./react-adapter";

export type CompatibilityClassification =
  | "LOSSLESS_RUST_CANDIDATE"
  | "ADAPTER_INCOMPATIBLE"
  | "INVALID_SOURCE_DOCUMENT"
  | "AMBIGUOUS_OWNERSHIP"
  | "ANALYSIS_FAILED";

export interface CompatibilityAnalysis {
  analyzerVersion: "logos-react-rust-compatibility/v1";
  classification: CompatibilityClassification;
  adapterIssues: AdapterIssue[];
  rustDiagnostics: RustDiagnostic[];
  checks: {
    nativeRustValidation: "PASS" | "NOT_RUN" | "REJECTED";
    roundTripProjection: "PASS" | "NOT_RUN" | "FAILED";
    semanticEquality: "PASS" | "NOT_RUN" | "FAILED";
    identityAndMembershipStable: "PASS" | "NOT_RUN" | "FAILED";
    unknownFieldsPreserved: "NOT_APPLICABLE" | "NOT_RUN" | "FAILED";
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (!isRecord(value)) return value;
  return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
}

function semanticEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));
}

/** Detect only ambiguous ownership, not general graph validity. Rust remains the graph validator. */
function hasAmbiguousPageOwnership(input: unknown): boolean {
  if (!isRecord(input) || !isRecord(input.pages) || !isRecord(input.shapes) || !Array.isArray(input.pageOrder)) return false;
  const rootsByPage = new Map<string, Set<string>>();
  for (const pageId of input.pageOrder) {
    if (typeof pageId !== "string") continue;
    const page = input.pages[pageId];
    const roots = isRecord(page) && Array.isArray(page.rootShapeIds)
      ? page.rootShapeIds.filter((id): id is string => typeof id === "string") : [];
    rootsByPage.set(pageId, new Set(roots));
  }
  const owner = new Map<string, string>();
  for (const [pageId, roots] of rootsByPage) {
    const pending = [...roots];
    const visited = new Set<string>();
    while (pending.length) {
      const id = pending.pop()!;
      if (visited.has(id)) continue;
      visited.add(id);
      const previous = owner.get(id);
      if (previous && previous !== pageId) return true;
      owner.set(id, pageId);
      const shape = input.shapes[id];
      if (isRecord(shape) && Array.isArray(shape.children)) {
        pending.push(...shape.children.filter((child): child is string => typeof child === "string"));
      }
    }
  }
  for (const [shapeId, raw] of Object.entries(input.shapes)) {
    if (!isRecord(raw)) continue;
    const parentId = raw.parentId;
    if (typeof parentId !== "string") continue;
    const parentOwner = owner.get(parentId);
    const childOwner = owner.get(shapeId);
    if (parentOwner && childOwner && parentOwner !== childOwner) return true;
  }
  return false;
}

function hasSourceShapeDefect(issues: AdapterIssue[]): boolean {
  return issues.some(({ code, path }) => code === "INVALID_GEOMETRY" || code === "INVALID_TRANSFORM" ||
    (code === "INVALID_SNAPSHOT" && path !== "/pages"));
}

/**
 * Read-only classification against the existing React adapter and Rust validator.
 * No repair, coercion, persistence, state write, or identity generation occurs.
 */
export function analyzeReactSnapshot(
  input: unknown,
  rust: RustDocumentValidator,
): CompatibilityAnalysis {
  const base = {
    analyzerVersion: "logos-react-rust-compatibility/v1" as const,
    adapterIssues: [] as AdapterIssue[],
    rustDiagnostics: [] as RustDiagnostic[],
    checks: {
      nativeRustValidation: "NOT_RUN" as const,
      roundTripProjection: "NOT_RUN" as const,
      semanticEquality: "NOT_RUN" as const,
      identityAndMembershipStable: "NOT_RUN" as const,
      unknownFieldsPreserved: "NOT_APPLICABLE" as const,
    },
  };
  if (hasAmbiguousPageOwnership(input)) return { ...base, classification: "AMBIGUOUS_OWNERSHIP" };

  const converted = toRustCandidate(input);
  if (!converted.ok) {
    const sourceInvalid = hasSourceShapeDefect(converted.issues);
    return {
      ...base,
      classification: sourceInvalid ? "INVALID_SOURCE_DOCUMENT" : "ADAPTER_INCOMPATIBLE",
      adapterIssues: converted.issues,
    };
  }

  let response: unknown;
  try {
    response = JSON.parse(rust.validateDocumentJson(JSON.stringify(converted.candidate)));
  } catch {
    return {
      ...base,
      classification: "ANALYSIS_FAILED",
      rustDiagnostics: [{ code: "RUST_VALIDATION_RESPONSE_INVALID", path: "/" }],
      checks: { ...base.checks, nativeRustValidation: "REJECTED" },
    };
  }
  if (!isRecord(response) || typeof response.ok !== "boolean" || !Array.isArray(response.diagnostics)) {
    return {
      ...base,
      classification: "ANALYSIS_FAILED",
      rustDiagnostics: [{ code: "RUST_VALIDATION_RESPONSE_INVALID", path: "/" }],
      checks: { ...base.checks, nativeRustValidation: "REJECTED" },
    };
  }
  const diagnostics = response.diagnostics.filter(isRecord).map((item) => ({
    code: String(item.code ?? "UNKNOWN_RUST_DIAGNOSTIC"),
    path: String(item.path ?? "/"),
    ...(typeof item.objectId === "string" ? { objectId: item.objectId } : {}),
    ...(typeof item.relatedId === "string" ? { relatedId: item.relatedId } : {}),
  }));
  if (response.ok !== true || !isRecord(response.document)) return {
    ...base,
    classification: "INVALID_SOURCE_DOCUMENT",
    rustDiagnostics: diagnostics,
    checks: { ...base.checks, nativeRustValidation: "REJECTED" },
  };

  try {
    const roundTrip = fromRustDocument(response.document);
    const inputSnapshot = input as ReactDocumentSnapshot;
    const sameSemantics = semanticEqual(inputSnapshot, roundTrip);
    const sameIdentityMembership = inputSnapshot.documentId === roundTrip.documentId &&
      semanticEqual(inputSnapshot.pageOrder, roundTrip.pageOrder) &&
      semanticEqual(Object.keys(inputSnapshot.shapes).sort(), Object.keys(roundTrip.shapes).sort()) &&
      Object.entries(inputSnapshot.shapes).every(([id, shape]) =>
        shape.id === roundTrip.shapes[id]?.id && shape.parentId === roundTrip.shapes[id]?.parentId &&
        semanticEqual(shape.children, roundTrip.shapes[id]?.children));
    const checks = {
      nativeRustValidation: "PASS" as const,
      roundTripProjection: "PASS" as const,
      semanticEquality: sameSemantics ? "PASS" as const : "FAILED" as const,
      identityAndMembershipStable: sameIdentityMembership ? "PASS" as const : "FAILED" as const,
      unknownFieldsPreserved: "NOT_APPLICABLE" as const,
    };
    return {
      ...base,
      classification: sameSemantics && sameIdentityMembership
        ? "LOSSLESS_RUST_CANDIDATE" : "ADAPTER_INCOMPATIBLE",
      checks,
    };
  } catch {
    return {
      ...base,
      classification: "ADAPTER_INCOMPATIBLE",
      checks: { ...base.checks, nativeRustValidation: "PASS", roundTripProjection: "FAILED" },
    };
  }
}
