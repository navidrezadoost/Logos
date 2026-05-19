/**
 * stores/prototypeStore.ts  (P4.8)
 *
 * Stores prototype interactions (per-shape), connection-drawing ephemeral state,
 * and preview playback state. No persistence — reset on reload.
 */

import { create } from "zustand";
import type {
  PrototypeInteraction,
  PrototypeTransition,
} from "../types/prototype";

// ─────────────────────────────────────────────────────────────────────────────
// State shape
// ─────────────────────────────────────────────────────────────────────────────

interface ProtoState {
  // ── Persisted-in-session interaction data ──────────────────────────────────
  /** sourceShapeId → list of interactions */
  interactions: Record<string, PrototypeInteraction[]>;

  // ── Connection-drawing ephemeral state ────────────────────────────────────
  /** Shape ID being connected FROM (while dragging a new arrow). */
  pendingSource: string | null;
  /** Screen-space cursor position during an in-progress arrow drag. */
  arrowCursor: { x: number; y: number } | null;
  /** Currently selected connection for the config panel. */
  selectedConnection: { sourceId: string; index: number } | null;

  // ── Preview state ─────────────────────────────────────────────────────────
  previewOpen: boolean;
  previewCurrentFrame: string | null;
  previewPrevFrame: string | null;
  previewTransition: PrototypeTransition;
  previewDuration: number;
  previewTransitioning: boolean;

  // ── Actions ───────────────────────────────────────────────────────────────
  setPendingSource: (id: string | null) => void;
  setArrowCursor: (pos: { x: number; y: number } | null) => void;

  addInteraction: (sourceId: string, interaction: Omit<PrototypeInteraction, "id">) => void;
  updateInteraction: (sourceId: string, index: number, patch: Partial<PrototypeInteraction>) => void;
  removeInteraction: (sourceId: string, index: number) => void;

  selectConnection: (sourceId: string, index: number) => void;
  clearConnectionSelection: () => void;

  startPreview: (startFrameId: string) => void;
  navigate: (targetFrameId: string, transition: PrototypeTransition, duration: number) => void;
  stopPreview: () => void;

  getInteractions: (shapeId: string) => PrototypeInteraction[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export const useProtoStore = create<ProtoState>((set, get) => ({
  interactions: {},
  pendingSource: null,
  arrowCursor: null,
  selectedConnection: null,

  previewOpen: false,
  previewCurrentFrame: null,
  previewPrevFrame: null,
  previewTransition: "instant",
  previewDuration: 0,
  previewTransitioning: false,

  // ── Connection drawing ─────────────────────────────────────────────────────

  setPendingSource: (id) => set({ pendingSource: id, arrowCursor: null }),

  setArrowCursor: (pos) => set({ arrowCursor: pos }),

  // ── Interaction CRUD ──────────────────────────────────────────────────────

  addInteraction: (sourceId, interaction) => {
    const id = crypto.randomUUID();
    set((s) => ({
      interactions: {
        ...s.interactions,
        [sourceId]: [...(s.interactions[sourceId] ?? []), { id, ...interaction }],
      },
    }));
  },

  updateInteraction: (sourceId, index, patch) => {
    set((s) => {
      const list = [...(s.interactions[sourceId] ?? [])];
      if (!list[index]) return s;
      list[index] = { ...list[index], ...patch };
      return { interactions: { ...s.interactions, [sourceId]: list } };
    });
  },

  removeInteraction: (sourceId, index) => {
    set((s) => {
      const list = [...(s.interactions[sourceId] ?? [])];
      list.splice(index, 1);
      return { interactions: { ...s.interactions, [sourceId]: list } };
    });
  },

  // ── Selection ─────────────────────────────────────────────────────────────

  selectConnection: (sourceId, index) => set({ selectedConnection: { sourceId, index } }),
  clearConnectionSelection: () => set({ selectedConnection: null }),

  // ── Preview playback ──────────────────────────────────────────────────────

  startPreview: (startFrameId) =>
    set({
      previewOpen: true,
      previewCurrentFrame: startFrameId,
      previewPrevFrame: null,
      previewTransition: "instant",
      previewDuration: 0,
      previewTransitioning: false,
    }),

  navigate: (targetFrameId, transition, duration) => {
    set((s) => ({
      previewPrevFrame: s.previewCurrentFrame,
      previewCurrentFrame: targetFrameId,
      previewTransition: transition,
      previewDuration: duration,
      previewTransitioning: true,
    }));

    // Clear transitioning flag after the animation completes
    const timeout = transition === "instant" ? 0 : duration;
    setTimeout(() => {
      set({ previewTransitioning: false, previewPrevFrame: null });
    }, timeout + 50);
  },

  stopPreview: () =>
    set({
      previewOpen: false,
      previewCurrentFrame: null,
      previewPrevFrame: null,
      previewDuration: 0,
      previewTransitioning: false,
    }),

  // ── Derived ──────────────────────────────────────────────────────────────

  getInteractions: (shapeId) => get().interactions[shapeId] ?? [],
}));
