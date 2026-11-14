import type { DocumentContext, LiteralExpression, PropertyValue, ProposedPlan, TokenDefinition, TokenSnapshot, UserIntent, TypedColor } from "./contract";

export const fillPropertyPath = (index: number): string => `fills[${index}].color`;

export interface TokenWorkflowProposal {
  snapshot: TokenSnapshot;
  intent: UserIntent;
  context: DocumentContext;
  plan: ProposedPlan;
  binding: Extract<PropertyValue, { kind: "binding" }>;
}

export interface TokenWorkflowSession extends TokenWorkflowProposal {
  definition: TokenDefinition;
  directProperty: PropertyValue;
  propertyPath: string;
}

export function typedColor(hex: string): TypedColor {
  const normalized = hex.toUpperCase();
  if (!/^#[0-9A-F]{6}$/.test(normalized)) throw new Error("Enter a six-digit hex color.");
  return { type: "color", value: { space: "srgb", hex: normalized, alpha: 1 } };
}

export function createTokenWorkflowProposal(shapeId: string, fillHex: string, tokenName: string, fillIndex = 0): TokenWorkflowSession {
  const name = tokenName.trim();
  if (!name) throw new Error("Give the color token a name.");
  const revision = crypto.randomUUID();
  const intentId = crypto.randomUUID();
  const planId = crypto.randomUUID();
  const tokenId = crypto.randomUUID();
  const value = typedColor(fillHex);
  const propertyPath = fillPropertyPath(fillIndex);
  const directProperty: PropertyValue = { kind: "literal", value };
  const expression: LiteralExpression = { kind: "literal", value };
  const definition: TokenDefinition = {
    id: tokenId,
    namespace: "document",
    name,
    type: "color",
    groupPath: [],
    values: [{ when: {}, expression }],
  };
  const binding: Extract<PropertyValue, { kind: "binding" }> = {
    kind: "binding",
    target: { namespace: "document", tokenId },
    modeStrategy: { kind: "inherit" },
    fallback: value,
  };
  const snapshot: TokenSnapshot = {
    contractVersion: "0.1.0", status: "PROPOSED", revision, documentNamespace: "document",
    tokens: [], collections: [], sets: [], contextDimensions: [], activeSetIds: [],
  };
  const intent: UserIntent = {
    protocolVersion: "0.1.0", intentId,
    text: `Save this fill as ${name} and apply it to the selected shape.`,
    requestedCapabilities: ["createToken", "bindProperty"], constraints: [],
  };
  const context: DocumentContext = {
    protocolVersion: "0.1.0", documentId: crypto.randomUUID(), revision,
    selectedShapeIds: [shapeId],
    propertyValues: [{ shapeId, propertyPath, value: directProperty }],
    allowedNamespaces: ["document"], permissions: ["readDocument", "readTokens", "proposeChanges"],
  };
  const plan: ProposedPlan = {
    protocolVersion: "0.1.0", planId, baseRevision: revision, intentId,
    summary: `Create ${name} and bind the selected fill.`,
    operations: [
      { kind: "createToken", definition },
      { kind: "setProperty", shapeId, propertyPath, expectedProperty: directProperty, nextProperty: binding },
    ],
    assumptions: [], previewContexts: [], requiresUserApproval: true,
  };
  return { snapshot, intent, context, plan, definition, binding, directProperty, propertyPath };
}

export function createTokenValuePreview(session: TokenWorkflowSession, nextHex: string): ProposedPlan {
  const previous = session.definition.values[0]?.expression;
  if (!previous) throw new Error("The proposed token has no base value.");
  const nextValue: LiteralExpression = { kind: "literal", value: typedColor(nextHex) };
  return {
    protocolVersion: "0.1.0", planId: crypto.randomUUID(), baseRevision: session.snapshot.revision,
    intentId: session.intent.intentId,
    summary: `Preview a value change to ${session.definition.name}.`,
    operations: [
      { kind: "setTokenValue", target: session.binding.target, context: {}, expectedValue: previous, nextValue },
      // Re-propose the binding against the unchanged direct document pre-state
      // so the Rust plan validator resolves its effective value after the token update.
      { kind: "setProperty", shapeId: session.context.selectedShapeIds[0], propertyPath: session.propertyPath, expectedProperty: session.directProperty, nextProperty: session.binding },
    ],
    assumptions: [], previewContexts: [], requiresUserApproval: true,
  };
}

export function snapshotWithDefinition(session: TokenWorkflowSession, expression: LiteralExpression): TokenSnapshot {
  return { ...session.snapshot, tokens: [{ ...session.definition, values: [{ when: {}, expression }] }] };
}
