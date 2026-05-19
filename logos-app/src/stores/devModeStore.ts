/**
 * stores/devModeStore.ts
 *
 * Ephemeral state for P4.9 Dev Mode.
 * Tracks the currently inspected shape and clipboard-flash feedback.
 */

import { create } from "zustand";

interface DevModeState {
  /** The shape being deeply inspected (hovered in dev mode). */
  inspectedShapeId: string | null;
  /** Which CSS property key was most recently copied (for flash UI). */
  copiedProp: string | null;

  setInspectedShape: (id: string | null) => void;
  flashCopied: (prop: string) => void;
  clearFlash: () => void;
}

export const useDevModeStore = create<DevModeState>((set) => ({
  inspectedShapeId: null,
  copiedProp: null,

  setInspectedShape: (id) => set({ inspectedShapeId: id }),

  flashCopied: (prop) => {
    set({ copiedProp: prop });
    // Auto-clear after 1.5 s.
    setTimeout(() => set((s) => (s.copiedProp === prop ? { copiedProp: null } : {})), 1500);
  },

  clearFlash: () => set({ copiedProp: null }),
}));
