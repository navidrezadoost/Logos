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
import { nanoid } from "nanoid";
import { type Shape, type Rect, type VNAnchor, type VNSegment, type VNRegion, type ComponentMeta, type InstanceMeta, createRect, createVectorNetwork, IDENTITY_TRANSFORM } from "../types/shapes";

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
   * Remove a shape and unlink from its page / parent.
   * Does not recurse into children – the caller is responsible for that
   * at the Rust scene graph level.
   */
  removeShape: (id: string) => void;

  /** Re-order shapes within a page (drag-and-drop in layers panel). */
  reorderPageShapes: (pageId: string, newOrder: string[]) => void;

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

  setCurrentPage: (pageId) => set({ currentPageId: pageId }),

  // ── Shapes ────────────────────────────────────────────────────────────────

  addRect: (bounds, color = "#0000ff") => {
    const id = shapeId(); // crypto.randomUUID() — hex format required by WASM bridge
    const { currentPageId: pid } = get();
    const name = `Rectangle ${Object.values(get().shapes).filter((s) => s.type === "rect").length + 1}`;
    const shape = createRect(id, name, bounds, color);

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

  addShape: (shape) => {
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

  updateShape: (id, patch) =>
    set((s) => ({
      shapes: { ...s.shapes, [id]: { ...s.shapes[id], ...patch } },
    })),

  batchUpdate: (patches) =>
    set((s) => {
      const next = { ...s.shapes };
      for (const [id, patch] of Object.entries(patches)) {
        if (next[id]) next[id] = { ...next[id], ...patch };
      }
      return { shapes: next };
    }),

  removeShape: (id) =>
    set((s) => {
      const shape = s.shapes[id];
      if (!shape) return s;

      const nextShapes = { ...s.shapes };
      delete nextShapes[id];

      // Unlink from page root list
      const nextPages = { ...s.pages };
      for (const [pid, page] of Object.entries(nextPages)) {
        if (page.rootShapeIds.includes(id)) {
          nextPages[pid] = {
            ...page,
            rootShapeIds: page.rootShapeIds.filter((sid) => sid !== id),
          };
        }
      }

      // Unlink from parent children list
      if (shape.parentId) {
        const parent = nextShapes[shape.parentId];
        if (parent) {
          nextShapes[shape.parentId] = {
            ...parent,
            children: parent.children.filter((cid) => cid !== id),
          };
        }
      }

      return { shapes: nextShapes, pages: nextPages };
    }),

  reorderPageShapes: (pageId, newOrder) =>
    set((s) => ({
      pages: {
        ...s.pages,
        [pageId]: { ...s.pages[pageId], rootShapeIds: newOrder },
      },
    })),

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

/** Ordered top-level shapes for the current page. */
export function useCurrentPageShapes(): Shape[] {
  return useDocumentStore((s) => {
    const page = s.pages[s.currentPageId];
    if (!page) return [];
    return page.rootShapeIds.flatMap((id) => (s.shapes[id] ? [s.shapes[id]] : []));
  });
}
