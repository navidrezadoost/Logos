/**
 * stores/documentStore.ts
 *
 * Thin projection of the document state that the React shell needs:
 *  - Shape metadata (id, name, type, bounds, fills, …)
 *  - Page list and current page
 *
 * Heavy geometry stays in the Rust scene graph; these records are the
 * "React shadow" that drives the layers panel, inspector, and toolbar.
 */

import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { useMemo } from "react";
import { nanoid } from "nanoid";
import { type Shape, type Rect, type VNAnchor, type VNSegment, type VNRegion, type ComponentMeta, type InstanceMeta, createRect, createVectorNetwork, IDENTITY_TRANSFORM } from "../types/shapes";
import { useSelectionStore } from "./selectionStore";
import { planHierarchyOperation, type HierarchyOperation, type HierarchyOperationResult } from "./hierarchyOperations";

/** Generate a proper UUID v4 for shapes — required by the WASM bridge's uuidToU32x4(). */
const shapeId = (): string => crypto.randomUUID();

// ─────────────────────────────────────────────────────────────────────────────
// Page
// ─────────────────────────────────────────────────────────────────────────────

export interface Page {
  id: string;
  name: string;
  /** Ordered list of top-level shape IDs on this page. */
  rootShapeIds: string[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Store shape
// ─────────────────────────────────────────────────────────────────────────────

interface DocumentState {
  // Pages
  pages: Record<string, Page>;
  pageOrder: string[];
  currentPageId: string;

  // Shapes (all pages merged, keyed by id)
  shapes: Record<string, Shape>;

  // ── Actions ────────────────────────────────────────────────────────────────

  /** Add a page; returns the new page's id. */
  addPage: (name?: string) => string;
  setCurrentPage: (pageId: string) => void;

  /** Add a shape to the current page; returns the new shape id. */
  addRect: (bounds: Rect, color?: string) => string;

  /** Add any pre-built shape to the current page; returns the shape id. */
  addShape: (shape: Shape) => string;

  /** Commit a completed pen session as a vector-network shape. */
  addVectorNetwork: (
    anchors: VNAnchor[],
    segments: VNSegment[],
    regions?: VNRegion[],
    fill?: string
  ) => string;

  /**
   * Replace two existing shapes with one new vector-network shape
   * (used after a boolean op merges two shapes).
   */
  replaceWithVectorNetwork: (
    removeIds: string[],
    anchors: VNAnchor[],
    segments: VNSegment[],
    regions?: VNRegion[]
  ) => string;

  /** Update mutable display properties (name, color, opacity…). */
  updateShape: (id: string, patch: Partial<Shape>) => void;

  /** Apply multiple shape patches atomically (Worker result). */
  batchUpdate: (patches: Record<string, Partial<Shape>>) => void;

  /**
   * Remove a shape subtree and unlink it from its page / parent.
   */
  removeShape: (id: string) => HierarchyOperationResult;

  /** Re-order shapes within a page (drag-and-drop in layers panel). */
  reorderPageShapes: (pageId: string, newOrder: string[]) => HierarchyOperationResult;

  /** Validate and atomically commit a supported current-page hierarchy operation. */
  applyHierarchyOperation: (operation: HierarchyOperation) => HierarchyOperationResult;
  /** Store-internal variant used by existing page-addressed root-order APIs. */
  applyHierarchyOperationForPage: (pageId: string, operation: HierarchyOperation) => HierarchyOperationResult;

  // ── P4.4: Component Variants ──────────────────────────────────────────────

  /**
   * Convert an existing frame/group shape into a component master.
   * The shape's type changes to "component" and componentMeta is set.
   * Returns a snapshot of the shape as it was before promotion (so the
   * caller can register defaults in componentStore).
   */
  promoteToComponent: (shapeId: string, meta: ComponentMeta) => Shape | null;

  /**
   * Add a new "instance" shell shape to the current page.
   * The caller must also call componentStore.createInstance() to register
   * the instance metadata and get back the InstanceMeta to attach here.
   */
  addInstanceShape: (
    instanceId: string,
    componentName: string,
    bounds: Rect,
    instanceMeta: InstanceMeta
  ) => string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Initial state — one page, no shapes
// ─────────────────────────────────────────────────────────────────────────────

const PAGE_1_ID = nanoid();

const INITIAL_PAGE: Page = {
  id: PAGE_1_ID,
  name: "Page 1",
  rootShapeIds: [],
};

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export const useDocumentStore = create<DocumentState>((set, get) => ({
  pages: { [PAGE_1_ID]: INITIAL_PAGE },
  pageOrder: [PAGE_1_ID],
  currentPageId: PAGE_1_ID,
  shapes: {},

  // ── Pages ─────────────────────────────────────────────────────────────────

  addPage: (name) => {
    const id = nanoid();
    const page: Page = { id, name: name ?? `Page ${get().pageOrder.length + 1}`, rootShapeIds: [] };
    set((s) => ({
      pages: { ...s.pages, [id]: page },
      pageOrder: [...s.pageOrder, id],
    }));
    return id;
  },

  setCurrentPage: (pageId) => {
    if (get().pages[pageId]) set({ currentPageId: pageId });
  },

  // ── Shapes ────────────────────────────────────────────────────────────────

  addRect: (bounds, color = "#0000ff") => {
    const id = shapeId(); // crypto.randomUUID() — hex format required by WASM bridge
    const name = `Rectangle ${Object.values(get().shapes).filter((s) => s.type === "rect").length + 1}`;
    const shape = createRect(id, name, bounds, color);

    const result = get().applyHierarchyOperation({ kind: "create-root", shape });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return id;
  },

  addShape: (shape) => {
    if (shape.type === "rect" || shape.type === "frame") {
      const operation: HierarchyOperation = shape.parentId === null
        ? { kind: "create-root", shape }
        : { kind: "insert-child", parentId: shape.parentId, shape, index: 0 };
      const result = get().applyHierarchyOperation(operation);
      if (!result.ok) throw new Error(result.diagnostic.code);
      return shape.id;
    }
    const { currentPageId: pid } = get();
    set((s) => ({
      shapes: { ...s.shapes, [shape.id]: shape },
      pages: {
        ...s.pages,
        [pid]: {
          ...s.pages[pid],
          rootShapeIds: [shape.id, ...s.pages[pid].rootShapeIds],
        },
      },
    }));
    return shape.id;
  },

  addVectorNetwork: (anchors, segments, regions = [], fill = "#6c9ef8") => {
    const id = shapeId();
    const { currentPageId: pid } = get();
    const count = Object.values(get().shapes).filter((s) => s.type === "vector-network").length + 1;
    const shape = createVectorNetwork(id, `Path ${count}`, anchors, segments, regions, fill);
    set((s) => ({
      shapes: { ...s.shapes, [id]: shape },
      pages: {
        ...s.pages,
        [pid]: {
          ...s.pages[pid],
          rootShapeIds: [id, ...s.pages[pid].rootShapeIds],
        },
      },
    }));
    return id;
  },

  replaceWithVectorNetwork: (removeIds, anchors, segments, regions = []) => {
    const id = shapeId();
    const { currentPageId: pid } = get();
    const count = Object.values(get().shapes).filter((s) => s.type === "vector-network").length + 1;
    const shape = createVectorNetwork(id, `Path ${count}`, anchors, segments, regions);
    set((s) => {
      const nextShapes = { ...s.shapes };
      for (const rid of removeIds) delete nextShapes[rid];
      nextShapes[id] = shape;
      const nextPages = { ...s.pages };
      nextPages[pid] = {
        ...nextPages[pid],
        rootShapeIds: [
          id,
          ...nextPages[pid].rootShapeIds.filter((sid) => !removeIds.includes(sid)),
        ],
      };
      return { shapes: nextShapes, pages: nextPages };
    });
    return id;
  },

  updateShape: (id, patch) => {
    if (Object.prototype.hasOwnProperty.call(patch, "parentId") || Object.prototype.hasOwnProperty.call(patch, "children")) {
      throw new Error("LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY");
    }
    set((s) => ({
      shapes: { ...s.shapes, [id]: { ...s.shapes[id], ...patch } },
    }));
  },

  batchUpdate: (patches) => {
    if (Object.values(patches).some((patch) => Object.prototype.hasOwnProperty.call(patch, "parentId") || Object.prototype.hasOwnProperty.call(patch, "children"))) {
      throw new Error("LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY");
    }
    set((s) => {
      const next = { ...s.shapes };
      for (const [id, patch] of Object.entries(patches)) {
        if (next[id]) next[id] = { ...next[id], ...patch };
      }
      return { shapes: next };
    });
  },

  removeShape: (id) => {
    const snapshot = get();
    const owners = Object.keys(snapshot.pages).filter((pageId) => {
      const [, ...scene] = selectCurrentPageScene({ ...snapshot, currentPageId: pageId });
      return scene.some((shape) => shape.id === id);
    });
    if (owners.length !== 1) {
      return { ok: false, diagnostic: { code: owners.length === 0 ? "LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN" : "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS" } };
    }
    return get().applyHierarchyOperationForPage(owners[0], { kind: "delete", shapeId: id });
  },

  reorderPageShapes: (pageId, newOrder) =>
    {
      const ids = newOrder;
      return get().applyHierarchyOperationForPage(pageId, { kind: "reorder-root-list", rootShapeIds: ids });
    },

  applyHierarchyOperation: (operation) => {
    return get().applyHierarchyOperationForPage(get().currentPageId, operation);
  },

  applyHierarchyOperationForPage: (pageId, operation) => {
    const snapshot = get();
    const pageSnapshot = { ...snapshot, currentPageId: pageId };
    const [page, ...currentPageShapes] = selectCurrentPageScene(pageSnapshot);
    if (!page) return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_UNAVAILABLE" } };
    const currentIds = new Set(currentPageShapes.map((shape) => shape.id));
    for (const otherPageId of Object.keys(snapshot.pages)) {
      if (otherPageId === pageId) continue;
      const [, ...otherPageShapes] = selectCurrentPageScene({ ...snapshot, currentPageId: otherPageId });
      if (otherPageShapes.some((shape) => currentIds.has(shape.id))) {
        return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS" } };
      }
    }
    const result = planHierarchyOperation(
      { pages: snapshot.pages, currentPageId: pageId, shapes: snapshot.shapes, currentPageShapes },
      operation,
    );
    if (result.ok) set({ pages: result.pages, shapes: result.shapes });
    return result;
  },

  // ── P4.4: Component Variants ──────────────────────────────────────────────

  promoteToComponent: (shapeId, meta) => {
    const shape = get().shapes[shapeId];
    if (!shape) return null;
    const snapshot = { ...shape };
    set((s) => ({
      shapes: {
        ...s.shapes,
        [shapeId]: { ...shape, type: "component", componentMeta: meta },
      },
    }));
    return snapshot;
  },

  addInstanceShape: (instanceId, componentName, bounds, instanceMeta) => {
    const { currentPageId: pid } = get();
    const instanceShape: Shape = {
      id: instanceId,
      type: "instance",
      name: `${componentName} (instance)`,
      bounds,
      transform: IDENTITY_TRANSFORM,
      rotation: 0,
      fills: [],
      opacity: 1,
      hidden: false,
      locked: false,
      parentId: null,
      children: [],
      instanceMeta,
    };
    set((s) => ({
      shapes: { ...s.shapes, [instanceId]: instanceShape },
      pages: {
        ...s.pages,
        [pid]: {
          ...s.pages[pid],
          rootShapeIds: [instanceId, ...s.pages[pid].rootShapeIds],
        },
      },
    }));
    return instanceId;
  },
}));

// ─────────────────────────────────────────────────────────────────────────────
// Selectors
// ─────────────────────────────────────────────────────────────────────────────

/** Ordered top-level shapes for the current page.
 *
 * Uses `useShallow` so Zustand compares the returned array element-by-element
 * (by reference) rather than always treating the new `flatMap` result as a
 * different value.  Without this, React 18's useSyncExternalStore fires the
 * "getSnapshot should be cached" warning and causes an infinite update loop.
 */
export function selectCurrentPageShapes(state: {
  pages: Record<string, Page>;
  currentPageId: string;
  shapes: Record<string, Shape>;
}): Shape[] {
  const page = state.pages[state.currentPageId];
  if (!page) return [];
  return page.rootShapeIds.flatMap((id) => (state.shapes[id] ? [state.shapes[id]] : []));
}

export function useCurrentPageShapes(): Shape[] {
  return useDocumentStore(useShallow(selectCurrentPageShapes));
}

/**
 * Current page scene records in stable document membership. Child records are
 * discovered through either side of the existing parent/children relation so
 * malformed one-sided links reach the renderer validator instead of vanishing.
 * Root order remains the Page.rootShapeIds order. Disconnected parentless
 * shapes cannot be attributed to a page because Shape has no pageId field.
 */
export function selectCurrentPageScene(state: {
  pages: Record<string, Page>;
  currentPageId: string;
  shapes: Record<string, Shape>;
}): readonly [Page | null, ...Shape[]] {
  const page = state.pages[state.currentPageId] ?? null;
  if (!page) return [null];

  const childrenByParent = new Map<string, Shape[]>();
  for (const shape of Object.values(state.shapes)) {
    if (shape.parentId) {
      const children = childrenByParent.get(shape.parentId) ?? [];
      children.push(shape);
      childrenByParent.set(shape.parentId, children);
    }
  }

  const scoped = new Map<string, Shape>();
  const pending = [...page.rootShapeIds].reverse();
  while (pending.length > 0) {
    const id = pending.pop()!;
    const shape = state.shapes[id];
    if (!shape || scoped.has(id)) continue;
    scoped.set(id, shape);
    const declaredChildren = Array.isArray(shape.children) ? shape.children : [];
    const linkedChildren = new Set([...declaredChildren, ...(childrenByParent.get(id) ?? []).map((child) => child.id)]);
    for (const childId of [...linkedChildren].reverse()) pending.push(childId);
  }
  return [page, ...scoped.values()];
}

export function useCurrentPageScene(): { page: Page | null; shapes: Shape[] } {
  const snapshot = useDocumentStore(useShallow(selectCurrentPageScene));
  return useMemo(() => ({ page: snapshot[0], shapes: snapshot.slice(1) as Shape[] }), [snapshot]);
}

useDocumentStore.subscribe((state, previous) => {
  if (state.currentPageId !== previous.currentPageId) {
    useSelectionStore.getState().clearSelection();
    return;
  }
  if (state.shapes === previous.shapes && state.pages === previous.pages) return;
  const [, ...sceneShapes] = selectCurrentPageScene(state);
  useSelectionStore.getState().reconcileTo(sceneShapes.map((shape) => shape.id));
});
