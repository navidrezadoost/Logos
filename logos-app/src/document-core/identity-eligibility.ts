import { toRustCandidate } from "./react-adapter";
import type { DocumentIdentitySource, Page } from "../stores/documentStore";
import type { Shape } from "../types/shapes";

export type RustIdentityEligibility =
  | "CANONICAL_UUID_READY"
  | "LEGACY_IDENTITY"
  | "DOCUMENT_ID_UNAVAILABLE"
  | "UNSUPPORTED_DOCUMENT_CONTENT"
  | "AMBIGUOUS_OWNERSHIP";

export interface IdentityEligibilityInput {
  documentId: string | null;
  identitySource: DocumentIdentitySource;
  pages: Record<string, Page>;
  pageOrder: string[];
  shapes: Record<string, Shape>;
}

/** Read-only prerequisite check; it never repairs IDs or writes document state. */
export function classifyRustIdentityEligibility(
  input: IdentityEligibilityInput,
): RustIdentityEligibility {
  if (!input.documentId) return "DOCUMENT_ID_UNAVAILABLE";
  if (input.identitySource === "unknown" || input.identitySource === "ambiguous") {
    return "AMBIGUOUS_OWNERSHIP";
  }
  if (input.identitySource === "legacy") return "LEGACY_IDENTITY";

  const converted = toRustCandidate({
    documentId: input.documentId,
    pages: input.pages,
    pageOrder: input.pageOrder,
    shapes: input.shapes,
  });
  if (converted.ok) return "CANONICAL_UUID_READY";

  if (converted.issues.some(({ code }) =>
    code === "IDENTITY_REQUIRED" || code === "IDENTITY_NOT_CANONICAL" || code === "UNSUPPORTED_PAGE_ID")) {
    return "LEGACY_IDENTITY";
  }
  return "UNSUPPORTED_DOCUMENT_CONTENT";
}
