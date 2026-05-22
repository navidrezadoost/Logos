/**
 * stores/uiStore.ts
 *
 * Ephemeral UI state: active tool, panel visibility, zoom/pan.
 * Nothing here is persisted; it resets on page reload.
 */

import { create } from "zustand";

export type Tool =
  // ── Group 1: Move ──────────────────────────────────────────────────────────
  | "select"      // V   — pointer / selection
  | "hand"        // H   — pan canvas
  | "scale"       // K   — uniform scale
  // ── Group 2: Frame ────────────────────────────────────────────────────────
  | "frame"       // F   — draw frame container
  | "selection"   // S   — marquee (region) select            (Shift+S)
  | "slice"       // S   — define export slice                (S)
  // ── Group 3: Shapes ───────────────────────────────────────────────────────
  | "rect"        // R   — rectangle
  | "line"        // L   — straight line
  | "arrow"       //     — line with arrowhead                (Shift+L)
  | "ellipse"     // O   — ellipse / circle
  | "polygon"     //     — regular polygon
  | "star"        //     — star shape
  | "imageImport" //     — image / video import               (Ctrl+Shift+K)
  // ── Standalone ────────────────────────────────────────────────────────────
  | "text"        // T   — text
  | "path"        // P   — pen / path
  | "prototype"   //     — prototype connections
  | "dev";        // D   — dev mode

interface UiState {
  activeTool: Tool;
  zoom: number;
  panX: number;
  panY: number;

  layersPanelOpen: boolean;
  inspectorOpen: boolean;
  aiPanelOpen: boolean;

  // ── Actions ────────────────────────────────────────────────────────────────

  setTool: (tool: Tool) => void;
  setZoom: (zoom: number) => void;
  setPan: (x: number, y: number) => void;
  resetView: () => void;
  toggleLayersPanel: () => void;
  toggleInspector: () => void;
  toggleAiPanel: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTool: "select",
  zoom: 1,
  panX: 0,
  panY: 0,
  layersPanelOpen: true,
  inspectorOpen: true,
  aiPanelOpen: false,

  setTool: (activeTool) => set({ activeTool }),
  setZoom: (zoom) => set({ zoom: Math.max(0.02, Math.min(256, zoom)) }),
  setPan: (panX, panY) => set({ panX, panY }),
  resetView: () => set({ zoom: 1, panX: 0, panY: 0 }),
  toggleLayersPanel: () => set((s) => ({ layersPanelOpen: !s.layersPanelOpen })),
  toggleInspector: () => set((s) => ({ inspectorOpen: !s.inspectorOpen })),
  toggleAiPanel: () => set((s) => ({ aiPanelOpen: !s.aiPanelOpen })),
}));
