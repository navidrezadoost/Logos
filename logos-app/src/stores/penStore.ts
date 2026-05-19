/**
 * stores/penStore.ts
 *
 * Ephemeral state for the active Pen (path) tool session.
 * Cleared when the path is committed or the session is cancelled.
 */

import { create } from "zustand";
import type { VNAnchor, VNSegment } from "../types/shapes";

export interface PenMousePos {
  x: number;
  y: number;
}

interface PenState {
  /** Anchors placed so far in the current session. */
  anchors: VNAnchor[];
  /** Segments placed so far (always anchors.length - 1 for an open path). */
  segments: VNSegment[];
  /** Current mouse position, used for the live preview segment. */
  cursor: PenMousePos | null;
  /** Index of the anchor currently being dragged (for handle editing). */
  draggingAnchor: number | null;
  /** Drag start position (for computing handle delta). */
  dragStart: PenMousePos | null;

  // ── Actions ────────────────────────────────────────────────────────────────

  /** Add a new anchor at the given position. */
  addAnchor: (x: number, y: number) => void;

  /** Update cursor position (for preview segment). */
  setCursor: (pos: PenMousePos | null) => void;

  /** Begin dragging an anchor to set its handles. */
  startAnchorDrag: (index: number, x: number, y: number) => void;

  /** Update handle offsets for the anchor being dragged. */
  updateAnchorHandle: (x: number, y: number) => void;

  /** Finish dragging an anchor handle. */
  endAnchorDrag: () => void;

  /** Reset the entire pen session (commit or cancel). */
  reset: () => void;
}

export const usePenStore = create<PenState>((set) => ({
  anchors: [],
  segments: [],
  cursor: null,
  draggingAnchor: null,
  dragStart: null,

  addAnchor: (x, y) =>
    set((s) => {
      const newAnchor: VNAnchor = { x, y };
      const newAnchors = [...s.anchors, newAnchor];
      const newSegments = [...s.segments];

      // Add a straight segment connecting previous anchor to this one
      if (newAnchors.length > 1) {
        const segStart = newAnchors.length - 2;
        const segEnd = newAnchors.length - 1;
        newSegments.push({ s: segStart, e: segEnd });
      }

      return { anchors: newAnchors, segments: newSegments };
    }),

  setCursor: (cursor) => set({ cursor }),

  startAnchorDrag: (index, x, y) =>
    set({ draggingAnchor: index, dragStart: { x, y } }),

  updateAnchorHandle: (x, y) =>
    set((s) => {
      const { draggingAnchor, dragStart, anchors } = s;
      if (draggingAnchor === null || !dragStart) return s;

      const dx = x - anchors[draggingAnchor].x;
      const dy = y - anchors[draggingAnchor].y;

      const newAnchors = anchors.map((a, i) =>
        i === draggingAnchor
          ? { ...a, ho: [dx, dy] as [number, number], hi: [-dx, -dy] as [number, number] }
          : a
      );

      // Update the cubic control points on the adjacent segments
      const newSegments = s.segments.map((seg) => {
        if (seg.e === draggingAnchor) {
          // Outgoing from previous anchor into this one — update c2
          const anchor = newAnchors[draggingAnchor];
          return { ...seg, c2: [anchor.x - dx, anchor.y - dy] as [number, number] };
        }
        if (seg.s === draggingAnchor) {
          // Leaving this anchor — update c1
          const anchor = newAnchors[draggingAnchor];
          return { ...seg, c1: [anchor.x + dx, anchor.y + dy] as [number, number] };
        }
        return seg;
      });

      return { anchors: newAnchors, segments: newSegments };
    }),

  endAnchorDrag: () => set({ draggingAnchor: null, dragStart: null }),

  reset: () =>
    set({ anchors: [], segments: [], cursor: null, draggingAnchor: null, dragStart: null }),
}));
