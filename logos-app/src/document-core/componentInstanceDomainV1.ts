/**
 * FG-C3.1 versioned, pure candidate contract for component/instance data.
 *
 * Isolated from Zustand, production resolution, rendering, and persistence. This
 * module validates caller-supplied identities; it never issues or maps IDs.
 * Unknown fields are rejected (the v1 import policy); explicit unresolved
 * references and their JSON locator payload are retained and reported.
 */
export const COMPONENT_INSTANCE_DOMAIN_VERSION = 1 as const;

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export type ComponentReference =
  | { kind: "local-component"; componentId: string }
  | { kind: "local-set"; componentSetId: string }
  | { kind: "external"; fileId: string; componentId: string; revision: string | null }
  | { kind: "unresolved"; locator: JsonValue };

export interface VariantProperty {
  id: string;
  name: string;
  kind: "variant";
  values: string[];
  defaultValue: string;
}
export interface BooleanProperty {
  id: string;
  name: string;
  kind: "boolean";
  targetNodeId: string;
  defaultValue: boolean;
}
export interface TextProperty {
  id: string;
  name: string;
  kind: "text";
  targetNodeId: string;
  defaultValue: string;
}
export interface InstanceSwapProperty {
  id: string;
  name: string;
  kind: "instance-swap";
  targetNodeId: string;
  allowedComponentIds: string[];
  defaultComponentId: string;
}
export interface SlotProperty {
  id: string;
  name: string;
  kind: "slot";
  targetNodeId: string;
  guidance?: {
    minimumEntries?: number;
    maximumEntries?: number;
    preferredComponentIds?: string[];
  };
  /** Explicit allow-list restriction, distinct from advisory preferred instances. */
  onlyAllowPreferredInstances?: boolean;
  /** Explicit Logos-authored hard constraints; never inferred from guidance. */
  hardLimits?: { minimumEntries?: number; maximumEntries?: number };
}
export type ComponentProperty = BooleanProperty | TextProperty | InstanceSwapProperty | SlotProperty;
export type ComponentSetProperty = VariantProperty;

export interface DefinitionNode {
  id: string;
  kind: "container" | "shape" | "text" | "instance";
  parentId: string | null;
  childIds: string[];
  source?: ComponentReference;
  variantSelection?: Record<string, string>;
}
export interface ComponentDefinition {
  id: string;
  name: string;
  setId: string | null;
  rootNodeIds: string[];
  nodes: Record<string, DefinitionNode>;
  properties: Record<string, ComponentProperty>;
  /** Exact set-property values for a set member; empty for standalone components. */
  variantValues: Record<string, string>;
}
export interface ComponentSet {
  id: string;
  name: string;
  /** Membership order is semantic and is preserved exactly. */
  memberComponentIds: string[];
  properties: Record<string, ComponentSetProperty>;
}
export type OverrideField = "name" | "text" | "fills" | "strokes" | "effects" | "layoutGuides" | "exportSettings";
export interface InstanceOverride {
  address: { targetNodeId: string; field: OverrideField };
  value: JsonValue;
}
export interface SlotContentNode {
  /** Authored slot-content identity; not an expanded definition-child identity. */
  id: string;
  kind: "container" | "shape" | "text" | "instance";
  parentId: string | null;
  childIds: string[];
  source?: ComponentReference;
  variantSelection?: Record<string, string>;
}
export interface SlotContent {
  rootNodeIds: string[];
  nodes: Record<string, SlotContentNode>;
}
export interface ComponentInstance {
  id: string;
  source: ComponentReference;
  /** Set-property selection for a local set reference. */
  variantSelection: Record<string, string>;
  /** Explicit exposed component property values; defaults may be omitted. */
  propertyValues: Record<string, JsonValue>;
  overrides: InstanceOverride[];
  /** Slot property ID -> ordered, authored content graph. */
  slotContent: Record<string, SlotContent>;
}
export interface ComponentInstanceDocumentV1 {
  version: typeof COMPONENT_INSTANCE_DOMAIN_VERSION;
  components: Record<string, ComponentDefinition>;
  componentSets: Record<string, ComponentSet>;
  instances: Record<string, ComponentInstance>;
}

export type ComponentDiagnosticCode =
  | "COMPONENT_DOCUMENT_MALFORMED" | "COMPONENT_VERSION_UNSUPPORTED" | "COMPONENT_UNKNOWN_FIELD"
  | "COMPONENT_JSON_INVALID" | "COMPONENT_RECORD_MALFORMED" | "COMPONENT_ID_MISMATCH"
  | "COMPONENT_DUPLICATE_ID" | "COMPONENT_PROPERTY_KIND_UNSUPPORTED" | "COMPONENT_PROPERTY_INVALID"
  | "COMPONENT_NODE_GRAPH_INVALID" | "COMPONENT_NODE_KIND_UNSUPPORTED" | "COMPONENT_REFERENCE_INVALID"
  | "COMPONENT_REFERENCE_MISSING" | "COMPONENT_REFERENCE_UNRESOLVED" | "COMPONENT_REFERENCE_CYCLE"
  | "COMPONENT_SET_MEMBERSHIP_INVALID" | "COMPONENT_VARIANT_INVALID" | "COMPONENT_VARIANT_DUPLICATE"
  | "COMPONENT_VARIANT_SELECTION_INVALID" | "COMPONENT_PROPERTY_VALUE_INVALID"
  | "COMPONENT_OVERRIDE_INVALID" | "COMPONENT_OVERRIDE_TARGET_MISSING" | "COMPONENT_OVERRIDE_FIELD_INVALID"
  | "COMPONENT_SLOT_INVALID" | "COMPONENT_SLOT_GUIDANCE_EXCEEDED" | "COMPONENT_SLOT_HARD_LIMIT_EXCEEDED" | "COMPONENT_SLOT_CONTENT_NOT_ALLOWED"
  | "COMPONENT_AMBIGUOUS_SOURCE";
export interface ComponentDiagnostic {
  code: ComponentDiagnosticCode;
  path: string;
  severity: "error" | "warning";
  detail: string;
}
export interface ComponentValidationResult {
  ok: boolean;
  diagnostics: ComponentDiagnostic[];
  /** The original reference is returned only when validation has no errors. */
  document: ComponentInstanceDocumentV1 | null;
}

const OVERRIDE_FIELDS = new Set<OverrideField>(["name", "text", "fills", "strokes", "effects", "layoutGuides", "exportSettings"]);
const SLOT_CONTENT_KINDS = new Set(["container", "shape", "text", "instance"]);
const DEFINITION_KINDS = new Set(["container", "shape", "text", "instance"]);
type AnyRecord = Record<string, unknown>;
const isRecord = (v: unknown): v is AnyRecord => {
  if (v === null || typeof v !== "object" || Array.isArray(v)) return false;
  try { const p = Object.getPrototypeOf(v); return p === Object.prototype || p === null; } catch { return false; }
};
const has = (o: AnyRecord, k: string) => Object.prototype.hasOwnProperty.call(o, k);
const nonEmpty = (v: unknown): v is string => typeof v === "string" && v.length > 0;
const stringArray = (v: unknown): v is string[] => Array.isArray(v) && v.every(nonEmpty);

function isJson(value: unknown, seen = new Set<object>()): value is JsonValue {
  if (value === null || typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value !== "object") return false;
  if (seen.has(value)) return false;
  seen.add(value);
  let ok = false;
  if (Array.isArray(value)) ok = value.every((v) => isJson(v, seen));
  else if (isRecord(value)) ok = Object.keys(value).sort().every((k) => isJson(value[k], seen));
  seen.delete(value);
  return ok;
}
function onlyKeys(o: AnyRecord, allowed: string[]): string | null {
  const set = new Set(allowed);
  return Object.keys(o).sort().find((k) => !set.has(k)) ?? null;
}
function add(ds: ComponentDiagnostic[], code: ComponentDiagnosticCode, path: string, detail: string, severity: "error" | "warning" = "error") {
  ds.push({ code, path, severity, detail });
}
function strictRecord(v: unknown, path: string, allowed: string[], ds: ComponentDiagnostic[]): v is AnyRecord {
  if (!isRecord(v)) { add(ds, "COMPONENT_RECORD_MALFORMED", path, "Expected a plain JSON object."); return false; }
  const extra = onlyKeys(v, allowed);
  if (extra) add(ds, "COMPONENT_UNKNOWN_FIELD", `${path}.${extra}`, "Unknown fields are rejected by component-domain v1.");
  return !extra;
}

function validateReference(ref: unknown, path: string, ds: ComponentDiagnostic[], localComponentIds: Set<string>, localSetIds: Set<string>): void {
  if (!isRecord(ref) || typeof ref.kind !== "string") { add(ds, "COMPONENT_REFERENCE_INVALID", path, "Expected an explicit source reference."); return; }
  if (ref.kind === "local-component") {
    if (!strictRecord(ref, path, ["kind", "componentId"], ds) || !nonEmpty(ref.componentId)) { add(ds, "COMPONENT_REFERENCE_INVALID", path, "Invalid local component reference."); return; }
    if (!localComponentIds.has(ref.componentId)) add(ds, "COMPONENT_REFERENCE_MISSING", path, `Unknown component ID ${ref.componentId}.`);
    // Reference graph edges are added by validateNestedSelection, which also
    // checks exact member selection for local-set references.
  } else if (ref.kind === "local-set") {
    if (!strictRecord(ref, path, ["kind", "componentSetId"], ds) || !nonEmpty(ref.componentSetId)) { add(ds, "COMPONENT_REFERENCE_INVALID", path, "Invalid local component-set reference."); return; }
    if (!localSetIds.has(ref.componentSetId)) add(ds, "COMPONENT_REFERENCE_MISSING", path, `Unknown component-set ID ${ref.componentSetId}.`);
    // Set references are resolved after validating their selected tuple.
  } else if (ref.kind === "external") {
    if (!strictRecord(ref, path, ["kind", "fileId", "componentId", "revision"], ds)
      || !nonEmpty(ref.fileId) || !nonEmpty(ref.componentId) || !(ref.revision === null || nonEmpty(ref.revision))) {
      add(ds, "COMPONENT_REFERENCE_INVALID", path, "External references require fileId, componentId, and nullable revision.");
    } else add(ds, "COMPONENT_REFERENCE_UNRESOLVED", path, "External source identity is explicit but cannot be resolved from this isolated document.", "warning");
  } else if (ref.kind === "unresolved") {
    if (!strictRecord(ref, path, ["kind", "locator"], ds) || !has(ref, "locator") || !isJson(ref.locator)) {
      add(ds, "COMPONENT_REFERENCE_INVALID", path, "Unresolved source locator must be JSON data.");
    } else add(ds, "COMPONENT_REFERENCE_UNRESOLVED", path, "Unresolved source locator is preserved; resolution is blocked.", "warning");
  } else add(ds, "COMPONENT_REFERENCE_INVALID", path, "Unknown reference kind.");
}

function validateNestedSelection(
  ref: unknown, selection: unknown, path: string, ds: ComponentDiagnostic[],
  components: Record<string, unknown>, sets: Record<string, unknown>, edges: Set<string>,
): void {
  if (!isRecord(selection) || Object.values(selection).some((v) => !nonEmpty(v))) {
    add(ds, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Nested instance selection must be a string-valued map.");
    return;
  }
  if (isRecord(ref) && ref.kind === "local-set" && typeof ref.componentSetId === "string") {
    const set = sets[ref.componentSetId];
    if (!isRecord(set) || !isRecord(set.properties) || !Array.isArray(set.memberComponentIds)) return;
    const props = set.properties;
    if (Object.keys(selection).length !== Object.keys(props).length || Object.keys(selection).some((key) => {
      const property = props[key]; return !isRecord(property) || !Array.isArray(property.values) || !property.values.includes(selection[key]);
    })) {
      add(ds, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Nested set reference needs one allowed value for every set property.");
      return;
    }
    const matches = set.memberComponentIds.filter((memberId) => {
      const member = components[String(memberId)];
      const values = isRecord(member) && isRecord(member.variantValues) ? member.variantValues : null;
      return values !== null && Object.keys(selection).every((key) => values[key] === selection[key]);
    });
    if (matches.length > 1) add(ds, "COMPONENT_AMBIGUOUS_SOURCE", path, "Nested set reference matches multiple component definitions.");
    else if (matches.length === 0) add(ds, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Nested set reference does not resolve to a member.");
    else edges.add(String(matches[0]));
  } else if (isRecord(ref) && ref.kind === "local-component" && typeof ref.componentId === "string") {
    if (Object.keys(selection).length) add(ds, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "A direct component reference cannot also carry set selection.");
    else if (components[ref.componentId]) edges.add(ref.componentId);
  } else if (Object.keys(selection).length) {
    add(ds, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "External or unresolved references cannot be locally variant-selected.");
  }
}

function resolveLocalComponentReference(ref: unknown, selection: unknown, components: Record<string, unknown>, sets: Record<string, unknown>): string | null {
  if (!isRecord(ref)) return null;
  if (ref.kind === "local-component" && typeof ref.componentId === "string" && components[ref.componentId]) return ref.componentId;
  if (ref.kind !== "local-set" || typeof ref.componentSetId !== "string" || !isRecord(selection)) return null;
  const set = sets[ref.componentSetId];
  if (!isRecord(set) || !isRecord(set.properties) || !Array.isArray(set.memberComponentIds)) return null;
  const matches = set.memberComponentIds.filter((memberId) => {
    const member = components[String(memberId)];
    const values = isRecord(member) && isRecord(member.variantValues) ? member.variantValues : null;
    return values !== null && Object.keys(selection).length === Object.keys(set.properties as AnyRecord).length
      && Object.keys(selection).every((key) => values[key] === selection[key]);
  });
  return matches.length === 1 ? String(matches[0]) : null;
}

function validateNodeGraph(nodesValue: unknown, rootsValue: unknown, path: string, ds: ComponentDiagnostic[], allowedKinds: Set<string>): Set<string> {
  const ids = new Set<string>();
  if (!isRecord(nodesValue) || !stringArray(rootsValue)) { add(ds, "COMPONENT_NODE_GRAPH_INVALID", path, "Expected a node map and ordered root ID array."); return ids; }
  const nodes = nodesValue as Record<string, unknown>;
  for (const id of Object.keys(nodes).sort()) {
    const n = nodes[id];
    if (!isRecord(n) || n.id !== id || !nonEmpty(id) || !allowedKinds.has(String(n.kind))
      || !(n.parentId === null || nonEmpty(n.parentId)) || !stringArray(n.childIds)) {
      add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}`, "Malformed node, key/ID mismatch, unsupported kind, or malformed membership.");
      continue;
    }
    const allowedNodeFields = n.kind === "instance"
      ? ["id", "kind", "parentId", "childIds", "source", "variantSelection"]
      : ["id", "kind", "parentId", "childIds"];
    const extra = onlyKeys(n, allowedNodeFields);
    if (extra) add(ds, "COMPONENT_UNKNOWN_FIELD", `${path}.nodes.${id}.${extra}`, "Unknown node fields are rejected by component-domain v1.");
    if (n.kind === "instance" && (!has(n, "source") || !isRecord(n.variantSelection) || Object.values(n.variantSelection).some((v) => !nonEmpty(v)))) add(ds, "COMPONENT_REFERENCE_INVALID", `${path}.nodes.${id}.source`, "Nested instance needs an explicit source and string-valued variant selection map.");
    if (n.kind === "shape" && has(n, "source")) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}.source`, "Only instance nodes may carry a source reference.");
    if ((n.kind === "shape" || n.kind === "text" || n.kind === "instance") && n.childIds.length !== 0) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}.childIds`, "Only container nodes may own structural children.");
    ids.add(id);
  }
  if (new Set(rootsValue).size !== rootsValue.length || rootsValue.some((id) => !ids.has(id))) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.rootNodeIds`, "Roots must be unique node IDs in the graph.");
  const memberships = new Map<string, string>();
  const walk = (id: string, parent: string | null, stack: Set<string>) => {
    const node = nodes[id];
    if (!isRecord(node)) return;
    if (stack.has(id)) { add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}`, "Node containment cycle detected."); return; }
    if (node.parentId !== parent) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}.parentId`, "Parent and ordered child membership are not reciprocal.");
    if (memberships.has(id)) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}`, "Node has multiple owners.");
    memberships.set(id, parent ?? "<root>");
    const next = new Set(stack); next.add(id);
    if (Array.isArray(node.childIds)) for (const child of node.childIds) {
      if (!ids.has(child)) add(ds, "COMPONENT_NODE_GRAPH_INVALID", `${path}.nodes.${id}.childIds`, `Missing child ${child}.`);
      else walk(child, id, next);
    }
  };
  for (const root of rootsValue) walk(root, null, new Set());
  if (memberships.size !== ids.size) add(ds, "COMPONENT_NODE_GRAPH_INVALID", path, "Graph contains unreachable nodes.");
  return ids;
}

function validateProperty(value: unknown, key: string, path: string, nodeIds: Set<string>, ds: ComponentDiagnostic[], setContext = false): void {
  if (!isRecord(value) || value.id !== key || !nonEmpty(value.name) || typeof value.kind !== "string") {
    add(ds, "COMPONENT_PROPERTY_INVALID", path, "Malformed property record or key/ID mismatch."); return;
  }
  if (value.kind === "variant") {
    if (!setContext) { add(ds, "COMPONENT_PROPERTY_KIND_UNSUPPORTED", `${path}.kind`, "Variant properties belong to a Component Set in v1."); return; }
    if (!strictRecord(value, path, ["id", "name", "kind", "values", "defaultValue"], ds)
      || !stringArray(value.values) || value.values.length === 0 || new Set(value.values).size !== value.values.length
      || !nonEmpty(value.defaultValue) || !value.values.includes(value.defaultValue)) add(ds, "COMPONENT_VARIANT_INVALID", path, "Variant property values must be unique and include the default.");
    return;
  }
  if (setContext) { add(ds, "COMPONENT_VARIANT_INVALID", path, "Component Set properties must have kind variant."); return; }
  if (!nonEmpty(value.targetNodeId) || !nodeIds.has(value.targetNodeId)) add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.targetNodeId`, "Property target is missing from this definition.");
  if (value.kind === "boolean") {
    if (!strictRecord(value, path, ["id", "name", "kind", "targetNodeId", "defaultValue"], ds) || typeof value.defaultValue !== "boolean") add(ds, "COMPONENT_PROPERTY_INVALID", path, "Boolean default must be boolean.");
  } else if (value.kind === "text") {
    if (!strictRecord(value, path, ["id", "name", "kind", "targetNodeId", "defaultValue"], ds) || typeof value.defaultValue !== "string") add(ds, "COMPONENT_PROPERTY_INVALID", path, "Text default must be a string.");
  } else if (value.kind === "instance-swap") {
    if (!strictRecord(value, path, ["id", "name", "kind", "targetNodeId", "allowedComponentIds", "defaultComponentId"], ds)
      || !stringArray(value.allowedComponentIds) || new Set(value.allowedComponentIds).size !== value.allowedComponentIds.length
      || !nonEmpty(value.defaultComponentId) || !value.allowedComponentIds.includes(value.defaultComponentId)) add(ds, "COMPONENT_PROPERTY_INVALID", path, "Instance-swap options must be unique and include the default component.");
  } else if (value.kind === "slot") {
    if (!strictRecord(value, path, ["id", "name", "kind", "targetNodeId", "guidance", "onlyAllowPreferredInstances", "hardLimits"], ds)) return;
    if (value.onlyAllowPreferredInstances !== undefined && typeof value.onlyAllowPreferredInstances !== "boolean") add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.onlyAllowPreferredInstances`, "Only-allow-preferred-instances must be boolean.");
    if (value.onlyAllowPreferredInstances === true && (!isRecord(value.guidance) || !Array.isArray(value.guidance.preferredComponentIds) || value.guidance.preferredComponentIds.length === 0)) add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.onlyAllowPreferredInstances`, "An explicit allow-list requires at least one preferred component ID.");
    for (const keyName of ["guidance", "hardLimits"] as const) {
      const limits = value[keyName];
      if (limits === undefined) continue;
      const allowed = keyName === "guidance" ? ["minimumEntries", "maximumEntries", "preferredComponentIds"] : ["minimumEntries", "maximumEntries"];
      if (!isRecord(limits) || onlyKeys(limits, allowed) !== null
        || (limits.minimumEntries !== undefined && (!Number.isInteger(limits.minimumEntries) || (limits.minimumEntries as number) < 0))
        || (limits.maximumEntries !== undefined && (!Number.isInteger(limits.maximumEntries) || (limits.maximumEntries as number) < 0))
        || (limits.minimumEntries !== undefined && limits.maximumEntries !== undefined && (limits.minimumEntries as number) > (limits.maximumEntries as number))
        || (limits.preferredComponentIds !== undefined && !stringArray(limits.preferredComponentIds))) add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.${keyName}`, "Invalid slot guidance or explicit hard limits.");
    }
  } else add(ds, "COMPONENT_PROPERTY_KIND_UNSUPPORTED", `${path}.kind`, `Unsupported property kind: ${value.kind}.`);
}

function validatePropertyTarget(property: unknown, nodes: unknown, path: string, ds: ComponentDiagnostic[]): void {
  if (!isRecord(property) || !nonEmpty(property.targetNodeId) || !isRecord(nodes)) return;
  const target = nodes[property.targetNodeId];
  if (!isRecord(target)) return; // Missing target has its own stable diagnostic.
  if (property.kind === "text" && target.kind !== "text") add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.targetNodeId`, "Text property must target a text node.");
  if (property.kind === "instance-swap" && target.kind !== "instance") add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.targetNodeId`, "Instance-swap property must target a nested instance node.");
  if (property.kind === "slot" && target.kind !== "container") add(ds, "COMPONENT_PROPERTY_INVALID", `${path}.targetNodeId`, "Slot property must target a container node.");
}

function validateSlotContent(contentValue: unknown, path: string, ds: ComponentDiagnostic[], refs: (r: unknown, s: unknown, p: string) => void): Set<string> {
  if (!isRecord(contentValue) || onlyKeys(contentValue, ["rootNodeIds", "nodes"]) !== null) { add(ds, "COMPONENT_SLOT_INVALID", path, "Slot content must contain only ordered roots and nodes."); return new Set(); }
  const nodeIds = validateNodeGraph(contentValue.nodes, contentValue.rootNodeIds, path, ds, SLOT_CONTENT_KINDS);
  if (isRecord(contentValue.nodes)) for (const id of Object.keys(contentValue.nodes).sort()) {
    const node = contentValue.nodes[id];
    if (isRecord(node) && node.kind === "instance") {
      if (onlyKeys(node, ["id", "kind", "parentId", "childIds", "source", "variantSelection"]) !== null || !has(node, "source") || !isRecord(node.variantSelection)) add(ds, "COMPONENT_SLOT_INVALID", `${path}.nodes.${id}`, "Malformed slot instance node.");
      else refs(node.source, node.variantSelection, `${path}.nodes.${id}.source`);
    } else if (isRecord(node) && onlyKeys(node, ["id", "kind", "parentId", "childIds"]) !== null) add(ds, "COMPONENT_SLOT_INVALID", `${path}.nodes.${id}`, "Unknown slot-content node fields are rejected.");
  }
  return nodeIds;
}

function overrideTypeOk(field: OverrideField, value: unknown): boolean {
  if (!isJson(value)) return false;
  if (field === "name" || field === "text") return typeof value === "string";
  if (field === "fills" || field === "strokes" || field === "effects" || field === "layoutGuides" || field === "exportSettings") return Array.isArray(value);
  return false;
}

/** Validate JSON-like unknown input. It is pure and returns deterministic diagnostics. */
export function validateComponentInstanceDocument(input: unknown): ComponentValidationResult {
  const diagnostics: ComponentDiagnostic[] = [];
  if (!isJson(input) || !isRecord(input)) {
    add(diagnostics, "COMPONENT_JSON_INVALID", "$", "Input must be acyclic plain JSON data with finite numbers.");
    return { ok: false, diagnostics, document: null };
  }
  if (input.version !== COMPONENT_INSTANCE_DOMAIN_VERSION) {
    add(diagnostics, "COMPONENT_VERSION_UNSUPPORTED", "$.version", "Unsupported component-domain version.");
    return { ok: false, diagnostics, document: null };
  }
  if (onlyKeys(input, ["version", "components", "componentSets", "instances"]) !== null) {
    const extra = onlyKeys(input, ["version", "components", "componentSets", "instances"])!;
    add(diagnostics, "COMPONENT_UNKNOWN_FIELD", `$.${extra}`, "Unknown fields are rejected by component-domain v1.");
  }
  if (!isRecord(input.components) || !isRecord(input.componentSets) || !isRecord(input.instances)) {
    add(diagnostics, "COMPONENT_DOCUMENT_MALFORMED", "$", "components, componentSets, and instances must be plain records.");
    return { ok: false, diagnostics, document: null };
  }
  const components = input.components as Record<string, unknown>;
  const sets = input.componentSets as Record<string, unknown>;
  const instances = input.instances as Record<string, unknown>;
  const componentIds = new Set(Object.keys(components));
  const setIds = new Set(Object.keys(sets));
  const componentEdges = new Map<string, Set<string>>();
  const componentNodes = new Map<string, Set<string>>();
  const setsByMember = new Map<string, string>();
  // Component, set, instance, and definition-node IDs share one domain identity
  // namespace. Slot-content IDs are scoped to their owning slot graph.
  const identityOwner = new Map<string, string>();
  const claimIdentity = (id: string, path: string) => {
    const previous = identityOwner.get(id);
    if (previous) add(diagnostics, "COMPONENT_DUPLICATE_ID", path, `ID ${id} is already used at ${previous}.`);
    else identityOwner.set(id, path);
  };
  for (const id of Object.keys(components).sort()) {
    const path = `$.components.${id}`; const value = components[id];
    if (!strictRecord(value, path, ["id", "name", "setId", "rootNodeIds", "nodes", "properties", "variantValues"], diagnostics)) continue;
    if (!nonEmpty(value.id) || !nonEmpty(value.name) || !(value.setId === null || nonEmpty(value.setId)) || !isRecord(value.properties) || !isRecord(value.variantValues)) {
      add(diagnostics, "COMPONENT_RECORD_MALFORMED", path, "Malformed component definition."); continue;
    }
    if (value.id !== id) add(diagnostics, "COMPONENT_ID_MISMATCH", `${path}.id`, "Record ID must match its map key.");
    claimIdentity(value.id, path);
    if (value.setId !== null && !setIds.has(String(value.setId))) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `${path}.setId`, "Component references a missing set.");
    if (value.setId === null && Object.keys(value.variantValues).length) add(diagnostics, "COMPONENT_VARIANT_INVALID", `${path}.variantValues`, "Standalone component cannot carry set variant values.");
    const nodeIds = validateNodeGraph(value.nodes, value.rootNodeIds, path, diagnostics, DEFINITION_KINDS);
    for (const nodeId of [...nodeIds].sort()) claimIdentity(nodeId, `${path}.nodes.${nodeId}`);
    componentNodes.set(id, nodeIds); componentEdges.set(id, new Set());
    for (const propId of Object.keys(value.properties).sort()) {
      validateProperty(value.properties[propId], propId, `${path}.properties.${propId}`, nodeIds, diagnostics);
      validatePropertyTarget(value.properties[propId], value.nodes, `${path}.properties.${propId}`, diagnostics);
      const prop = value.properties[propId];
      if (isRecord(prop) && prop.kind === "instance-swap" && typeof prop.defaultComponentId === "string") componentEdges.get(id)!.add(prop.defaultComponentId);
    }
    for (const propId of Object.keys(value.properties).sort()) {
      const prop = value.properties[propId];
      if (!isRecord(prop)) continue;
      if (prop.kind === "instance-swap" && Array.isArray(prop.allowedComponentIds)) {
        for (const allowedId of prop.allowedComponentIds) if (typeof allowedId === "string" && !componentIds.has(allowedId)) add(diagnostics, "COMPONENT_PROPERTY_INVALID", `${path}.properties.${propId}.allowedComponentIds`, `Unknown allowed component ID ${allowedId}.`);
      }
      if (prop.kind === "slot" && isRecord(prop.guidance) && Array.isArray(prop.guidance.preferredComponentIds)) {
        for (const preferredId of prop.guidance.preferredComponentIds) if (typeof preferredId === "string" && !componentIds.has(preferredId)) add(diagnostics, "COMPONENT_PROPERTY_INVALID", `${path}.properties.${propId}.guidance.preferredComponentIds`, `Unknown preferred component ID ${preferredId}.`);
      }
    }
    if (value.setId !== null && Object.keys(value.properties).length) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `${path}.properties`, "Set member properties are declared on the set, not duplicated on each variant.");
    const rawNodes = isRecord(value.nodes) ? value.nodes : {};
    for (const nodeId of Object.keys(rawNodes).sort()) {
      const n = isRecord(rawNodes[nodeId]) ? rawNodes[nodeId] as AnyRecord : null;
      if (n?.kind === "instance" && has(n, "source")) {
        validateReference(n.source, `${path}.nodes.${nodeId}.source`, diagnostics, componentIds, setIds);
        validateNestedSelection(n.source, n.variantSelection, `${path}.nodes.${nodeId}`, diagnostics, components, sets, componentEdges.get(id)!);
      }
    }
    if (value.setId !== null) {
      const memberId = id;
      if (setsByMember.has(memberId)) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `${path}.setId`, "Component belongs to multiple sets.");
      setsByMember.set(memberId, String(value.setId));
    }
  }
  for (const id of Object.keys(sets).sort()) {
    const path = `$.componentSets.${id}`; const value = sets[id];
    if (!strictRecord(value, path, ["id", "name", "memberComponentIds", "properties"], diagnostics)) continue;
    if (!nonEmpty(value.id) || !nonEmpty(value.name) || !stringArray(value.memberComponentIds) || !isRecord(value.properties) || Object.keys(value.properties).length === 0 || value.memberComponentIds.length === 0) {
      add(diagnostics, "COMPONENT_RECORD_MALFORMED", path, "Malformed component set."); continue;
    }
    if (value.id !== id) add(diagnostics, "COMPONENT_ID_MISMATCH", `${path}.id`, "Record ID must match its map key.");
    claimIdentity(value.id, path);
    if (new Set(value.memberComponentIds).size !== value.memberComponentIds.length) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `${path}.memberComponentIds`, "Set member order cannot contain duplicate IDs.");
    for (const memberId of value.memberComponentIds) {
      if (!componentIds.has(memberId) || setsByMember.get(memberId) !== id) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `${path}.memberComponentIds`, `Membership is not reciprocal for ${memberId}.`);
    }
    const variantProps = new Map<string, VariantProperty>();
    for (const propId of Object.keys(value.properties).sort()) {
      validateProperty(value.properties[propId], propId, `${path}.properties.${propId}`, new Set(), diagnostics, true);
      const prop = value.properties[propId];
      if (isRecord(prop) && prop.kind === "variant") variantProps.set(propId, prop as unknown as VariantProperty);
    }
    const combos = new Set<string>();
    for (const memberId of value.memberComponentIds) {
      const member = components[memberId];
      if (!isRecord(member)) continue;
      const vals = member.variantValues;
      const keys = isRecord(vals) ? Object.keys(vals).sort() : [];
      if (keys.length !== variantProps.size || keys.some((key) => !variantProps.has(key))
        || [...variantProps.keys()].some((key) => !isRecord(vals) || typeof vals[key] !== "string" || !variantProps.get(key)!.values.includes(vals[key] as string))) {
        add(diagnostics, "COMPONENT_VARIANT_INVALID", `$.components.${memberId}.variantValues`, "Variant member must provide exactly one allowed value for each set property.");
        continue;
      }
      const tuple = [...variantProps.keys()].sort().map((key) => `${key}=${(vals as AnyRecord)[key]}`).join("\u0000");
      if (combos.has(tuple)) add(diagnostics, "COMPONENT_VARIANT_DUPLICATE", `$.components.${memberId}.variantValues`, "Variant property combination duplicates another member.");
      combos.add(tuple);
    }
  }
  for (const [memberId, setId] of setsByMember) {
    const set = sets[setId];
    if (!isRecord(set) || !Array.isArray(set.memberComponentIds) || !set.memberComponentIds.includes(memberId)) add(diagnostics, "COMPONENT_SET_MEMBERSHIP_INVALID", `$.components.${memberId}.setId`, "Set does not list this member in its ordered membership.");
  }

  for (const id of Object.keys(instances).sort()) {
    const path = `$.instances.${id}`; const value = instances[id];
    if (!strictRecord(value, path, ["id", "source", "variantSelection", "propertyValues", "overrides", "slotContent"], diagnostics)) continue;
    if (!nonEmpty(value.id) || !isRecord(value.variantSelection) || !isRecord(value.propertyValues) || !Array.isArray(value.overrides) || !isRecord(value.slotContent)) {
      add(diagnostics, "COMPONENT_RECORD_MALFORMED", path, "Malformed instance record."); continue;
    }
    if (value.id !== id) add(diagnostics, "COMPONENT_ID_MISMATCH", `${path}.id`, "Record ID must match its map key.");
    claimIdentity(value.id, path);
    validateReference(value.source, `${path}.source`, diagnostics, componentIds, setIds);
    let definition: AnyRecord | null = null; let set: AnyRecord | null = null;
    if (isRecord(value.source) && value.source.kind === "local-component" && typeof value.source.componentId === "string") {
      const candidate = components[value.source.componentId]; if (isRecord(candidate)) definition = candidate;
    } else if (isRecord(value.source) && value.source.kind === "local-set" && typeof value.source.componentSetId === "string") {
      set = isRecord(sets[value.source.componentSetId]) ? sets[value.source.componentSetId] as AnyRecord : null;
    }
    if (set) {
      const setProps = isRecord(set.properties) ? set.properties : {};
      const selection = value.variantSelection as AnyRecord;
      if (Object.keys(selection).length !== Object.keys(setProps).length || Object.keys(selection).some((p) => {
        const property = setProps[p];
        return !isRecord(property) || !Array.isArray(property.values) || !property.values.includes(selection[p]);
      })) {
        add(diagnostics, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Selection must provide one allowed value for each set property.");
      } else if (Array.isArray(set.memberComponentIds)) {
        const matches = set.memberComponentIds.filter((memberId) => {
          const member = components[String(memberId)];
          const memberValues = isRecord(member) && isRecord(member.variantValues) ? member.variantValues : null;
          return memberValues !== null && Object.keys(selection).every((p) => memberValues[p] === selection[p]);
        });
        if (matches.length > 1) add(diagnostics, "COMPONENT_AMBIGUOUS_SOURCE", `${path}.variantSelection`, "Variant selection matches more than one component definition.");
        else if (matches.length === 0) add(diagnostics, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Variant selection does not resolve to a set member.");
        else { const candidate = components[String(matches[0])]; if (isRecord(candidate)) definition = candidate; }
      }
    } else if (Object.keys(value.variantSelection).length) add(diagnostics, "COMPONENT_VARIANT_SELECTION_INVALID", `${path}.variantSelection`, "Only a reference to a component set may carry variant selection.");
    if (definition) {
      const nodeIds = componentNodes.get(String(definition.id)) ?? new Set<string>();
      const properties = isRecord(definition.properties) ? definition.properties : {};
      const ownerEdges = componentEdges.get(String(definition.id)) ?? new Set<string>();
      for (const propId of Object.keys(value.propertyValues).sort()) {
        const prop = properties[propId]; const propValue = (value.propertyValues as AnyRecord)[propId];
        if (!isRecord(prop)) add(diagnostics, "COMPONENT_PROPERTY_VALUE_INVALID", `${path}.propertyValues.${propId}`, "Value refers to an undeclared property.");
        else if (prop.kind === "boolean" && typeof propValue !== "boolean" || prop.kind === "text" && typeof propValue !== "string") add(diagnostics, "COMPONENT_PROPERTY_VALUE_INVALID", `${path}.propertyValues.${propId}`, "Value does not match property kind.");
        else if (prop.kind === "instance-swap") {
          if (!isRecord(propValue) || onlyKeys(propValue, ["kind", "componentId"]) !== null || propValue.kind !== "local-component" || !Array.isArray(prop.allowedComponentIds) || !prop.allowedComponentIds.includes(propValue.componentId as string)) add(diagnostics, "COMPONENT_PROPERTY_VALUE_INVALID", `${path}.propertyValues.${propId}`, "Swap value is not an explicitly allowed local component reference.");
          else ownerEdges.add(propValue.componentId as string);
        }
        else if (prop.kind === "slot") add(diagnostics, "COMPONENT_PROPERTY_VALUE_INVALID", `${path}.propertyValues.${propId}`, "Slot values belong in slotContent.");
      }
      const overrideAddresses = new Set<string>();
      for (let index = 0; index < value.overrides.length; index++) {
        const override = value.overrides[index]; const opath = `${path}.overrides.${index}`;
        if (!isRecord(override) || onlyKeys(override, ["address", "value"]) !== null || !isRecord(override.address)
          || onlyKeys(override.address, ["targetNodeId", "field"]) !== null || !nonEmpty(override.address.targetNodeId)
          || typeof override.address.field !== "string" || !OVERRIDE_FIELDS.has(override.address.field as OverrideField)) {
          add(diagnostics, "COMPONENT_OVERRIDE_INVALID", opath, "Malformed typed override address."); continue;
        }
        if (!nodeIds.has(override.address.targetNodeId)) add(diagnostics, "COMPONENT_OVERRIDE_TARGET_MISSING", `${opath}.address.targetNodeId`, "Override target is absent from selected definition.");
        if (!overrideTypeOk(override.address.field as OverrideField, override.value)) add(diagnostics, "COMPONENT_OVERRIDE_FIELD_INVALID", `${opath}.value`, "Override value does not match its typed field or is not JSON data.");
        const addressKey = `${override.address.targetNodeId}\u0000${override.address.field}`;
        if (overrideAddresses.has(addressKey)) add(diagnostics, "COMPONENT_OVERRIDE_INVALID", opath, "An instance cannot carry duplicate overrides for the same typed address.");
        overrideAddresses.add(addressKey);
      }
      for (const propId of Object.keys(value.slotContent).sort()) {
        const prop = properties[propId]; const slotPath = `${path}.slotContent.${propId}`;
        if (!isRecord(prop) || prop.kind !== "slot") { add(diagnostics, "COMPONENT_SLOT_INVALID", slotPath, "Slot content must be owned by a declared slot property."); continue; }
        const content = value.slotContent[propId];
        const localIds = validateSlotContent(content, slotPath, diagnostics, (r, selection, p) => {
          validateReference(r, p, diagnostics, componentIds, setIds);
          validateNestedSelection(r, selection, p, diagnostics, components, sets, ownerEdges);
        });
        if (!isRecord(content) || !Array.isArray(content.rootNodeIds)) continue;
        const entryCount = content.rootNodeIds.length;
        const guidance = isRecord(prop.guidance) ? prop.guidance : {};
        const hard = isRecord(prop.hardLimits) ? prop.hardLimits : {};
        if (typeof guidance.minimumEntries === "number" && entryCount < guidance.minimumEntries || typeof guidance.maximumEntries === "number" && entryCount > guidance.maximumEntries) add(diagnostics, "COMPONENT_SLOT_GUIDANCE_EXCEEDED", slotPath, `Slot has ${entryCount} direct entries outside advisory guidance; retained as a warning.`, "warning");
        if (typeof hard.minimumEntries === "number" && entryCount < hard.minimumEntries || typeof hard.maximumEntries === "number" && entryCount > hard.maximumEntries) add(diagnostics, "COMPONENT_SLOT_HARD_LIMIT_EXCEEDED", slotPath, `Slot has ${entryCount} direct entries outside explicitly declared hard limits.`);
        if (prop.onlyAllowPreferredInstances === true && isRecord(content.nodes) && Array.isArray(content.rootNodeIds)) {
          const preferred = isRecord(guidance) && Array.isArray(guidance.preferredComponentIds) ? guidance.preferredComponentIds : [];
          for (const rootId of content.rootNodeIds) {
            const root = content.nodes[String(rootId)];
            const componentId = isRecord(root) && root.kind === "instance"
              ? resolveLocalComponentReference(root.source, root.variantSelection, components, sets)
              : null;
            if (componentId === null || !preferred.includes(componentId)) add(diagnostics, "COMPONENT_SLOT_CONTENT_NOT_ALLOWED", slotPath, `Slot entry ${String(rootId)} is not an explicitly preferred component instance.`);
          }
        }
        // Names are local to a slot-content graph; no definition-child identity mapping is inferred.
        if (localIds.size !== Object.keys(isRecord(content.nodes) ? content.nodes : {}).length) add(diagnostics, "COMPONENT_SLOT_INVALID", slotPath, "Slot content graph identity is ambiguous.");
      }
    } else if (Object.keys(value.propertyValues).length || value.overrides.length || Object.keys(value.slotContent).length) {
      add(diagnostics, "COMPONENT_REFERENCE_UNRESOLVED", path, "Instance payload is preserved, but dependent values cannot be validated until its local source resolves.", "warning");
    }
  }

  const visiting = new Set<string>(); const visited = new Set<string>();
  const visit = (id: string, trail: string[]) => {
    if (visiting.has(id)) { add(diagnostics, "COMPONENT_REFERENCE_CYCLE", `$.components.${id}`, `Nested component reference cycle: ${[...trail, id].join(" -> ")}.`); return; }
    if (visited.has(id)) return;
    visiting.add(id);
    for (const next of [...(componentEdges.get(id) ?? [])].sort()) if (componentIds.has(next)) visit(next, [...trail, id]);
    visiting.delete(id); visited.add(id);
  };
  for (const id of [...componentIds].sort()) visit(id, []);

  const compare = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
  diagnostics.sort((a, b) => compare(a.path, b.path) || compare(a.code, b.code) || compare(a.severity, b.severity));
  const ok = diagnostics.every((d) => d.severity !== "error");
  return { ok, diagnostics, document: ok ? input as unknown as ComponentInstanceDocumentV1 : null };
}
