import { create } from "zustand";

export interface TokenColorPreview {
  /** Identifies the one transient workflow that currently owns the preview. */
  ownerId: string;
  documentId: string | null;
  pageId: string;
  shapeId: string;
  fillIndex: number;
  color: string;
}

interface TokenWorkflowPreviewState {
  preview: TokenColorPreview | null;
  setPreview: (preview: TokenColorPreview) => void;
  clearPreview: (ownerId?: string) => void;
}

/** A single, page-scoped Canvas projection; never persisted or copied to the document store. */
export const useTokenWorkflowPreviewStore = create<TokenWorkflowPreviewState>((set) => ({
  preview: null,
  setPreview: (preview) => set({ preview }),
  clearPreview: (ownerId) => set((state) => {
    if (ownerId && state.preview?.ownerId !== ownerId) return state;
    return state.preview ? { preview: null } : state;
  }),
}));
