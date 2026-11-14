import type { Shape } from "../types/shapes";
import { WASM_SHAPE_TYPE_MAP } from "./shape-capabilities";

export const NIL_RENDERER_ROOT_ID = "00000000-0000-0000-0000-000000000000";

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export type SceneProjectionDiagnosticCode =
  | "LOGOS_SCENE_INVALID_UUID"
  | "LOGOS_SCENE_SHAPE_KEY_MISMATCH"
  | "LOGOS_SCENE_DUPLICATE_SHAPE_ID"
  | "LOGOS_SCENE_SYNTHETIC_ROOT_COLLISION"
  | "LOGOS_SCENE_ROOT_ORDER_MISMATCH"
  | "LOGOS_SCENE_DUPLICATE_ROOT"
  | "LOGOS_SCENE_MISSING_ROOT"
  | "LOGOS_SCENE_ROOT_PARENT_MISMATCH"
  | "LOGOS_SCENE_MISSING_PARENT"
  | "LOGOS_SCENE_MISSING_CHILD"
  | "LOGOS_SCENE_DUPLICATE_CHILD"
  | "LOGOS_SCENE_DUPLICATE_CHILD_MEMBERSHIP"
  | "LOGOS_SCENE_PARENT_CHILD_MISMATCH"
  | "LOGOS_SCENE_CYCLE"
  | "LOGOS_SCENE_ORPHAN_NODE"
  | "LOGOS_SCENE_UNSUPPORTED_SHAPE";

export interface SceneProjectionDiagnostic {
  readonly code: SceneProjectionDiagnosticCode;
  readonly pageId: string;
  readonly shapeId?: string;
  readonly relatedShapeId?: string;
  readonly shapeType?: string;
  readonly backend: "wasm";
}

export interface SceneProjectionNode {
  /** A nil UUID only for the renderer-owned synthetic root. */
  readonly id: string;
  readonly shape: Shape;
  readonly rendererType: number;
  /** Stored order consumed by the Rust traversal (back-to-front). */
  readonly rendererChildrenIds: readonly string[];
  readonly synthetic: boolean;
}

export interface SceneProjectionPlan {
  readonly pageId: string;
  readonly backend: "wasm";
  readonly rootId: typeof NIL_RENDERER_ROOT_ID;
  /** Input root IDs in document order (top-first). */
  readonly sourceRootIds: readonly string[];
  /** Serialized root children in Rust painter order (back-to-front). */
  readonly rendererRootChildrenIds: readonly string[];
  /** Synthetic root followed by a stable source-order preorder of page nodes. */
  readonly nodes: readonly SceneProjectionNode[];
}

export type SceneProjectionResult =
  | { readonly ok: true; readonly plan: SceneProjectionPlan }
  | { readonly ok: false; readonly diagnostic: SceneProjectionDiagnostic };

export interface SceneProjectionPage {
  readonly id: string;
  readonly rootShapeIds: readonly string[];
}

export type SceneShapeRecords = Readonly<Record<string, Shape>> | readonly Shape[];

/**
 * Build a renderer-only hierarchy plan. `shapeRecords` must be scoped to this
 * page; a global multi-page table cannot identify disconnected orphans because
 * Shape has no page-ownership field. The input objects are never mutated.
 */
export function buildSceneProjection(
  page: SceneProjectionPage,
  shapeRecords: SceneShapeRecords,
  orderedRootIds: readonly string[] = page.rootShapeIds,
): SceneProjectionResult {
  const fail = (
    code: SceneProjectionDiagnosticCode,
    details: Omit<SceneProjectionDiagnostic, "code" | "pageId" | "backend"> = {},
  ): SceneProjectionResult => ({
    ok: false,
    diagnostic: { code, pageId: page.id, backend: "wasm", ...details },
  });

  if (!sameIds(page.rootShapeIds, orderedRootIds)) {
    return fail("LOGOS_SCENE_ROOT_ORDER_MISMATCH");
  }

  const entries: Array<[string, Shape]> = Array.isArray(shapeRecords)
    ? (shapeRecords as readonly Shape[]).map((shape) => [shape.id, shape])
    : Object.entries(shapeRecords as Readonly<Record<string, Shape>>);
  entries.sort(([left], [right]) => left.localeCompare(right));
  const byId = new Map<string, Shape>();
  const canonicalShapeIds = new Set<string>();
  for (const [key, shape] of entries) {
    if (!shape || typeof shape.id !== "string" || !UUID_PATTERN.test(shape.id)) {
      return fail("LOGOS_SCENE_INVALID_UUID", { shapeId: shape?.id ?? key });
    }
    if (shape.id === NIL_RENDERER_ROOT_ID) {
      return fail("LOGOS_SCENE_SYNTHETIC_ROOT_COLLISION", { shapeId: shape.id });
    }
    if (key !== shape.id) {
      return fail("LOGOS_SCENE_SHAPE_KEY_MISMATCH", { shapeId: shape.id, relatedShapeId: key });
    }
    const canonicalId = shape.id.toLowerCase();
    if (canonicalShapeIds.has(canonicalId)) {
      return fail("LOGOS_SCENE_DUPLICATE_SHAPE_ID", { shapeId: shape.id, relatedShapeId: key });
    }
    canonicalShapeIds.add(canonicalId);
    byId.set(shape.id, shape);
  }

  const rootSet = new Set<string>();
  const canonicalRootSet = new Set<string>();
  for (const rootId of orderedRootIds) {
    if (!UUID_PATTERN.test(rootId)) return fail("LOGOS_SCENE_INVALID_UUID", { shapeId: rootId });
    if (rootId === NIL_RENDERER_ROOT_ID) {
      return fail("LOGOS_SCENE_SYNTHETIC_ROOT_COLLISION", { shapeId: rootId });
    }
    if (canonicalRootSet.has(rootId.toLowerCase())) return fail("LOGOS_SCENE_DUPLICATE_ROOT", { shapeId: rootId });
    rootSet.add(rootId);
    canonicalRootSet.add(rootId.toLowerCase());
    const root = byId.get(rootId);
    if (!root) return fail("LOGOS_SCENE_MISSING_ROOT", { shapeId: rootId });
    if (root.parentId !== null) {
      return fail("LOGOS_SCENE_ROOT_PARENT_MISMATCH", { shapeId: rootId, relatedShapeId: root.parentId });
    }
  }

  const childOwner = new Map<string, string>();
  for (const [shapeId, shape] of entries.map(([key]) => [key, byId.get(key)!] as const)) {
    if (!Array.isArray(shape.children)) {
      return fail("LOGOS_SCENE_PARENT_CHILD_MISMATCH", { shapeId });
    }
    if (shape.parentId !== null) {
      if (!UUID_PATTERN.test(shape.parentId)) {
        return fail("LOGOS_SCENE_INVALID_UUID", { shapeId, relatedShapeId: shape.parentId });
      }
      if (shape.parentId === NIL_RENDERER_ROOT_ID) {
        return fail("LOGOS_SCENE_SYNTHETIC_ROOT_COLLISION", { shapeId, relatedShapeId: shape.parentId });
      }
      if (!byId.has(shape.parentId)) {
        return fail("LOGOS_SCENE_MISSING_PARENT", { shapeId, relatedShapeId: shape.parentId });
      }
      if (rootSet.has(shapeId)) {
        return fail("LOGOS_SCENE_ROOT_PARENT_MISMATCH", { shapeId, relatedShapeId: shape.parentId });
      }
    } else if (!rootSet.has(shapeId)) {
      return fail("LOGOS_SCENE_ORPHAN_NODE", { shapeId });
    }

    const localChildren = new Set<string>();
    for (const childId of shape.children) {
      if (!UUID_PATTERN.test(childId)) {
        return fail("LOGOS_SCENE_INVALID_UUID", { shapeId, relatedShapeId: childId });
      }
      if (childId === shapeId) return fail("LOGOS_SCENE_CYCLE", { shapeId, relatedShapeId: childId });
      const canonicalChildId = childId.toLowerCase();
      if (localChildren.has(canonicalChildId)) {
        return fail("LOGOS_SCENE_DUPLICATE_CHILD", { shapeId, relatedShapeId: childId });
      }
      localChildren.add(canonicalChildId);
      if (!byId.has(childId)) return fail("LOGOS_SCENE_MISSING_CHILD", { shapeId, relatedShapeId: childId });
      const previousParent = childOwner.get(canonicalChildId);
      if (previousParent) {
        return fail("LOGOS_SCENE_DUPLICATE_CHILD_MEMBERSHIP", {
          shapeId: childId,
          relatedShapeId: previousParent,
        });
      }
      childOwner.set(canonicalChildId, shapeId);
      if (byId.get(childId)!.parentId !== shapeId) {
        return fail("LOGOS_SCENE_PARENT_CHILD_MISMATCH", { shapeId: childId, relatedShapeId: shapeId });
      }
    }
  }

  for (const [shapeId, shape] of byId) {
    if (shape.parentId !== null && childOwner.get(shapeId.toLowerCase()) !== shape.parentId) {
      return fail("LOGOS_SCENE_PARENT_CHILD_MISMATCH", { shapeId, relatedShapeId: shape.parentId });
    }
  }

  const cycle = findParentCycle(byId);
  if (cycle) return fail("LOGOS_SCENE_CYCLE", { shapeId: cycle[0], relatedShapeId: cycle[1] });

  const preorder: string[] = [];
  const reachable = new Set<string>();
  const stack = [...orderedRootIds].reverse();
  while (stack.length > 0) {
    const shapeId = stack.pop()!;
    if (reachable.has(shapeId)) continue;
    reachable.add(shapeId);
    preorder.push(shapeId);
    const children = byId.get(shapeId)!.children;
    for (let index = children.length - 1; index >= 0; index -= 1) stack.push(children[index]);
  }
  const orphan = [...byId.keys()].sort().find((shapeId) => !reachable.has(shapeId));
  if (orphan) return fail("LOGOS_SCENE_ORPHAN_NODE", { shapeId: orphan });

  const rustTypes = new Map<string, number>();
  for (const shapeId of preorder) {
    const shape = byId.get(shapeId)!;
    if (!Object.prototype.hasOwnProperty.call(WASM_SHAPE_TYPE_MAP, shape.type)) {
      return fail("LOGOS_SCENE_UNSUPPORTED_SHAPE", { shapeId, shapeType: String(shape.type) });
    }
    rustTypes.set(shapeId, WASM_SHAPE_TYPE_MAP[shape.type]);
  }

  const rendererRootChildrenIds = reverseCopy(orderedRootIds);
  const rootShape = makeSyntheticRoot(rendererRootChildrenIds);
  const nodes: SceneProjectionNode[] = [{
    id: NIL_RENDERER_ROOT_ID,
    shape: rootShape,
    rendererType: WASM_SHAPE_TYPE_MAP.frame,
    rendererChildrenIds: rendererRootChildrenIds,
    synthetic: true,
  }];
  for (const shapeId of preorder) {
    const source = byId.get(shapeId)!;
    const rendererChildrenIds = reverseCopy(source.children);
    nodes.push({
      id: shapeId,
      shape: snapshotShape(source),
      rendererType: rustTypes.get(shapeId)!,
      rendererChildrenIds,
      synthetic: false,
    });
  }

  return {
    ok: true,
    plan: {
      pageId: page.id,
      backend: "wasm",
      rootId: NIL_RENDERER_ROOT_ID,
      sourceRootIds: [...orderedRootIds],
      rendererRootChildrenIds,
      nodes,
    },
  };
}

function sameIds(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((id, index) => id === right[index]);
}

function reverseCopy(ids: readonly string[]): string[] {
  return [...ids].reverse();
}

function findParentCycle(byId: ReadonlyMap<string, Shape>): [string, string] | null {
  const complete = new Set<string>();
  for (const startId of [...byId.keys()].sort()) {
    if (complete.has(startId)) continue;
    const path: string[] = [];
    const pathPositions = new Map<string, number>();
    let cursor: string | null = startId;
    while (cursor !== null && !complete.has(cursor)) {
      const previousPosition = pathPositions.get(cursor);
      if (previousPosition !== undefined) return [cursor, path[previousPosition + 1] ?? cursor];
      pathPositions.set(cursor, path.length);
      path.push(cursor);
      cursor = byId.get(cursor)!.parentId;
    }
    for (const id of path) complete.add(id);
  }
  return null;
}

function snapshotShape(source: Shape): Shape {
  return structuredClone(source);
}

function makeSyntheticRoot(children: readonly string[]): Shape {
  return {
    id: NIL_RENDERER_ROOT_ID,
    type: "frame",
    name: "Renderer synthetic root",
    bounds: { x: 0, y: 0, w: 0, h: 0 },
    transform: [1, 0, 0, 1, 0, 0],
    rotation: 0,
    fills: [],
    opacity: 1,
    hidden: false,
    locked: false,
    parentId: null,
    children: [...children],
  };
}
