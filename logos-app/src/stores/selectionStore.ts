/**
 * stores/selectionStore.ts
 *
 * Tracks which shape IDs are currently selected and exposes
 * inspector-level properties for the selection.
 */

import { create } from "zustand";

interface SelectionState {
  /** The primary selected shape IDs (order preserved for multi-select). */
  selectedIds: string[];

  // ── Actions ────────────────────────────────────────────────────────────────

  /** Select exactly one shape, clearing any prior selection. */
  select: (id: string) => void;

  /** Toggle a shape in/out of a multi-selection. */
  toggleSelect: (id: string) => void;

  /** Select a range of shapes (e.g. shift-click in layers panel). */
  selectRange: (ids: string[]) => void;

  /** Clear the selection. */
  clearSelection: () => void;
}

export const useSelectionStore = create<SelectionState>((set, get) => ({
  selectedIds: [],

  select: (id) => set({ selectedIds: [id] }),

  toggleSelect: (id) =>
    set((s) => {
      const has = s.selectedIds.includes(id);
      return {
        selectedIds: has
          ? s.selectedIds.filter((sid) => sid !== id)
          : [...s.selectedIds, id],
      };
    }),

  selectRange: (ids) => set({ selectedIds: Array.from(new Set(ids)) }),

  clearSelection: () => set({ selectedIds: [] }),
}));

// Convenience selector
export function useSelectedIds(): string[] {
  return useSelectionStore((s) => s.selectedIds);
}

export function useIsSelected(id: string): boolean {
  return useSelectionStore((s) => s.selectedIds.includes(id));
}
