import { buildSceneProjection, type SceneProjectionPlan } from "../render-wasm/scene-projection";
import { validateA9Capabilities } from "../render-wasm/scene";
import type { Page } from "./documentStore";
import type { ComponentMeta, Shape } from "../types/shapes";

export interface HierarchyDocumentSnapshot {
  readonly pages: Record<string, Page>;
  readonly currentPageId: string;
  readonly shapes: Record<string, Shape>;
  /** Current-page records found by the store's bidirectional page selector. */
  readonly currentPageShapes: readonly Shape[];
}

export interface RendererCompatibilityDiagnostic {
  readonly code: string;
  readonly shapeId?: string;
  readonly shapeType?: string;
}

export type HierarchyOperation =
  | { readonly kind: "create-root"; readonly shape: Shape }
  | { readonly kind: "insert-child"; readonly parentId: string; readonly shape: Shape; readonly index?: number }
  | { readonly kind: "rename"; readonly shapeId: string; readonly name: string }
  | { readonly kind: "delete"; readonly shapeId: string }
  | { readonly kind: "reorder"; readonly shapeId: string; readonly toIndex: number }
  | { readonly kind: "reorder-root-list"; readonly rootShapeIds: readonly string[] }
  | { readonly kind: "reparent"; readonly shapeId: string; readonly parentId: string | null; readonly index?: number; readonly preserveWorldPosition: true }
  | { readonly kind: "compat-add-shape"; readonly shape: Shape }
  | { readonly kind: "compat-add-shapes"; readonly shapes: readonly Shape[] }
  | { readonly kind: "compat-delete"; readonly shapeId: string }
  | { readonly kind: "compat-promote-component"; readonly shapeId: string; readonly meta: ComponentMeta }
  | { readonly kind: "compat-replace-vector-network"; readonly removeIds: readonly string[]; readonly shape: Shape };

export type HierarchyOperationResult =
  | {
      readonly ok: true;
      readonly pages: Record<string, Page>;
      readonly shapes: Record<string, Shape>;
      readonly pageId: string;
      readonly affectedIds: readonly string[];
      readonly projection: SceneProjectionPlan | null;
      /** Preserved visual content is structurally sound but outside the verified A9 renderer contract. */
      readonly renderingDiagnostic?: RendererCompatibilityDiagnostic;
      /** Set when a replacement should become the user's reconciled selection. */
      readonly replacementShapeId?: string;
    }
  | {
      readonly ok: false;
      readonly diagnostic: { readonly code: string; readonly shapeId?: string; readonly relatedShapeId?: string };
    };

const fail = (code: string, shapeId?: string, relatedShapeId?: string): HierarchyOperationResult => ({
  ok: false,
  diagnostic: { code, ...(shapeId ? { shapeId } : {}), ...(relatedShapeId ? { relatedShapeId } : {}) },
});

/** Plan a supported hierarchy change without mutating the supplied snapshot. */
export function planHierarchyOperation(
  snapshot: HierarchyDocumentSnapshot,
  operation: HierarchyOperation,
): HierarchyOperationResult {
  const compatibilityMode = operation.kind.startsWith("compat-");
  const page = snapshot.pages[snapshot.currentPageId];
  if (!page) return fail("LOGOS_OPERATION_PAGE_UNAVAILABLE");

  const current = new Map(snapshot.currentPageShapes.map((shape) => [shape.id, cloneShape(shape)]));
  const originalProjection = buildSceneProjection(page, Object.fromEntries(current.entries()), page.rootShapeIds);
  let renderingDiagnostic: RendererCompatibilityDiagnostic | undefined;
  if (originalProjection.ok) {
    renderingDiagnostic = validateA9Capabilities(originalProjection.plan) ?? undefined;
  } else if (compatibilityMode && originalProjection.diagnostic.code === "LOGOS_SCENE_UNSUPPORTED_SHAPE") {
    // The scene projector validates the complete graph before checking type-map support.
    // Keeping this content is a documented compatibility operation; Canvas will
    // publish its ordinary unsupported-shape diagnostic after the atomic commit.
    renderingDiagnostic = originalProjection.diagnostic;
  } else {
    return fail(originalProjection.diagnostic.code, originalProjection.diagnostic.shapeId, originalProjection.diagnostic.relatedShapeId);
  }
  if (!compatibilityMode && renderingDiagnostic) {
    return fail(renderingDiagnostic.code, renderingDiagnostic.shapeId);
  }
  const next = new Map(current);
  const roots = [...page.rootShapeIds];
  const affected = new Set<string>();
  const rejectLocked = (shape: Shape | undefined): HierarchyOperationResult | null =>
    shape?.locked ? fail("LOGOS_OPERATION_LOCKED_SHAPE", shape.id) : null;

  const siblingsFor = (parentId: string | null): string[] | null => {
    if (parentId === null) return roots;
    const parent = next.get(parentId);
    return parent ? parent.children : null;
  };

  switch (operation.kind) {
    case "create-root": {
      const shape = operation.shape;
      if (!shape || typeof shape !== "object" || typeof shape.id !== "string") return fail("LOGOS_OPERATION_INVALID_SHAPE");
      if (shape.type !== "rect" && shape.type !== "frame") return fail("LOGOS_OPERATION_UNSUPPORTED_CREATE", shape.id);
      if (hasCanonicalId(snapshot.shapes, shape.id)) return fail("LOGOS_OPERATION_ID_CONFLICT", shape.id);
      if (shape.parentId !== null || !Array.isArray(shape.children) || shape.children.length !== 0) return fail("LOGOS_OPERATION_CREATE_MUST_BE_EMPTY_ROOT", shape.id);
      next.set(shape.id, cloneShape(shape));
      roots.unshift(shape.id);
      affected.add(shape.id);
      break;
    }
    case "insert-child": {
      const parent = next.get(operation.parentId);
      const shape = operation.shape;
      if (!shape || typeof shape !== "object" || typeof shape.id !== "string") return fail("LOGOS_OPERATION_INVALID_SHAPE");
      if (!parent) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.parentId);
      if (parent.type !== "frame" && parent.type !== "group") return fail("LOGOS_OPERATION_PARENT_NOT_CONTAINER", parent.id);
      if (shape.type !== "rect") return fail("LOGOS_OPERATION_UNSUPPORTED_CHILD", shape.id);
      if (hasCanonicalId(snapshot.shapes, shape.id)) return fail("LOGOS_OPERATION_ID_CONFLICT", shape.id);
      if (shape.parentId !== null || !Array.isArray(shape.children) || shape.children.length !== 0) return fail("LOGOS_OPERATION_CHILD_MUST_BE_EMPTY", shape.id);
      const locked = rejectLocked(parent);
      if (locked) return locked;
      const index = operation.index ?? 0;
      if (!validIndex(index, parent.children.length)) return fail("LOGOS_OPERATION_INVALID_ORDER_INDEX", shape.id);
      const children = [...parent.children];
      children.splice(index, 0, shape.id);
      next.set(parent.id, { ...parent, children });
      next.set(shape.id, { ...cloneShape(shape), parentId: parent.id });
      affected.add(parent.id);
      affected.add(shape.id);
      break;
    }
    case "rename": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      const locked = rejectLocked(shape);
      if (locked) return locked;
      const name = operation.name.trim();
      if (!name || name.length > 128 || /[\u0000-\u001f\u007f]/.test(name)) return fail("LOGOS_OPERATION_INVALID_NAME", shape.id);
      next.set(shape.id, { ...shape, name });
      affected.add(shape.id);
      break;
    }
    case "delete": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      const removed = collectSubtree(next, shape.id);
      for (const id of removed) {
        const locked = rejectLocked(next.get(id));
        if (locked) return locked;
      }
      const siblings = siblingsFor(shape.parentId);
      if (!siblings) return fail("LOGOS_OPERATION_PARENT_MISSING", shape.id, shape.parentId ?? undefined);
      const parentLocked = rejectLocked(shape.parentId ? next.get(shape.parentId) : undefined);
      if (parentLocked) return parentLocked;
      removeFrom(siblings, shape.id);
      for (const id of removed) {
        next.delete(id);
        affected.add(id);
      }
      if (shape.parentId) affected.add(shape.parentId);
      break;
    }
    case "reorder": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      const locked = rejectLocked(shape);
      if (locked) return locked;
      const siblings = siblingsFor(shape.parentId);
      if (!siblings || !siblings.includes(shape.id)) return fail("LOGOS_OPERATION_PARENT_CHILD_MISMATCH", shape.id, shape.parentId ?? undefined);
      const parentLocked = rejectLocked(shape.parentId ? next.get(shape.parentId) : undefined);
      if (parentLocked) return parentLocked;
      if (!validIndex(operation.toIndex, siblings.length - 1)) return fail("LOGOS_OPERATION_INVALID_ORDER_INDEX", shape.id);
      removeFrom(siblings, shape.id);
      siblings.splice(operation.toIndex, 0, shape.id);
      affected.add(shape.parentId ?? page.id);
      affected.add(shape.id);
      break;
    }
    case "reorder-root-list": {
      const proposed = [...operation.rootShapeIds];
      if (proposed.length !== roots.length || new Set(proposed).size !== roots.length || proposed.some((id) => !roots.includes(id))) {
        return fail("LOGOS_OPERATION_ROOT_ORDER_MEMBERSHIP_MISMATCH");
      }
      for (const id of proposed) {
        const locked = rejectLocked(next.get(id));
        if (locked) return locked;
      }
      roots.splice(0, roots.length, ...proposed);
      affected.add(page.id);
      for (const id of proposed) affected.add(id);
      break;
    }
    case "reparent": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      const locked = rejectLocked(shape);
      if (locked) return locked;
      if (operation.preserveWorldPosition !== true) return fail("LOGOS_OPERATION_WORLD_POSITION_REQUIRED", shape.id);
      if (operation.parentId === shape.id || (operation.parentId && collectSubtree(next, shape.id).has(operation.parentId))) {
        return fail("LOGOS_OPERATION_CYCLE", shape.id, operation.parentId ?? undefined);
      }
      const target = operation.parentId === null ? null : next.get(operation.parentId);
      if (operation.parentId !== null && !target) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.parentId);
      if (target && target.type !== "frame" && target.type !== "group") return fail("LOGOS_OPERATION_PARENT_NOT_CONTAINER", target.id);
      const targetLocked = rejectLocked(target ?? undefined);
      if (targetLocked) return targetLocked;
      const sourceParentLocked = rejectLocked(shape.parentId ? next.get(shape.parentId) : undefined);
      if (sourceParentLocked) return sourceParentLocked;
      const sourceSiblings = siblingsFor(shape.parentId);
      const targetSiblings = siblingsFor(operation.parentId);
      if (!sourceSiblings?.includes(shape.id) || !targetSiblings) return fail("LOGOS_OPERATION_PARENT_CHILD_MISMATCH", shape.id, shape.parentId ?? undefined);
      const index = operation.index ?? 0;
      const maximum = targetSiblings.length - (sourceSiblings === targetSiblings ? 1 : 0);
      if (!validIndex(index, maximum)) return fail("LOGOS_OPERATION_INVALID_ORDER_INDEX", shape.id);
      removeFrom(sourceSiblings, shape.id);
      targetSiblings.splice(index, 0, shape.id);
      next.set(shape.id, { ...shape, parentId: operation.parentId });
      affected.add(shape.id);
      if (shape.parentId) affected.add(shape.parentId);
      if (operation.parentId) affected.add(operation.parentId);
      break;
    }
    case "compat-add-shape": {
      if (!operation.shape || typeof operation.shape !== "object" || typeof operation.shape.id !== "string") return fail("LOGOS_OPERATION_INVALID_SHAPE");
      const incoming = cloneShape(operation.shape);
      if (hasCanonicalId(snapshot.shapes, incoming.id)) return fail("LOGOS_OPERATION_ID_CONFLICT", incoming.id);
      if (!Array.isArray(incoming.children) || incoming.children.length !== 0) {
        return fail("LOGOS_COMPAT_ADD_REQUIRES_EMPTY_CHILD_LIST", incoming.id);
      }
      if (incoming.parentId === null) {
        roots.unshift(incoming.id);
      } else {
        const parent = next.get(incoming.parentId);
        if (!parent) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", incoming.parentId);
        const locked = rejectLocked(parent);
        if (locked) return locked;
        next.set(parent.id, { ...parent, children: [...parent.children, incoming.id] });
      }
      next.set(incoming.id, incoming);
      affected.add(incoming.id);
      if (incoming.parentId) affected.add(incoming.parentId);
      break;
    }
    case "compat-add-shapes": {
      if (!Array.isArray(operation.shapes) || operation.shapes.length === 0) return fail("LOGOS_COMPAT_ADD_REQUIRES_SHAPES");
      const incomingIds = new Set<string>();
      const incomingShapes: Shape[] = [];
      for (const value of operation.shapes) {
        if (!value || typeof value !== "object" || typeof value.id !== "string") return fail("LOGOS_OPERATION_INVALID_SHAPE");
        const incoming = cloneShape(value);
        const canonicalId = incoming.id.toLowerCase();
        if (hasCanonicalId(snapshot.shapes, incoming.id) || incomingIds.has(canonicalId)) return fail("LOGOS_OPERATION_ID_CONFLICT", incoming.id);
        if (!Array.isArray(incoming.children) || incoming.children.length !== 0) return fail("LOGOS_COMPAT_ADD_REQUIRES_EMPTY_CHILD_LIST", incoming.id);
        incomingIds.add(canonicalId);
        incomingShapes.push(incoming);
        next.set(incoming.id, incoming);
      }
      for (const incoming of incomingShapes) {
        if (incoming.parentId === null) {
          roots.unshift(incoming.id);
        } else {
          const parent = next.get(incoming.parentId);
          if (!parent || parent.id === incoming.id) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", incoming.parentId);
          const locked = rejectLocked(parent);
          if (locked) return locked;
          next.set(parent.id, { ...parent, children: [...parent.children, incoming.id] });
          affected.add(parent.id);
        }
        affected.add(incoming.id);
      }
      break;
    }
    case "compat-delete": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      const removed = collectSubtree(next, shape.id);
      for (const id of removed) {
        const locked = rejectLocked(next.get(id));
        if (locked) return locked;
      }
      const siblings = siblingsFor(shape.parentId);
      if (!siblings) return fail("LOGOS_OPERATION_PARENT_MISSING", shape.id, shape.parentId ?? undefined);
      const parentLocked = rejectLocked(shape.parentId ? next.get(shape.parentId) : undefined);
      if (parentLocked) return parentLocked;
      removeFrom(siblings, shape.id);
      for (const id of removed) {
        next.delete(id);
        affected.add(id);
      }
      if (shape.parentId) affected.add(shape.parentId);
      break;
    }
    case "compat-promote-component": {
      const shape = next.get(operation.shapeId);
      if (!shape) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.shapeId);
      if (shape.type !== "frame" && shape.type !== "group") return fail("LOGOS_COMPAT_PROMOTION_REQUIRES_CONTAINER", shape.id);
      const locked = rejectLocked(shape);
      if (locked) return locked;
      next.set(shape.id, { ...shape, type: "component", componentMeta: clonePlainData(operation.meta) });
      affected.add(shape.id);
      break;
    }
    case "compat-replace-vector-network": {
      if (!operation.shape || typeof operation.shape !== "object" || typeof operation.shape.id !== "string" || !Array.isArray(operation.removeIds)) {
        return fail("LOGOS_COMPAT_REPLACEMENT_INVALID_SHAPE");
      }
      const replacement = cloneShape(operation.shape);
      const removeIds = [...operation.removeIds];
      if (replacement.type !== "vector-network" || replacement.parentId !== null || replacement.children.length !== 0) {
        return fail("LOGOS_COMPAT_REPLACEMENT_INVALID_SHAPE", replacement.id);
      }
      if (hasCanonicalId(snapshot.shapes, replacement.id)) return fail("LOGOS_OPERATION_ID_CONFLICT", replacement.id);
      if (removeIds.length < 1 || new Set(removeIds).size !== removeIds.length) {
        return fail("LOGOS_COMPAT_REPLACEMENT_INVALID_SELECTION");
      }
      const selected = removeIds.map((id) => next.get(id));
      if (selected.some((shape) => !shape)) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN");
      const targets = selected as Shape[];
      if (targets.some((shape) => shape.type !== "vector-network")) return fail("LOGOS_COMPAT_REPLACEMENT_REQUIRES_VECTOR_NETWORKS");
      for (const shape of targets) {
        const locked = rejectLocked(shape);
        if (locked) return locked;
      }
      const parentId = targets[0].parentId;
      if (targets.some((shape) => shape.parentId !== parentId)) return fail("LOGOS_COMPAT_REPLACEMENT_AMBIGUOUS_PARENT");
      for (const shape of targets) {
        if (collectSubtree(next, shape.id).size > 1) return fail("LOGOS_COMPAT_REPLACEMENT_HAS_REFERENCED_DESCENDANTS", shape.id);
      }
      const parentLocked = rejectLocked(parentId ? next.get(parentId) : undefined);
      if (parentLocked) return parentLocked;
      const siblings = siblingsFor(parentId);
      if (!siblings) return fail("LOGOS_OPERATION_PARENT_MISSING", targets[0].id, parentId ?? undefined);
      const positions = removeIds.map((id) => siblings.indexOf(id));
      if (positions.some((index) => index < 0)) return fail("LOGOS_OPERATION_PARENT_CHILD_MISMATCH", targets[0].id);
      const insertionIndex = Math.min(...positions);
      for (const id of removeIds) removeFrom(siblings, id);
      siblings.splice(insertionIndex, 0, replacement.id);
      if (parentId !== null) replacement.parentId = parentId;
      for (const id of removeIds) {
        next.delete(id);
        affected.add(id);
      }
      next.set(replacement.id, replacement);
      for (const id of [...affected]) {
        const descendants = collectSubtree(current, id);
        for (const descendant of descendants) {
          if (!next.has(descendant)) affected.add(descendant);
          next.delete(descendant);
        }
      }
      if (parentId) {
        const parent = next.get(parentId);
        if (!parent) return fail("LOGOS_OPERATION_PARENT_MISSING", replacement.id, parentId);
        // Preserve the one authoritative sibling ordering while replacing IDs.
        next.set(parentId, { ...parent, children: siblings });
        affected.add(parentId);
      }
      affected.add(replacement.id);
      return finalizePlan(snapshot, page, current, next, roots, affected, compatibilityMode, renderingDiagnostic, replacement.id);
    }
  }
  return finalizePlan(snapshot, page, current, next, roots, affected, compatibilityMode, renderingDiagnostic);
}

function finalizePlan(
  snapshot: HierarchyDocumentSnapshot,
  page: Page,
  current: ReadonlyMap<string, Shape>,
  next: Map<string, Shape>,
  roots: string[],
  affected: Set<string>,
  compatibilityMode: boolean,
  priorRenderingDiagnostic?: RendererCompatibilityDiagnostic,
  replacementShapeId?: string,
): HierarchyOperationResult {
  const pageShapes = Object.fromEntries(next.entries());
  const candidatePage = { ...page, rootShapeIds: roots };
  const projectionResult = buildSceneProjection(candidatePage, pageShapes, roots);
  let projection: SceneProjectionPlan | null = null;
  let renderingDiagnostic = priorRenderingDiagnostic;
  if (projectionResult.ok) {
    projection = projectionResult.plan;
    const capabilityDiagnostic = validateA9Capabilities(projectionResult.plan);
    if (capabilityDiagnostic) {
      if (!compatibilityMode) return fail(capabilityDiagnostic.code, capabilityDiagnostic.shapeId);
      renderingDiagnostic ??= capabilityDiagnostic;
    }
  } else if (compatibilityMode && projectionResult.diagnostic.code === "LOGOS_SCENE_UNSUPPORTED_SHAPE") {
    renderingDiagnostic ??= projectionResult.diagnostic;
  } else {
    return fail(projectionResult.diagnostic.code, projectionResult.diagnostic.shapeId, projectionResult.diagnostic.relatedShapeId);
  }

  const candidateShapes = { ...snapshot.shapes };
  for (const id of current.keys()) {
    if (!next.has(id)) delete candidateShapes[id];
  }
  Object.assign(candidateShapes, pageShapes);
  return {
    ok: true,
    pages: { ...snapshot.pages, [page.id]: candidatePage },
    shapes: candidateShapes,
    pageId: page.id,
    affectedIds: [...affected],
    projection,
    ...(renderingDiagnostic ? { renderingDiagnostic } : {}),
    ...(replacementShapeId ? { replacementShapeId } : {}),
  };
}

function cloneShape(shape: Shape): Shape {
  return clonePlainData(shape);
}

function clonePlainData<T>(value: T, seen = new WeakMap<object, unknown>()): T {
  if (value === null || typeof value !== "object") return value;
  const cached = seen.get(value as object);
  if (cached) return cached as T;
  if (Array.isArray(value)) {
    const result: unknown[] = [];
    seen.set(value, result);
    for (const item of value) result.push(clonePlainData(item, seen));
    return result as T;
  }
  const result: Record<string, unknown> = {};
  seen.set(value as object, result);
  for (const [key, item] of Object.entries(value as Record<string, unknown>)) result[key] = clonePlainData(item, seen);
  return result as T;
}

function validIndex(index: number, maximum: number): boolean {
  return Number.isInteger(index) && index >= 0 && index <= maximum;
}

function removeFrom(ids: string[], id: string): void {
  const index = ids.indexOf(id);
  if (index >= 0) ids.splice(index, 1);
}

function hasCanonicalId(shapes: Readonly<Record<string, Shape>>, id: string): boolean {
  const canonical = id.toLowerCase();
  return Object.values(shapes).some((shape) => shape.id.toLowerCase() === canonical);
}

function collectSubtree(shapes: ReadonlyMap<string, Shape>, rootId: string): Set<string> {
  const result = new Set<string>();
  const pending = [rootId];
  while (pending.length > 0) {
    const id = pending.pop()!;
    if (result.has(id)) continue;
    result.add(id);
    const shape = shapes.get(id);
    if (shape) pending.push(...shape.children);
  }
  return result;
}
