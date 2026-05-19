/**
 * stores/templateStore.ts
 *
 * P4.7 — Template Library
 *
 * Manages template browsing state and the insert-into-document flow.
 * Templates are bundled modules (no network fetch) — fully offline.
 *
 * Insert flow:
 *   1. Clone all shapes from the template, remapping every ID to a fresh UUID.
 *   2. Remap parentId references to the new UUIDs.
 *   3. Add shapes to the current document page via documentStore.addShape().
 *   4. Optionally offset shapes so they land at the canvas viewport centre.
 */

import { create } from "zustand";
import {
    ALL_TEMPLATES,
    CATEGORIES,
    getByCategory,
    type TemplateCategory,
    type TemplateData,
} from "../templates";
import { type Shape } from "../types/shapes";
import { useDocumentStore } from "./documentStore";

// ─────────────────────────────────────────────────────────────────────────────
// Store interface
// ─────────────────────────────────────────────────────────────────────────────

interface TemplateState {
    templates: TemplateData[];
    categories: TemplateCategory[];
    activeCategory: TemplateCategory;
    searchQuery: string;

    /** The template currently being previewed (hover / keyboard nav). */
    hoveredId: string | null;

    /** Whether the gallery modal is open. */
    galleryOpen: boolean;

    /** Insertion status for user feedback. */
    lastInserted: string | null;

    // ── Actions ──────────────────────────────────────────────────────────────
    openGallery: () => void;
    closeGallery: () => void;
    setCategory: (cat: TemplateCategory) => void;
    setSearchQuery: (q: string) => void;
    setHovered: (id: string | null) => void;

    /** Clone template shapes with fresh UUIDs and add to current document page. */
    insertTemplate: (templateId: string, offsetX?: number, offsetY?: number) => void;

    /** Derive: templates visible in current tab + search. */
    visibleTemplates: () => TemplateData[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Insert helper
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Deep-clone a template's shapes, assigning fresh UUIDs to every shape
 * and rewriting all parentId / children references to the new IDs.
 * An optional (offsetX, offsetY) shifts the entire set so it lands
 * at the user's viewport centre rather than at coordinate (0,0).
 */
function cloneShapes(
    shapes: Shape[],
    offsetX: number,
    offsetY: number
): Shape[] {
    // Build old-id → new-id mapping
    const idMap = new Map<string, string>();
    for (const s of shapes) {
        idMap.set(s.id, crypto.randomUUID());
    }

    return shapes.map((s) => ({
        ...s,
        id: idMap.get(s.id)!,
        bounds: {
            ...s.bounds,
            x: s.bounds.x + offsetX,
            y: s.bounds.y + offsetY,
        },
        parentId: s.parentId ? (idMap.get(s.parentId) ?? null) : null,
        children: s.children.map((cid) => idMap.get(cid) ?? cid),
        fills: s.fills.map((f) => ({ ...f })), // shallow-clone fills
    }));
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export const useTemplateStore = create<TemplateState>((set, get) => ({
    templates: ALL_TEMPLATES,
    categories: CATEGORIES,
    activeCategory: "Web",
    searchQuery: "",
    hoveredId: null,
    galleryOpen: false,
    lastInserted: null,

    openGallery: () => set({ galleryOpen: true }),
    closeGallery: () => set({ galleryOpen: false, hoveredId: null }),
    setCategory: (activeCategory) => set({ activeCategory, searchQuery: "" }),
    setSearchQuery: (searchQuery) => set({ searchQuery }),
    setHovered: (hoveredId) => set({ hoveredId }),

    insertTemplate(templateId, offsetX = 100, offsetY = 100) {
        const tpl = ALL_TEMPLATES.find((t) => t.id === templateId);
        if (!tpl) return;

        const cloned = cloneShapes(tpl.shapes, offsetX, offsetY);
        const { addShape } = useDocumentStore.getState();
        for (const shape of cloned) {
            addShape(shape);
        }

        set({ lastInserted: tpl.name, galleryOpen: false });

        // Clear the "last inserted" toast after 3 s
        setTimeout(() => set({ lastInserted: null }), 3000);
    },

    visibleTemplates() {
        const { activeCategory, searchQuery } = get();
        let list = getByCategory(activeCategory);
        const q = searchQuery.trim().toLowerCase();
        if (q) {
            list = list.filter(
                (t) =>
                    t.name.toLowerCase().includes(q) ||
                    t.description.toLowerCase().includes(q)
            );
        }
        return list;
    },
}));
