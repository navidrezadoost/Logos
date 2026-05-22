/**
 * stores/toolbarStore.ts
 *
 * UI-only state for the toolbar: which dropdown group is currently open,
 * and which tool is displayed (and therefore active) for each group.
 *
 * Ground truth for the active tool is still `uiStore.activeTool`.
 * This store handles only the visual state of the grouped toolbar buttons.
 */

import { create } from "zustand";
import type { Tool } from "./uiStore";

// ─── Group + tool definitions ─────────────────────────────────────────────────

export interface ToolDef {
  id: Tool;
  label: string;
  /** Keyboard shortcut label string, e.g. "V" or "Shift+L". Empty string = none. */
  shortcut: string;
  /** Unicode / text icon for the button. */
  icon: string;
}

export interface ToolGroup {
  id: string;
  label: string;
  tools: ToolDef[];
}

/** The three grouped tool sections rendered in the top region of the toolbar. */
export const TOOL_GROUPS: ToolGroup[] = [
  {
    id: "move",
    label: "Move tools",
    tools: [
      { id: "select",    label: "Move",  shortcut: "V",           icon: "↖" },
      { id: "hand",      label: "Hand",  shortcut: "H",           icon: "✋" },
      { id: "scale",     label: "Scale", shortcut: "K",           icon: "⤡" },
    ],
  },
  {
    id: "frame",
    label: "Frame tools",
    tools: [
      { id: "frame",     label: "Frame",     shortcut: "F",       icon: "▣" },
      { id: "selection", label: "Selection", shortcut: "Shift+S", icon: "⬚" },
      { id: "slice",     label: "Slice",     shortcut: "S",       icon: "⌂" },
    ],
  },
  {
    id: "shapes",
    label: "Shape tools",
    tools: [
      { id: "rect",        label: "Rectangle",    shortcut: "R",           icon: "▭" },
      { id: "line",        label: "Line",         shortcut: "L",           icon: "╱" },
      { id: "arrow",       label: "Arrow",        shortcut: "Shift+L",     icon: "→" },
      { id: "ellipse",     label: "Ellipse",      shortcut: "O",           icon: "○" },
      { id: "polygon",     label: "Polygon",      shortcut: "",            icon: "△" },
      { id: "star",        label: "Star",         shortcut: "",            icon: "☆" },
      { id: "imageImport", label: "Image/Video…", shortcut: "Ctrl+Shift+K", icon: "⬛" },
    ],
  },
];

/** Default tool id displayed for each group before any selection. */
export const GROUP_DEFAULTS: Record<string, Tool> = {
  move:   "select",
  frame:  "frame",
  shapes: "rect",
};

/** Find which group (if any) owns a given tool. */
export function groupForTool(toolId: Tool): ToolGroup | undefined {
  return TOOL_GROUPS.find((g) => g.tools.some((t) => t.id === toolId));
}

// ─── Store ────────────────────────────────────────────────────────────────────

interface ToolbarState {
  /**
   * ID of the tool group whose dropdown is currently open.
   * `null` means all dropdowns are closed.
   */
  openGroupId: string | null;

  /**
   * For each group, which tool is currently displayed and active.
   * This is updated both when the user picks a tool via the dropdown
   * and when a keyboard shortcut activates a grouped tool.
   */
  activeToolInGroup: Record<string, Tool>;

  // ── Actions ──────────────────────────────────────────────────────────────

  /** Open (or toggle-close) the dropdown for a group. */
  openGroup: (groupId: string | null) => void;

  /** Record that a given tool is now the active/visible tool in its group. */
  setActiveToolInGroup: (groupId: string, tool: Tool) => void;
}

export const useToolbarStore = create<ToolbarState>((set) => ({
  openGroupId: null,
  activeToolInGroup: { ...GROUP_DEFAULTS },

  openGroup: (groupId) =>
    set((s) => ({
      openGroupId: s.openGroupId === groupId ? null : groupId,
    })),

  setActiveToolInGroup: (groupId, tool) =>
    set((s) => ({
      activeToolInGroup: { ...s.activeToolInGroup, [groupId]: tool },
    })),
}));
