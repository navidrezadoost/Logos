/**
 * FG-C2.0 candidate Section domain contract.
 *
 * This is an immutable, renderer-independent proposal. It is deliberately not
 * imported by Zustand, the production hierarchy planner, persistence, or the
 * renderer. IDs are supplied by the caller; this module never issues IDs.
 */

export const SECTION_DOMAIN_VERSION = 1 as const;

export type SectionNodeKind =
  | "section"
  | "frame"
  | "group"
  | "rect"
  | "circle"
  | "ellipse"
  | "path"
  | "text"
  | "svg-raw"
  | "vector-network"
  | "bool"
  | "component"
  | "component-set"
  | "instance";

export interface SectionBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface SectionDomainNode {
  id: string;
  pageId: string;
  kind: SectionNodeKind;
  parentId: string | null;
  childIds: string[];
  locked: boolean;
  visible: boolean;
  /** Present only for Sections; stored independently from child geometry. */
  bounds?: SectionBounds;
  /** Optional page-space geometry payload used to assert no implicit child transform. */
  worldBounds?: SectionBounds;
}

export interface SectionDomainPage {
  id: string;
  childIds: string[];
}

export interface SectionDomainSnapshot {
  version: typeof SECTION_DOMAIN_VERSION;
  pageOrder: string[];
  pages: Record<string, SectionDomainPage>;
  nodes: Record<string, SectionDomainNode>;
}

export type SectionOperation =
  | {
      kind: "create-section";
      id: string;
      pageId: string;
      parentId: string | null;
      index: number;
      bounds: SectionBounds;
      locked?: boolean;
      visible?: boolean;
    }
  | {
      kind: "insert-node";
      node: SectionDomainNode;
      parentId: string;
      index: number;
    }
  | { kind: "reorder"; id: string; index: number }
  | { kind: "reparent"; id: string; parentId: string | null; pageId?: string; index: number }
  | { kind: "delete-section"; id: string; contents: "delete" | "preserve" };

export type SectionDiagnosticCode =
  | "SECTION_DOMAIN_VERSION_UNSUPPORTED"
  | "SECTION_INVALID_UUID"
  | "SECTION_DUPLICATE_ID"
  | "SECTION_PAGE_ORDER_INVALID"
  | "SECTION_PAGE_MISSING"
  | "SECTION_PAGE_OWNERSHIP_MISMATCH"
  | "SECTION_PARENT_MISSING"
  | "SECTION_CHILD_MISSING"
  | "SECTION_DUPLICATE_OWNERSHIP"
  | "SECTION_PARENT_RECIPROCITY_MISMATCH"
  | "SECTION_CYCLE"
  | "SECTION_UNREACHABLE_NODE"
  | "SECTION_CONTAINMENT_FORBIDDEN"
  | "SECTION_RELATIONSHIP_UNQUALIFIED"
  | "SECTION_INVALID_BOUNDS"
  | "SECTION_INVALID_ORDER_INDEX"
  | "SECTION_NODE_ALREADY_EXISTS"
  | "SECTION_INSERT_REQUIRES_EMPTY_NODE"
  | "SECTION_TARGET_MISSING"
  | "SECTION_TARGET_NOT_SECTION"
  | "SECTION_LOCKED_ANCESTOR"
  | "SECTION_CROSS_PAGE_OPERATION"
  | "SECTION_DELETE_CONTENT_POLICY_UNQUALIFIED";

export interface SectionPlanResult {
  ok: true;
  snapshot: SectionDomainSnapshot;
  changedIds: string[];
  diagnostic: null;
}

export interface SectionRejectResult {
  ok: false;
  snapshot: SectionDomainSnapshot;
  changedIds: [];
  diagnostic: { code: SectionDiagnosticCode; detail?: string };
}

export type SectionPlanOutcome = SectionPlanResult | SectionRejectResult;

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const LEAF_KINDS = new Set<SectionNodeKind>([
  "rect", "circle", "ellipse", "path", "text", "svg-raw", "vector-network",
]);
const UNQUALIFIED_KINDS = new Set<SectionNodeKind>(["bool", "component", "component-set", "instance"]);

export function isSectionUuid(id: string): boolean {
  return UUID.test(id);
}

/** Strict containment rules for the supported candidate subset. */
export function sectionContainment(parent: "page" | SectionNodeKind, child: SectionNodeKind):
  | "allowed"
  | "forbidden"
  | "unqualified" {
  if (UNQUALIFIED_KINDS.has(parent as SectionNodeKind) || UNQUALIFIED_KINDS.has(child)) return "unqualified";
  if (parent === "page") return "allowed";
  if (parent === "section") {
    return child === "section" || child === "frame" || child === "group" || LEAF_KINDS.has(child)
      ? "allowed"
      : "forbidden";
  }
  if (parent === "frame" || parent === "group") {
    return child === "section" ? "forbidden" : child === "frame" || child === "group" || LEAF_KINDS.has(child)
      ? "allowed"
      : "forbidden";
  }
  return "forbidden";
}

function reject(snapshot: SectionDomainSnapshot, code: SectionDiagnosticCode, detail?: string): SectionRejectResult {
  return { ok: false, snapshot, changedIds: [], diagnostic: { code, ...(detail ? { detail } : {}) } };
}

function validBounds(bounds: SectionBounds): boolean {
  return Number.isFinite(bounds.x) && Number.isFinite(bounds.y)
    && Number.isFinite(bounds.width) && Number.isFinite(bounds.height)
    && bounds.width >= 0 && bounds.height >= 0;
}

/** Validates exact ownership, page association, reciprocal edges and acyclicity. */
export function validateSectionSnapshot(snapshot: SectionDomainSnapshot): SectionDiagnosticCode | null {
  if (snapshot.version !== SECTION_DOMAIN_VERSION) return "SECTION_DOMAIN_VERSION_UNSUPPORTED";
  const pageIds = Object.keys(snapshot.pages);
  if (new Set(snapshot.pageOrder).size !== snapshot.pageOrder.length
    || snapshot.pageOrder.length !== pageIds.length
    || snapshot.pageOrder.some((id) => !snapshot.pages[id])
    || pageIds.some((id) => !snapshot.pageOrder.includes(id))) return "SECTION_PAGE_ORDER_INVALID";

  const allIds = [...pageIds, ...Object.keys(snapshot.nodes)];
  if (new Set(allIds).size !== allIds.length) return "SECTION_DUPLICATE_ID";
  if (allIds.some((id) => !isSectionUuid(id))) return "SECTION_INVALID_UUID";

  for (const page of Object.values(snapshot.pages)) {
    if (page.id !== Object.keys(snapshot.pages).find((key) => snapshot.pages[key] === page)) return "SECTION_PAGE_MISSING";
  }
  for (const [id, node] of Object.entries(snapshot.nodes)) {
    if (node.id !== id) return "SECTION_DUPLICATE_ID";
    if (node.kind === "section" && (!node.bounds || !validBounds(node.bounds))) return "SECTION_INVALID_BOUNDS";
    if (node.kind !== "section" && node.bounds !== undefined) return "SECTION_INVALID_BOUNDS";
    if (node.worldBounds !== undefined && !validBounds(node.worldBounds)) return "SECTION_INVALID_BOUNDS";
    if (new Set(node.childIds).size !== node.childIds.length) return "SECTION_DUPLICATE_OWNERSHIP";
  }

  const owners = new Map<string, string>();
  for (const page of Object.values(snapshot.pages)) {
    if (new Set(page.childIds).size !== page.childIds.length) return "SECTION_DUPLICATE_OWNERSHIP";
    for (const childId of page.childIds) {
      if (!snapshot.nodes[childId]) return "SECTION_CHILD_MISSING";
      if (owners.has(childId)) return "SECTION_DUPLICATE_OWNERSHIP";
      owners.set(childId, page.id);
    }
  }
  for (const node of Object.values(snapshot.nodes)) {
    for (const childId of node.childIds) {
      const child = snapshot.nodes[childId];
      if (!child) return "SECTION_CHILD_MISSING";
      if (owners.has(childId)) return "SECTION_DUPLICATE_OWNERSHIP";
      const relation = sectionContainment(node.kind, child.kind);
      if (relation === "unqualified") return "SECTION_RELATIONSHIP_UNQUALIFIED";
      if (relation === "forbidden") return "SECTION_CONTAINMENT_FORBIDDEN";
      owners.set(childId, node.id);
    }
  }
  for (const [id, node] of Object.entries(snapshot.nodes)) {
    const owner = owners.get(id);
    if (!owner) return "SECTION_UNREACHABLE_NODE";
    const expectedParent = snapshot.pages[owner] ? null : owner;
    if (node.parentId !== expectedParent) return "SECTION_PARENT_RECIPROCITY_MISMATCH";
    const ownerPage = snapshot.pages[owner]?.id ?? snapshot.nodes[owner]?.pageId;
    if (ownerPage !== node.pageId) return "SECTION_PAGE_OWNERSHIP_MISMATCH";
  }

  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (id: string): boolean => {
    if (visiting.has(id)) return false;
    if (visited.has(id)) return true;
    visiting.add(id);
    for (const childId of snapshot.nodes[id]?.childIds ?? []) if (!visit(childId)) return false;
    visiting.delete(id);
    visited.add(id);
    return true;
  };
  for (const page of Object.values(snapshot.pages)) {
    for (const id of page.childIds) if (!visit(id)) return "SECTION_CYCLE";
  }
  return null;
}

function ownerChildren(snapshot: SectionDomainSnapshot, parentId: string): string[] | null {
  return snapshot.pages[parentId]?.childIds ?? snapshot.nodes[parentId]?.childIds ?? null;
}

function nodeSiblings(snapshot: SectionDomainSnapshot, node: SectionDomainNode): string[] | null {
  return node.parentId === null
    ? snapshot.pages[node.pageId]?.childIds ?? null
    : snapshot.nodes[node.parentId]?.childIds ?? null;
}

function isLockedPath(snapshot: SectionDomainSnapshot, id: string): boolean {
  let current: string | null = id;
  while (current) {
    const currentNode: SectionDomainNode | undefined = snapshot.nodes[current];
    if (currentNode?.locked) return true;
    current = currentNode?.parentId ?? null;
  }
  return false;
}

function insertAt(list: string[], id: string, index: number): boolean {
  if (!Number.isInteger(index) || index < 0 || index > list.length) return false;
  list.splice(index, 0, id);
  return true;
}

/** Build, validate, and return a new candidate snapshot; rejection returns the exact input reference. */
export function planSectionOperation(snapshot: SectionDomainSnapshot, operation: SectionOperation): SectionPlanOutcome {
  const initialDiagnostic = validateSectionSnapshot(snapshot);
  if (initialDiagnostic) return reject(snapshot, initialDiagnostic);

  if (operation.kind === "create-section") {
    if (!isSectionUuid(operation.id)) return reject(snapshot, "SECTION_INVALID_UUID");
    if (snapshot.nodes[operation.id] || snapshot.pages[operation.id]) return reject(snapshot, "SECTION_NODE_ALREADY_EXISTS");
    const page = snapshot.pages[operation.pageId];
    if (!page) return reject(snapshot, "SECTION_PAGE_MISSING");
    if (operation.parentId !== null) {
      const parent = snapshot.nodes[operation.parentId];
      if (!parent) return reject(snapshot, "SECTION_PARENT_MISSING");
      if (parent.kind !== "section") return reject(snapshot, parent.kind === "frame" || parent.kind === "group" ? "SECTION_CONTAINMENT_FORBIDDEN" : "SECTION_RELATIONSHIP_UNQUALIFIED");
      if (parent.pageId !== operation.pageId) return reject(snapshot, "SECTION_CROSS_PAGE_OPERATION");
      if (isLockedPath(snapshot, parent.id)) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
    }
    if (!validBounds(operation.bounds)) return reject(snapshot, "SECTION_INVALID_BOUNDS");
    const out = structuredClone(snapshot);
    const section: SectionDomainNode = {
      id: operation.id,
      pageId: operation.pageId,
      kind: "section",
      parentId: operation.parentId,
      childIds: [],
      locked: operation.locked ?? false,
      visible: operation.visible ?? true,
      bounds: { ...operation.bounds },
    };
    const siblings = operation.parentId ? out.nodes[operation.parentId].childIds : out.pages[operation.pageId].childIds;
    if (!insertAt(siblings, section.id, operation.index)) return reject(snapshot, "SECTION_INVALID_ORDER_INDEX");
    out.nodes[section.id] = section;
    return finish(snapshot, out, [section.id]);
  }

  if (operation.kind === "insert-node") {
    const { node, parentId, index } = operation;
    if (!isSectionUuid(node.id)) return reject(snapshot, "SECTION_INVALID_UUID");
    if (snapshot.nodes[node.id] || snapshot.pages[node.id]) return reject(snapshot, "SECTION_NODE_ALREADY_EXISTS");
    if (node.childIds.length !== 0) return reject(snapshot, "SECTION_INSERT_REQUIRES_EMPTY_NODE");
    const parent = snapshot.nodes[parentId];
    const children = ownerChildren(snapshot, parentId);
    if (!parent || !children) return reject(snapshot, "SECTION_PARENT_MISSING");
    if (isLockedPath(snapshot, parentId)) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
    const relation = sectionContainment(parent.kind, node.kind);
    if (relation === "forbidden") return reject(snapshot, "SECTION_CONTAINMENT_FORBIDDEN");
    if (relation === "unqualified") return reject(snapshot, "SECTION_RELATIONSHIP_UNQUALIFIED");
    if (node.pageId !== parent.pageId) return reject(snapshot, "SECTION_CROSS_PAGE_OPERATION");
    if (!insertAt([...children], node.id, index)) return reject(snapshot, "SECTION_INVALID_ORDER_INDEX");
    const out = structuredClone(snapshot);
    out.nodes[parentId].childIds.splice(index, 0, node.id);
    out.nodes[node.id] = structuredClone({ ...node, parentId, childIds: [...node.childIds] });
    return finish(snapshot, out, [parentId, node.id]);
  }

  if (operation.kind === "reorder") {
    const node = snapshot.nodes[operation.id];
    if (!node) return reject(snapshot, "SECTION_TARGET_MISSING");
    if (isLockedPath(snapshot, node.id)) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
    const siblings = nodeSiblings(snapshot, node);
    if (!siblings) return reject(snapshot, "SECTION_PARENT_MISSING");
    const from = siblings.indexOf(node.id);
    if (!Number.isInteger(operation.index) || operation.index < 0 || operation.index >= siblings.length) return reject(snapshot, "SECTION_INVALID_ORDER_INDEX");
    const out = structuredClone(snapshot);
    const list = nodeSiblings(out, out.nodes[node.id])!;
    list.splice(from, 1);
    list.splice(operation.index, 0, node.id);
    return finish(snapshot, out, [node.parentId ?? node.pageId, node.id]);
  }

  if (operation.kind === "reparent") {
    const node = snapshot.nodes[operation.id];
    const parent = operation.parentId === null ? null : snapshot.nodes[operation.parentId];
    const destinationPage = operation.parentId === null
      ? snapshot.pages[operation.pageId ?? node?.pageId ?? ""]
      : null;
    const siblings = parent?.childIds ?? destinationPage?.childIds ?? null;
    if (!node || (operation.parentId !== null && !parent) || (!parent && !destinationPage)) return reject(snapshot, "SECTION_TARGET_MISSING");
    if (isLockedPath(snapshot, node.id) || (parent && isLockedPath(snapshot, parent.id))) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
    if (node.kind === "section" && parent && parent.kind !== "section") {
      return reject(snapshot, parent.kind === "frame" || parent.kind === "group" ? "SECTION_CONTAINMENT_FORBIDDEN" : "SECTION_RELATIONSHIP_UNQUALIFIED");
    }
    const relation = sectionContainment(parent?.kind ?? "page", node.kind);
    if (relation === "forbidden") return reject(snapshot, "SECTION_CONTAINMENT_FORBIDDEN");
    if (relation === "unqualified") return reject(snapshot, "SECTION_RELATIONSHIP_UNQUALIFIED");
    const parentPageId = parent?.pageId ?? destinationPage?.id;
    if (!siblings || parentPageId !== node.pageId) return reject(snapshot, "SECTION_CROSS_PAGE_OPERATION");
    let ancestor: string | null = parent?.id ?? null;
    while (ancestor) {
      if (ancestor === node.id) return reject(snapshot, "SECTION_CYCLE");
      ancestor = snapshot.nodes[ancestor]?.parentId ?? null;
    }
    const oldSiblings = nodeSiblings(snapshot, node);
    if (!oldSiblings) return reject(snapshot, "SECTION_PARENT_MISSING");
    const from = oldSiblings.indexOf(node.id);
    const insertionLimit = siblings === oldSiblings ? siblings.length - 1 : siblings.length;
    if (!Number.isInteger(operation.index) || operation.index < 0 || operation.index > insertionLimit) return reject(snapshot, "SECTION_INVALID_ORDER_INDEX");
    const out = structuredClone(snapshot);
    nodeSiblings(out, out.nodes[node.id])!.splice(from, 1);
    const outDestination = parent ? out.nodes[parent.id].childIds : out.pages[destinationPage!.id].childIds;
    outDestination.splice(operation.index, 0, node.id);
    out.nodes[node.id].parentId = parent?.id ?? null;
    return finish(snapshot, out, [node.parentId ?? node.pageId, parent?.id ?? destinationPage!.id, node.id]);
  }

  const node = snapshot.nodes[operation.id];
  if (!node) return reject(snapshot, "SECTION_TARGET_MISSING");
  if (node.kind !== "section") return reject(snapshot, "SECTION_TARGET_NOT_SECTION");
  if (isLockedPath(snapshot, node.id)) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
  const parentId = node.parentId;
  const siblings = nodeSiblings(snapshot, node);
  if (!siblings) return reject(snapshot, "SECTION_PARENT_MISSING");
  const subtree: string[] = [];
  const collect = (id: string) => {
    subtree.push(id);
    for (const childId of snapshot.nodes[id]?.childIds ?? []) collect(childId);
  };
  collect(node.id);
  if (subtree.some((id) => snapshot.nodes[id]?.locked)) return reject(snapshot, "SECTION_LOCKED_ANCESTOR");
  if (operation.contents === "delete") {
    // Figma documents deleting a Section with its contents. This operation
    // deletes every descendant scene node and Section as one candidate edit.
    const out = structuredClone(snapshot);
    nodeSiblings(out, out.nodes[node.id])!.splice(siblings.indexOf(node.id), 1);
    for (const id of subtree) delete out.nodes[id];
    return finish(snapshot, out, [parentId ?? node.pageId, ...subtree]);
  }
  // Preserve mode unwraps one Section by splicing its ordered children into
  // the former parent's exact sibling slot. Descendant nodes and IDs survive.
  const out = structuredClone(snapshot);
  const outSiblings = nodeSiblings(out, out.nodes[node.id])!;
  const slot = outSiblings.indexOf(node.id);
  outSiblings.splice(slot, 1, ...node.childIds);
  for (const childId of node.childIds) out.nodes[childId].parentId = parentId;
  delete out.nodes[node.id];
  return finish(snapshot, out, [parentId ?? node.pageId, ...node.childIds, node.id]);
}

function finish(before: SectionDomainSnapshot, after: SectionDomainSnapshot, changedIds: string[]): SectionPlanOutcome {
  const diagnostic = validateSectionSnapshot(after);
  if (diagnostic) return reject(before, diagnostic);
  return { ok: true, snapshot: after, changedIds: [...new Set(changedIds)].sort(), diagnostic: null };
}
