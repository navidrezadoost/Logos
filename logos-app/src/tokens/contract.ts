/** Typed TypeScript projection of the v0.1 messages passed to Rust WASM. */
export interface TokenReference { namespace: string; tokenId: string }
export interface ColorValue { space: "srgb"; hex: string; alpha: number }
export interface TypedColor { type: "color"; value: ColorValue }
export interface LiteralExpression { kind: "literal"; value: TypedColor }
export interface TokenDefinition {
  id: string;
  namespace: string;
  name: string;
  type: "color";
  groupPath: string[];
  values: Array<{ when: Record<string, string>; expression: LiteralExpression }>;
}
export interface TokenSnapshot {
  contractVersion: "0.1.0";
  status: "PROPOSED";
  revision: string;
  documentNamespace: string;
  tokens: TokenDefinition[];
  collections: unknown[];
  sets: unknown[];
  contextDimensions: unknown[];
  activeSetIds: string[];
}
export interface PropertyValueLiteral { kind: "literal"; value: TypedColor }
export interface PropertyValueBinding {
  kind: "binding";
  target: TokenReference;
  modeStrategy: { kind: "inherit" };
  fallback?: TypedColor;
  localOverride?: TypedColor;
}
export type PropertyValue = PropertyValueLiteral | PropertyValueBinding;
export interface ResolveRequest {
  contractVersion: "0.1.0";
  snapshotRevision: string;
  propertyType: "color";
  property: PropertyValue;
  context: { explicit: Record<string, string>; ancestorsNearestFirst: Record<string, string>[]; page: Record<string, string>; document: Record<string, string> };
}
export interface TokenDiagnostic { code: string; severity: "error" | "warning"; message: string; propertyPath?: string }
export interface ResolveResult {
  contractVersion: "0.1.0";
  snapshotRevision: string;
  status: "RESOLVED" | "ERROR";
  value?: TypedColor;
  source?: "DIRECT" | "TOKEN" | "LOCAL_OVERRIDE" | "FALLBACK";
  diagnostics: TokenDiagnostic[];
  dependencyTrace: TokenReference[];
  effectiveContext: Record<string, string>;
}
export interface UserIntent {
  protocolVersion: "0.1.0";
  intentId: string;
  text: string;
  requestedCapabilities: string[];
  constraints: string[];
}
export interface DocumentContext {
  protocolVersion: "0.1.0";
  documentId: string;
  revision: string;
  selectedShapeIds: string[];
  propertyValues: Array<{ shapeId: string; propertyPath: string; value: PropertyValue }>;
  allowedNamespaces: string[];
  permissions: string[];
}
export type ProposedOperation =
  | { kind: "createToken"; definition: TokenDefinition }
  | { kind: "setTokenValue"; target: TokenReference; context: Record<string, string>; expectedValue: LiteralExpression; nextValue: LiteralExpression }
  | { kind: "setProperty"; shapeId: string; propertyPath: string; expectedProperty: PropertyValue; nextProperty: PropertyValue };
export interface ProposedPlan {
  protocolVersion: "0.1.0";
  planId: string;
  baseRevision: string;
  intentId: string;
  summary: string;
  operations: ProposedOperation[];
  assumptions: string[];
  previewContexts: Record<string, string>[];
  requiresUserApproval: true;
}
export interface ImpactSummary {
  createdTokens: number;
  updatedTokens: number;
  boundProperties: number;
  affectedPropertyPaths: string[];
  contextsEvaluated: Record<string, string>[];
  propertyPreviews: Array<{
    shapeId: string;
    propertyPath: string;
    context: Record<string, string>;
    propertyValue: PropertyValue;
    resolution: ResolveResult;
  }>;
  diagnostics: TokenDiagnostic[];
}
export interface ValidationResult {
  protocolVersion: "0.1.0";
  planId: string;
  baseRevision: string;
  status: "READY" | "REJECTED" | "NEEDS_CONFIRMATION";
  planDigest: string;
  impact: ImpactSummary;
  diagnostics: TokenDiagnostic[];
}
