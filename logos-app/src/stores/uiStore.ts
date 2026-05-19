/**
 * stores/uiStore.ts
 *
 * Ephemeral UI state: active tool, panel visibility, zoom/pan.
 * Nothing here is persisted; it resets on page reload.
 */

import { create } from "zustand";

export type Tool = "select" | "rect" | "ellipse" | "text" | "path" | "hand" | "prototype";

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
