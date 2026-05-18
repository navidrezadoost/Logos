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
import { type Shape, type Rect, createRect } from "../types/shapes";

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
