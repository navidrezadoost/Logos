import { buildSceneProjection, type SceneProjectionPlan } from "../render-wasm/scene-projection";
import { validateA9Capabilities } from "../render-wasm/scene";
import type { Page } from "./documentStore";
import type { Shape } from "../types/shapes";

export interface HierarchyDocumentSnapshot {
  readonly pages: Record<string, Page>;
  readonly currentPageId: string;
  readonly shapes: Record<string, Shape>;
  /** Current-page records found by the store's bidirectional page selector. */
  readonly currentPageShapes: readonly Shape[];
}

export type HierarchyOperation =
  | { readonly kind: "create-root"; readonly shape: Shape }
  | { readonly kind: "insert-child"; readonly parentId: string; readonly shape: Shape; readonly index?: number }
  | { readonly kind: "rename"; readonly shapeId: string; readonly name: string }
  | { readonly kind: "delete"; readonly shapeId: string }
  | { readonly kind: "reorder"; readonly shapeId: string; readonly toIndex: number }
  | { readonly kind: "reorder-root-list"; readonly rootShapeIds: readonly string[] }
  | { readonly kind: "reparent"; readonly shapeId: string; readonly parentId: string | null; readonly index?: number; readonly preserveWorldPosition: true };

export type HierarchyOperationResult =
  | {
      readonly ok: true;
      readonly pages: Record<string, Page>;
      readonly shapes: Record<string, Shape>;
      readonly pageId: string;
      readonly affectedIds: readonly string[];
      readonly projection: SceneProjectionPlan;
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
  const page = snapshot.pages[snapshot.currentPageId];
  if (!page) return fail("LOGOS_OPERATION_PAGE_UNAVAILABLE");

  const current = new Map(snapshot.currentPageShapes.map((shape) => [shape.id, { ...shape, children: Array.isArray(shape.children) ? [...shape.children] : shape.children }]));
  const originalProjection = buildSceneProjection(page, Object.fromEntries(current.entries()), page.rootShapeIds);
  if (!originalProjection.ok) {
    return fail(originalProjection.diagnostic.code, originalProjection.diagnostic.shapeId, originalProjection.diagnostic.relatedShapeId);
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
      if (shape.type !== "rect" && shape.type !== "frame") return fail("LOGOS_OPERATION_UNSUPPORTED_CREATE", shape.id);
      if (snapshot.shapes[shape.id]) return fail("LOGOS_OPERATION_ID_CONFLICT", shape.id);
      if (shape.parentId !== null || !Array.isArray(shape.children) || shape.children.length !== 0) return fail("LOGOS_OPERATION_CREATE_MUST_BE_EMPTY_ROOT", shape.id);
      next.set(shape.id, shape);
      roots.unshift(shape.id);
      affected.add(shape.id);
      break;
    }
    case "insert-child": {
      const parent = next.get(operation.parentId);
      const shape = operation.shape;
      if (!parent) return fail("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN", operation.parentId);
      if (parent.type !== "frame" && parent.type !== "group") return fail("LOGOS_OPERATION_PARENT_NOT_CONTAINER", parent.id);
      if (shape.type !== "rect") return fail("LOGOS_OPERATION_UNSUPPORTED_CHILD", shape.id);
      if (snapshot.shapes[shape.id]) return fail("LOGOS_OPERATION_ID_CONFLICT", shape.id);
      if (shape.parentId !== null || !Array.isArray(shape.children) || shape.children.length !== 0) return fail("LOGOS_OPERATION_CHILD_MUST_BE_EMPTY", shape.id);
      const locked = rejectLocked(parent);
      if (locked) return locked;
      const index = operation.index ?? 0;
      if (!validIndex(index, parent.children.length)) return fail("LOGOS_OPERATION_INVALID_ORDER_INDEX", shape.id);
      const children = [...parent.children];
      children.splice(index, 0, shape.id);
      next.set(parent.id, { ...parent, children });
      next.set(shape.id, { ...shape, parentId: parent.id });
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
  }

  const pageShapes = Object.fromEntries(next.entries());
  const candidatePage = { ...page, rootShapeIds: roots };
  const projectionResult = buildSceneProjection(candidatePage, pageShapes, roots);
  if (!projectionResult.ok) return fail(projectionResult.diagnostic.code, projectionResult.diagnostic.shapeId, projectionResult.diagnostic.relatedShapeId);
  const capabilityDiagnostic = validateA9Capabilities(projectionResult.plan);
  if (capabilityDiagnostic) return fail(capabilityDiagnostic.code, capabilityDiagnostic.shapeId);

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
    projection: projectionResult.plan,
  };
}

function validIndex(index: number, maximum: number): boolean {
  return Number.isInteger(index) && index >= 0 && index <= maximum;
}

function removeFrom(ids: string[], id: string): void {
  const index = ids.indexOf(id);
  if (index >= 0) ids.splice(index, 1);
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
