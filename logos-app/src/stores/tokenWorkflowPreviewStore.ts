import { create } from "zustand";

interface TokenWorkflowPreviewState {
  colors: Record<string, { fillIndex: number; color: string }>;
  setColor: (shapeId: string, fillIndex: number, color: string) => void;
  clearColor: (shapeId: string) => void;
  clearAll: () => void;
}

/** Ephemeral canvas projection for the token workflow; deliberately not persisted. */
export const useTokenWorkflowPreviewStore = create<TokenWorkflowPreviewState>((set) => ({
  colors: {},
  setColor: (shapeId, fillIndex, color) => set((state) => ({ colors: { ...state.colors, [shapeId]: { fillIndex, color } } })),
  clearColor: (shapeId) => set((state) => {
    if (!(shapeId in state.colors)) return state;
    const colors = { ...state.colors };
    delete colors[shapeId];
    return { colors };
  }),
  clearAll: () => set({ colors: {} }),
}));
