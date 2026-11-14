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
import { buildSceneProjection } from "../render-wasm/scene-projection";

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

/** Full in-memory load boundary used by offline hydration and import adapters. */
export interface DocumentHydration {
  pages: Record<string, Page>;
  pageOrder: string[];
  currentPageId: string;
  shapes: Record<string, Shape>;
}

export type DocumentHydrationResult =
  | { ok: true }
  | { ok: false; diagnostic: { code: string; pageId?: string; shapeId?: string; relatedShapeId?: string } };

type CompatibilityHierarchyOperation = Extract<HierarchyOperation, { kind: `compat-${string}` }>;

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

  /** Atomically validate and load a persisted/imported snapshot without rewriting IDs or unknown fields. */
  hydrateDocument: (document: DocumentHydration) => DocumentHydrationResult;

  /** Add a shape to the current page; returns the new shape id. */
  addRect: (bounds: Rect, color?: string) => string;

  /** Add any pre-built shape to the current page; returns the shape id. */
  addShape: (shape: Shape) => string;
  /** Atomically add a related legacy/template shape batch through the compatibility boundary. */
  addShapes: (shapes: Shape[]) => string[];

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
  /** Explicit compatibility boundary for legacy additions and subtree deletion/replacement. */
  applyCompatibilityOperation: (operation: CompatibilityHierarchyOperation) => HierarchyOperationResult;
  /** Store-internal variant used by existing page-addressed root-order APIs. */
  applyHierarchyOperationForPage: (pageId: string, operation: HierarchyOperation) => HierarchyOperationResult;
  /** Page-scoped compatibility operations are deliberately separate from A10's supported-operation API. */
  applyCompatibilityOperationForPage: (pageId: string, operation: CompatibilityHierarchyOperation) => HierarchyOperationResult;

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

  hydrateDocument: (document) => {
    const diagnostic = validateDocumentHydration(document);
    if (diagnostic) return { ok: false, diagnostic };
    set(clonePlainData(document));
    return { ok: true };
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
    if (!shape || typeof shape !== "object" || typeof shape.id !== "string") throw new Error("LOGOS_OPERATION_INVALID_SHAPE");
    if (shape.parentId !== null) {
      const parentOwners = getShapePageOwners(get(), shape.parentId);
      if (parentOwners.length > 1 || (parentOwners.length === 1 && parentOwners[0] !== get().currentPageId)) {
        throw new Error("LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS");
      }
    }
    if (shape.type === "rect" || shape.type === "frame") {
      if (shape.type === "frame" && shape.parentId !== null) {
        const result = get().applyCompatibilityOperation({ kind: "compat-add-shape", shape });
        if (!result.ok) throw new Error(result.diagnostic.code);
        return shape.id;
      }
      const operation: HierarchyOperation = shape.parentId === null
        ? { kind: "create-root", shape }
        : { kind: "insert-child", parentId: shape.parentId, shape: { ...shape, parentId: null }, index: 0 };
      const result = get().applyHierarchyOperation(operation);
      if (result.ok) return shape.id;
      if (!isRendererLimitation(result.diagnostic.code)) throw new Error(result.diagnostic.code);
    }
    const result = get().applyCompatibilityOperation({ kind: "compat-add-shape", shape });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return shape.id;
  },

  addShapes: (shapes) => {
    const state = get();
    const incomingIds = new Set(shapes.map((shape) => shape?.id?.toLowerCase()));
    for (const shape of shapes) {
      if (!shape || typeof shape.id !== "string") throw new Error("LOGOS_OPERATION_INVALID_SHAPE");
      if (shape.parentId !== null && !incomingIds.has(shape.parentId.toLowerCase())) {
        const owners = getShapePageOwners(state, shape.parentId);
        if (owners.length > 1 || (owners.length === 1 && owners[0] !== state.currentPageId)) {
          throw new Error("LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS");
        }
      }
    }
    const result = get().applyCompatibilityOperation({ kind: "compat-add-shapes", shapes });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return shapes.map((shape) => shape.id);
  },

  addVectorNetwork: (anchors, segments, regions = [], fill = "#6c9ef8") => {
    assertVectorNetworkInput(anchors, segments, regions);
    const id = shapeId();
    const count = Object.values(get().shapes).filter((s) => s.type === "vector-network").length + 1;
    const shape = createVectorNetwork(id, `Path ${count}`, anchors, segments, regions, fill);
    validateVectorNetwork(shape);
    const result = get().applyCompatibilityOperation({ kind: "compat-add-shape", shape });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return id;
  },

  replaceWithVectorNetwork: (removeIds, anchors, segments, regions = []) => {
    if (!Array.isArray(removeIds)) throw new Error("LOGOS_COMPAT_REPLACEMENT_INVALID_SELECTION");
    const state = get();
    const ownerPagesById = removeIds.map((id) => getShapePageOwners(state, id));
    if (ownerPagesById.some((owners) => owners.length > 1) || new Set(ownerPagesById.flat()).size > 1 ||
        ownerPagesById.some((owners) => owners.length === 1 && owners[0] !== state.currentPageId)) {
      throw new Error("LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS");
    }
    if (ownerPagesById.some((owners) => owners.length === 0)) throw new Error("LOGOS_OPERATION_PAGE_OWNERSHIP_UNKNOWN");
    assertVectorNetworkInput(anchors, segments, regions);
    const id = shapeId();
    const count = Object.values(get().shapes).filter((s) => s.type === "vector-network").length + 1;
    const shape = createVectorNetwork(id, `Path ${count}`, anchors, segments, regions);
    validateVectorNetwork(shape);
    const result = get().applyCompatibilityOperation({ kind: "compat-replace-vector-network", removeIds, shape });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return id;
  },

  updateShape: (id, patch) => {
    if (Object.prototype.hasOwnProperty.call(patch, "id") || Object.prototype.hasOwnProperty.call(patch, "parentId") || Object.prototype.hasOwnProperty.call(patch, "children")) {
      throw new Error("LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY");
    }
    set((s) => {
      if (!s.shapes[id]) throw new Error("LOGOS_OPERATION_SHAPE_NOT_FOUND");
      return { shapes: { ...s.shapes, [id]: { ...s.shapes[id], ...clonePlainData(patch) } } };
    });
  },

  batchUpdate: (patches) => {
    if (Object.values(patches).some((patch) => Object.prototype.hasOwnProperty.call(patch, "id") || Object.prototype.hasOwnProperty.call(patch, "parentId") || Object.prototype.hasOwnProperty.call(patch, "children"))) {
      throw new Error("LOGOS_OPERATION_USE_HIERARCHY_BOUNDARY");
    }
    const ids = Object.keys(patches);
    set((s) => {
      if (ids.some((id) => !s.shapes[id])) throw new Error("LOGOS_OPERATION_SHAPE_NOT_FOUND");
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
    const validated = get().applyHierarchyOperationForPage(owners[0], { kind: "delete", shapeId: id });
    if (validated.ok || !isRendererLimitation(validated.diagnostic.code)) return validated;
    const result = get().applyCompatibilityOperationForPage(owners[0], { kind: "compat-delete", shapeId: id });
    reportRenderingLimitation(result);
    return result;
  },

  reorderPageShapes: (pageId, newOrder) =>
    {
      const ids = newOrder;
      return get().applyHierarchyOperationForPage(pageId, { kind: "reorder-root-list", rootShapeIds: ids });
    },

  applyHierarchyOperation: (operation) => {
    if (operation.kind.startsWith("compat-")) return { ok: false, diagnostic: { code: "LOGOS_OPERATION_COMPATIBILITY_BOUNDARY_REQUIRED" } };
    return get().applyHierarchyOperationForPage(get().currentPageId, operation);
  },

  applyCompatibilityOperation: (operation) => {
    return get().applyCompatibilityOperationForPage(get().currentPageId, operation);
  },

  applyHierarchyOperationForPage: (pageId, operation) => {
    if (operation.kind.startsWith("compat-")) return { ok: false, diagnostic: { code: "LOGOS_OPERATION_COMPATIBILITY_BOUNDARY_REQUIRED" } };
    const snapshot = get();
    const pageSnapshot = { ...snapshot, currentPageId: pageId };
    const [page, ...currentPageShapes] = selectCurrentPageScene(pageSnapshot);
    if (!page) return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_UNAVAILABLE" } };
    const currentIds = new Set(currentPageShapes.map((shape) => shape.id.toLowerCase()));
    for (const otherPageId of Object.keys(snapshot.pages)) {
      if (otherPageId === pageId) continue;
      const [, ...otherPageShapes] = selectCurrentPageScene({ ...snapshot, currentPageId: otherPageId });
      if (otherPageShapes.some((shape) => currentIds.has(shape.id.toLowerCase()))) {
        return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS" } };
      }
    }
    const result = planHierarchyOperation(
      { pages: snapshot.pages, currentPageId: pageId, shapes: snapshot.shapes, currentPageShapes },
      operation,
    );
    if (result.ok) {
      const selectedBefore = useSelectionStore.getState().selectedIds;
      set({ pages: result.pages, shapes: result.shapes });
      if (result.replacementShapeId && selectedBefore.some((id) => result.affectedIds.includes(id))) {
        useSelectionStore.getState().select(result.replacementShapeId);
      }
    }
    return result;
  },

  applyCompatibilityOperationForPage: (pageId, operation) => {
    const snapshot = get();
    const pageSnapshot = { ...snapshot, currentPageId: pageId };
    const [page, ...currentPageShapes] = selectCurrentPageScene(pageSnapshot);
    if (!page) return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_UNAVAILABLE" } };
    const currentIds = new Set(currentPageShapes.map((shape) => shape.id.toLowerCase()));
    for (const otherPageId of Object.keys(snapshot.pages)) {
      if (otherPageId === pageId) continue;
      const [, ...otherPageShapes] = selectCurrentPageScene({ ...snapshot, currentPageId: otherPageId });
      if (otherPageShapes.some((shape) => currentIds.has(shape.id.toLowerCase()))) {
        return { ok: false, diagnostic: { code: "LOGOS_OPERATION_PAGE_OWNERSHIP_AMBIGUOUS" } };
      }
    }
    const result = planHierarchyOperation(
      { pages: snapshot.pages, currentPageId: pageId, shapes: snapshot.shapes, currentPageShapes },
      operation,
    );
    if (result.ok) {
      const selectedBefore = useSelectionStore.getState().selectedIds;
      set({ pages: result.pages, shapes: result.shapes });
      if (result.replacementShapeId && selectedBefore.some((id) => result.affectedIds.includes(id))) {
        useSelectionStore.getState().select(result.replacementShapeId);
      }
    }
    reportRenderingLimitation(result);
    return result;
  },

  // ── P4.4: Component Variants ──────────────────────────────────────────────

  promoteToComponent: (shapeId, meta) => {
    const shape = get().shapes[shapeId];
    if (!shape) return null;
    if (shape.type !== "frame" && shape.type !== "group") return null;
    if (getShapePageOwners(get(), shapeId).length !== 1) return null;
    const result = get().applyCompatibilityOperation({ kind: "compat-promote-component", shapeId, meta });
    return result.ok ? { ...shape } : null;
  },

  addInstanceShape: (instanceId, componentName, bounds, instanceMeta) => {
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
    const result = get().applyCompatibilityOperation({ kind: "compat-add-shape", shape: instanceShape });
    if (!result.ok) throw new Error(result.diagnostic.code);
    return instanceId;
  },
}));

function isRendererLimitation(code: string): boolean {
  return code === "LOGOS_SCENE_UNSUPPORTED_SHAPE" || code === "LOGOS_SCENE_UNSUPPORTED_CAPABILITY";
}

function reportRenderingLimitation(result: HierarchyOperationResult): void {
  if (result.ok && result.renderingDiagnostic) {
    console.warn("[documentStore] Preserved compatibility content outside the verified A9 renderer contract", result.renderingDiagnostic);
  }
}

function validateVectorNetwork(shape: Shape): void {
  const anchors = shape.vnAnchors;
  const segments = shape.vnSegments;
  const regions = shape.vnRegions ?? [];
  const validPoint = (point: unknown): boolean => Array.isArray(point) && point.length === 2 && point.every(Number.isFinite);
  if (!Array.isArray(anchors) || !Array.isArray(segments) || !Array.isArray(regions) || anchors.some((anchor) =>
    !anchor || !Number.isFinite(anchor.x) || !Number.isFinite(anchor.y) ||
    (anchor.hi != null && !validPoint(anchor.hi)) || (anchor.ho != null && !validPoint(anchor.ho))
  )) throw new Error("LOGOS_VECTOR_NETWORK_INVALID_GEOMETRY");
  if (segments.some((segment) => !segment || !Number.isInteger(segment.s) || !Number.isInteger(segment.e) ||
    segment.s < 0 || segment.e < 0 || segment.s >= anchors.length || segment.e >= anchors.length ||
    (segment.c1 != null && !validPoint(segment.c1)) || (segment.c2 != null && !validPoint(segment.c2))
  )) throw new Error("LOGOS_VECTOR_NETWORK_INVALID_GEOMETRY");
  if (regions.some((region) => !Array.isArray(region) || region.some((segmentIndex) =>
    !Number.isInteger(segmentIndex) || segmentIndex < 0 || segmentIndex >= segments.length
  ))) throw new Error("LOGOS_VECTOR_NETWORK_INVALID_GEOMETRY");
}

function assertVectorNetworkInput(anchors: VNAnchor[], segments: VNSegment[], regions: VNRegion[]): void {
  const candidate: Shape = {
    id: "00000000-0000-4000-8000-000000000000",
    type: "vector-network",
    name: "validation",
    bounds: { x: 0, y: 0, w: 0, h: 0 },
    transform: IDENTITY_TRANSFORM,
    rotation: 0,
    fills: [],
    opacity: 1,
    hidden: false,
    locked: false,
    parentId: null,
    children: [],
    vnAnchors: anchors,
    vnSegments: segments,
    vnRegions: regions,
  };
  validateVectorNetwork(candidate);
}

/**
 * Validate the load boundary without normalizing identities, unknown fields,
 * ordering, or unsupported renderer content. Renderer limitations are not
 * document corruption and remain visible after hydration.
 */
export function validateDocumentHydration(document: DocumentHydration): Extract<DocumentHydrationResult, { ok: false }>["diagnostic"] | null {
  if (!document || !isRecord(document.pages) || !Array.isArray(document.pageOrder) ||
      !isRecord(document.shapes) || typeof document.currentPageId !== "string") {
    return { code: "LOGOS_HYDRATION_INVALID_DOCUMENT_SHAPE" };
  }
  const pageIds = Object.keys(document.pages);
  if (pageIds.length === 0 || document.pageOrder.length !== pageIds.length ||
      new Set(document.pageOrder).size !== pageIds.length ||
      document.pageOrder.some((id) => !Object.prototype.hasOwnProperty.call(document.pages, id)) ||
      !document.pageOrder.includes(document.currentPageId)) {
    return { code: "LOGOS_HYDRATION_INVALID_PAGE_ORDER" };
  }
  for (const [pageId, page] of Object.entries(document.pages)) {
    if (!page || page.id !== pageId || typeof page.name !== "string" || !Array.isArray(page.rootShapeIds) ||
        page.rootShapeIds.some((id) => typeof id !== "string")) {
      return { code: "LOGOS_HYDRATION_INVALID_PAGE", pageId };
    }
  }
  for (const [key, shape] of Object.entries(document.shapes)) {
    if (!shape || typeof shape !== "object" || key !== shape.id || typeof shape.id !== "string") {
      return { code: "LOGOS_HYDRATION_SHAPE_KEY_MISMATCH", shapeId: shape?.id, relatedShapeId: key };
    }
  }

  const owners = new Map<string, string>();
  const knownIds = new Set<string>();
  for (const pageId of document.pageOrder) {
    const [, ...pageShapes] = selectCurrentPageScene({ ...document, currentPageId: pageId });
    for (const shape of pageShapes) {
      const canonicalId = shape.id.toLowerCase();
      const owner = owners.get(canonicalId);
      if (owner && owner !== pageId) return { code: "LOGOS_HYDRATION_AMBIGUOUS_PAGE_OWNERSHIP", pageId, shapeId: shape.id, relatedShapeId: owner };
      owners.set(canonicalId, pageId);
      knownIds.add(shape.id);
    }
    const page = document.pages[pageId];
    const projection = buildSceneProjection(page, pageShapes, page.rootShapeIds);
    if (!projection.ok && projection.diagnostic.code !== "LOGOS_SCENE_UNSUPPORTED_SHAPE") {
      return { code: projection.diagnostic.code, pageId, shapeId: projection.diagnostic.shapeId, relatedShapeId: projection.diagnostic.relatedShapeId };
    }
  }
  const unowned = Object.keys(document.shapes).find((id) => !knownIds.has(id));
  if (unowned) return { code: "LOGOS_HYDRATION_UNOWNED_SHAPE", shapeId: unowned };
  return null;
}

function isRecord(value: unknown): value is Record<string, any> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function getShapePageOwners(state: Pick<DocumentState, "pages" | "currentPageId" | "shapes">, shapeId: string): string[] {
  return Object.keys(state.pages).filter((pageId) => {
    const [, ...scene] = selectCurrentPageScene({ ...state, currentPageId: pageId });
    return scene.some((shape) => shape.id === shapeId);
  });
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
