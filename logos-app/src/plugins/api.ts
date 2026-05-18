/**
 * Plugin API — host-side implementation of the Logos plugin API surface.
 *
 * `buildPluginApi(pluginId)` returns an object whose methods are dispatched
 * by bridge.ts when a plugin calls `logos.call(method, params)`.
 *
 * This layer owns _permission enforcement_ (delegated from bridge.ts) and
 * _data mapping_ between internal Zustand shapes and the stable PluginShape
 * interface (decoupled so internal Shape refactors don't break plugins).
 */

import { useDocumentStore, type Page } from "../stores/documentStore";
import { useSelectionStore } from "../stores/selectionStore";
import type { Shape } from "../types/shapes";
import type { PluginFill, PluginPage, PluginShape, PluginShapeType } from "./types";

// ---------------------------------------------------------------------------
// Internal → Plugin shape mapping
// ---------------------------------------------------------------------------

function toPluginShapeType(type: Shape["type"]): PluginShapeType {
  switch (type) {
    case "circle":
    case "ellipse":
      return "ellipse";
    case "frame":
      return "frame";
    case "group":
      return "group";
    case "path":
      return "path";
    case "text":
      return "text";
    case "bool":
      return "bool";
    case "svg-raw":
      return "svg-raw";
    default:
      return "rect";
  }
}

function toPluginShape(s: Shape): PluginShape {
  return {
    id: s.id,
    type: toPluginShapeType(s.type),
    name: s.name,
    x: s.bounds.x,
    y: s.bounds.y,
    width: s.bounds.w,
    height: s.bounds.h,
    rotation: s.rotation,
    opacity: s.opacity,
    hidden: s.hidden,
    fills: s.fills.map(
      (f): PluginFill => ({ type: "solid", color: f.color, opacity: f.opacity })
    ),
  };
}

// ---------------------------------------------------------------------------
// API factory
// ---------------------------------------------------------------------------

export interface LogosPluginApi {
  /** Returns all shapes on the current page (read). */
  getPage(): PluginPage;

  /** Returns currently selected shapes (read). */
  getSelection(): PluginShape[];

  /** Returns a single shape by ID (read). */
  getShape(params: { id: string }): PluginShape | null;

  /** Patch properties on a shape (content). */
  updateShape(params: { id: string; patch: Partial<Omit<PluginShape, "id" | "type">> }): void;

  /** Create a new rectangle (content). Returns the new shape's ID. */
  createRect(params: { x: number; y: number; width: number; height: number; name?: string }): string;

  /** Create a new ellipse (content). Returns the new shape's ID. */
  createEllipse(params: { x: number; y: number; width: number; height: number; name?: string }): string;

  /** Delete a shape by ID (content). */
  deleteShape(params: { id: string }): void;
}

/**
 * Build the API object for a given plugin session.
 *
 * @param _pluginId - reserved for per-plugin audit logging in the future.
 */
export function buildPluginApi(_pluginId: string): LogosPluginApi {
  function getDocState() {
    return useDocumentStore.getState();
  }

  function getSelState() {
    return useSelectionStore.getState();
  }

  return {
    getPage(): PluginPage {
      const { pages, currentPageId, shapes } = getDocState();
      const page: Page = pages[currentPageId];
      if (!page) throw new Error("No active page");
      const pageShapes = page.rootShapeIds.map((id) => shapes[id]).filter(Boolean).map(toPluginShape);
      return { id: page.id, name: page.name, shapes: pageShapes };
    },

    getSelection(): PluginShape[] {
      const { selectedIds } = getSelState();
      const { shapes } = getDocState();
      return selectedIds
        .map((id) => shapes[id])
        .filter(Boolean)
        .map(toPluginShape);
    },

    getShape({ id }): PluginShape | null {
      const { shapes } = getDocState();
      const s = shapes[id];
      return s ? toPluginShape(s) : null;
    },

    updateShape({ id, patch }) {
      const { shapes } = getDocState();
      const s = shapes[id];
      if (!s) throw new Error(`Shape ${id} not found`);

      const shapePatch: Partial<Shape> = {};
      if (patch.x !== undefined || patch.y !== undefined || patch.width !== undefined || patch.height !== undefined) {
        shapePatch.bounds = {
          x: patch.x ?? s.bounds.x,
          y: patch.y ?? s.bounds.y,
          w: patch.width ?? s.bounds.w,
          h: patch.height ?? s.bounds.h,
        };
      }
      if (patch.name !== undefined) shapePatch.name = patch.name;
      if (patch.opacity !== undefined) shapePatch.opacity = patch.opacity;
      if (patch.hidden !== undefined) shapePatch.hidden = patch.hidden;
      if (patch.rotation !== undefined) shapePatch.rotation = patch.rotation;
      if (patch.fills !== undefined) {
        shapePatch.fills = patch.fills.map((f) => ({ type: "solid" as const, color: f.color, opacity: f.opacity }));
      }

      getDocState().batchUpdate({ [id]: shapePatch });
    },

    createRect({ x, y, width, height, name = "Rect" }): string {
      const { addShape } = getDocState();
      const id = crypto.randomUUID();
      addShape({
        id,
        type: "rect",
        name,
        bounds: { x, y, w: width, h: height },
        transform: [1, 0, 0, 1, 0, 0],
        rotation: 0,
        fills: [{ type: "solid", color: "#0066cc", opacity: 1 }],
        opacity: 1,
        hidden: false,
        locked: false,
        parentId: null,
        children: [],
      });
      return id;
    },

    createEllipse({ x, y, width, height, name = "Ellipse" }): string {
      const { addShape } = getDocState();
      const id = crypto.randomUUID();
      addShape({
        id,
        type: "ellipse",
        name,
        bounds: { x, y, w: width, h: height },
        transform: [1, 0, 0, 1, 0, 0],
        rotation: 0,
        fills: [{ type: "solid", color: "#cc6600", opacity: 1 }],
        opacity: 1,
        hidden: false,
        locked: false,
        parentId: null,
        children: [],
      });
      return id;
    },

    deleteShape({ id }) {
      // batchUpdate with null signals deletion — extend documentStore if needed.
      // For now, use removeShape if available via a store extension.
      const state = getDocState() as ReturnType<typeof useDocumentStore.getState> & {
        removeShape?: (id: string) => void;
      };
      if (typeof state.removeShape === "function") {
        state.removeShape(id);
      } else {
        console.warn("[logos-plugin] deleteShape: removeShape not implemented in documentStore yet.");
      }
    },
  };
}
