/**
 * components/toolbar/Toolbar.tsx
 *
 * Vertical icon toolbar — left edge of the workspace.
 *
 * Structure:
 *   ┌─────────────┐
 *   │  [Move ▾]   │  Group 1 — one slot; icon = active tool, ▾ opens list
 *   │  [Frame ▾]  │  Group 2 — frame / selection / slice
 *   │  [Rect ▾]   │  Group 3 — rect / line / arrow / ellipse / polygon / star / image
 *   │  ───────    │
 *   │  [T] text   │  Standalone
 *   │  [✏] pen   │
 *   │  ───────    │
 *   │  [⬡] proto │
 *   │  [</>] dev  │
 *   │  ───────    │
 *   │  1:1        │  Reset view
 *   │  ───────    │  (Boolean ops — only when 2 VN shapes selected)
 *   └─────────────┘
 *
 * Keyboard shortcuts:
 *   V           Move (select)
 *   H           Hand
 *   K           Scale
 *   F           Frame
 *   Shift+S     Region Selection
 *   S           Slice
 *   R           Rectangle
 *   L           Line
 *   Shift+L     Arrow
 *   O           Ellipse
 *   T           Text
 *   P           Pen
 *   D           Dev mode
 *   Ctrl+Shift+K  Image/Video import
 *   0           Reset view
 *   Escape      Clear selection + revert to Select
 */

import React, { useCallback, useEffect, useRef, useState } from "react";
import { useUiStore, type Tool } from "../../stores/uiStore";
import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { workerPool } from "../../worker";
import type { BoolOp } from "../../worker/vector-network.types";
import {
  useToolbarStore,
  TOOL_GROUPS,
  groupForTool,
} from "../../stores/toolbarStore";
import { ToolButton } from "./ToolButton";
import { ToolGroupButton } from "./ToolGroupButton";
import { ToolDropdown } from "./ToolDropdown";
import { theme } from "../../theme/colors";
import type { ToolbarIconName } from "./toolbarIcons";
import { BOOL_OP_ICONS, ToolbarIcon } from "./toolbarIcons";

// ─── Standalone (non-grouped) tool buttons ────────────────────────────────────

interface StandaloneToolDef {
  tool: Tool;
  icon: ToolbarIconName;
  label: string;
  shortcut: string;
}

const STANDALONE_TOOLS: StandaloneToolDef[] = [
  { tool: "text", icon: "text", label: "Text", shortcut: "T" },
  { tool: "path", icon: "path", label: "Pen",  shortcut: "P" },
];

const BOTTOM_TOOLS: StandaloneToolDef[] = [
  { tool: "prototype", icon: "prototype", label: "Prototype", shortcut: "" },
  { tool: "dev",       icon: "dev",       label: "Dev Mode",  shortcut: "D" },
];

const BOOL_OPS: { op: BoolOp; label: string }[] = [
  { op: "union",     label: "Union"     },
  { op: "intersect", label: "Intersect" },
  { op: "subtract",  label: "Subtract"  },
  { op: "exclude",   label: "Exclude"   },
];

// ─── Component ────────────────────────────────────────────────────────────────

export function Toolbar(): React.ReactElement {
  const { activeTool, setTool, resetView } = useUiStore();
  const { shapes, replaceWithVectorNetwork } = useDocumentStore();
  const { clearSelection, selectedIds } = useSelectionStore();
  const { openGroupId, activeToolInGroup, openGroup, setActiveToolInGroup } = useToolbarStore();

  // Per-group button container refs for positioning dropdowns
  const groupBtnRefs = useRef<Record<string, HTMLDivElement | null>>({});

  // Top pixel offset for the currently open dropdown
  const [dropdownTop, setDropdownTop] = useState(0);

  // Hidden file input for image / video import
  const imageInputRef = useRef<HTMLInputElement>(null);

  // ── Activate tool (keeps toolbarStore group state in sync) ───────────────
  const activateTool = useCallback(
    (tool: Tool) => {
      const group = groupForTool(tool);
      if (group) {
        setActiveToolInGroup(group.id, tool);
      }

      // Image import: trigger file picker immediately, then revert to select
      if (tool === "imageImport") {
        imageInputRef.current?.click();
        setTool("select");
        return;
      }

      setTool(tool);
    },
    [setTool, setActiveToolInGroup]
  );

  // ── Keyboard shortcuts ───────────────────────────────────────────────────
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      const shift = e.shiftKey;
      const ctrl  = e.ctrlKey || e.metaKey;
      const key   = e.key.toLowerCase();

      // Ctrl+Shift+K → image import
      if (ctrl && shift && key === "k") { e.preventDefault(); activateTool("imageImport"); return; }
      if (ctrl) return; // don't intercept other ctrl combos

      switch (key) {
        case "v":  activateTool("select");    break;
        case "h":  activateTool("hand");      break;
        case "k":  activateTool("scale");     break;
        case "f":  activateTool("frame");     break;
        case "s":
          if (shift) activateTool("selection");
          else       activateTool("slice");
          break;
        case "r":  activateTool("rect");      break;
        case "l":
          if (shift) activateTool("arrow");
          else       activateTool("line");
          break;
        case "o":  activateTool("ellipse");   break;
        case "t":  activateTool("text");      break;
        case "p":  activateTool("path");      break;
        case "d":  activateTool("dev");       break;
        case "0":  resetView();               break;
        case "escape":
          clearSelection();
          activateTool("select");
          break;
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activateTool, clearSelection, resetView]);

  // ── Image file handler ───────────────────────────────────────────────────
  function handleImageFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    // TODO: import as image-fill shape (P4.8 media pipeline follow-up)
    console.log("[Toolbar] Image/video selected for import:", file.name);
    e.target.value = "";
  }

  // ── Boolean ops ──────────────────────────────────────────────────────────
  const selectedVN = selectedIds.filter(
    (id) => shapes[id]?.type === "vector-network"
  );
  const canBoolOp = selectedVN.length === 2;

  async function runBoolOp(op: BoolOp) {
    if (!canBoolOp) return;
    const [idA, idB] = selectedVN;
    const shapeA = shapes[idA];
    const shapeB = shapes[idB];
    const netA = { anchors: shapeA.vnAnchors ?? [], segments: shapeA.vnSegments ?? [] };
    const netB = { anchors: shapeB.vnAnchors ?? [], segments: shapeB.vnSegments ?? [] };
    try {
      const [regA, regB] = await Promise.all([
        workerPool.findRegions({ net: netA }),
        workerPool.findRegions({ net: netB }),
      ]);
      if (!regA.ok || !regB.ok) { console.warn("[BoolOp] find_regions failed", regA, regB); return; }
      const result = await workerPool.boolOp({
        net_a: netA, net_b: netB,
        region_a: regA.regions[0] ?? [],
        region_b: regB.regions[0] ?? [],
        op,
      });
      if (!result.ok) { console.warn("[BoolOp] boolean_op failed", result); return; }
      replaceWithVectorNetwork([idA, idB], result.anchors, result.segments, result.regions);
    } catch (err) {
      console.error("[BoolOp] worker error", err);
    }
  }

  // ── Open a group dropdown (chevron / right-click) ─────────────────────────
  function handleOpenGroupMenu(groupId: string) {
    const btnEl = groupBtnRefs.current[groupId];
    if (btnEl) {
      setDropdownTop(btnEl.getBoundingClientRect().top);
    }
    openGroup(openGroupId === groupId ? null : groupId);
  }

  function handleActivateGroupTool(groupId: string, toolId: Tool) {
    if (openGroupId === groupId) {
      openGroup(null);
    }
    activateTool(toolId);
  }

  // ────────────────────────────────────────────────────────────────────────
  return (
    <div
      style={{
        width: 48,
        background: theme.panel,
        borderRight: `1px solid ${theme.border}`,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        paddingTop: 8,
        gap: 4,
        flexShrink: 0,
      }}
    >
      {/* Hidden image / video file input */}
      <input
        ref={imageInputRef}
        type="file"
        accept="image/*,video/*"
        style={{ display: "none" }}
        onChange={handleImageFile}
      />

      {/* ── Grouped tool buttons ─────────────────────────────────────────── */}
      {TOOL_GROUPS.map((group) => {
        const displayToolId = activeToolInGroup[group.id] ?? group.tools[0].id;
        const displayTool   = group.tools.find((t) => t.id === displayToolId) ?? group.tools[0];
        const groupActive   = group.tools.some((t) => t.id === activeTool);
        const dropdownOpen  = openGroupId === group.id;

        return (
          <div
            key={group.id}
            ref={(el) => { groupBtnRefs.current[group.id] = el; }}
          >
            <ToolGroupButton
              icon={displayTool.icon}
              label={displayTool.label}
              shortcut={displayTool.shortcut}
              active={groupActive}
              menuOpen={dropdownOpen}
              onActivate={() => handleActivateGroupTool(group.id, displayToolId)}
              onOpenMenu={() => handleOpenGroupMenu(group.id)}
            />

            {dropdownOpen && (
              <ToolDropdown
                tools={group.tools}
                activeToolId={displayToolId}
                topOffset={dropdownTop}
                onSelect={(toolId) => {
                  setActiveToolInGroup(group.id, toolId);
                  activateTool(toolId);
                }}
                onClose={() => openGroup(null)}
              />
            )}
          </div>
        );
      })}

      {/* Divider */}
      <div style={{ width: 28, height: 1, background: theme.border, margin: "4px 0" }} />

      {/* ── Standalone: text + pen ───────────────────────────────────────── */}
      {STANDALONE_TOOLS.map((btn) => (
        <ToolButton
          key={btn.tool}
          icon={btn.icon}
          label={btn.label}
          shortcut={btn.shortcut}
          active={activeTool === btn.tool}
          onClick={() => activateTool(btn.tool)}
        />
      ))}

      {/* Divider */}
      <div style={{ width: 28, height: 1, background: theme.border, margin: "4px 0" }} />

      {/* ── Bottom: prototype + dev ──────────────────────────────────────── */}
      {BOTTOM_TOOLS.map((btn) => (
        <ToolButton
          key={btn.tool}
          icon={btn.icon}
          label={btn.label}
          shortcut={btn.shortcut}
          active={activeTool === btn.tool}
          onClick={() => activateTool(btn.tool)}
        />
      ))}

      {/* Divider */}
      <div style={{ width: 28, height: 1, background: theme.border, margin: "4px 0" }} />

      {/* Reset view */}
      <button
        title="Reset view (0)"
        onClick={resetView}
        style={{
          width: 36, height: 36, borderRadius: 6, border: "none",
          background: "transparent", color: theme.textDim, cursor: "pointer",
          display: "flex", alignItems: "center", justifyContent: "center",
        }}
      >
        <ToolbarIcon name="resetView" size={16} />
      </button>

      {/* Boolean ops — shown only when exactly 2 vector-network shapes are selected */}
      {canBoolOp && (
        <>
          <div style={{ width: 28, height: 1, background: theme.border, margin: "4px 0" }} />
          <div style={{
            fontSize: 9, color: theme.textDim, textTransform: "uppercase",
            letterSpacing: "0.05em", textAlign: "center",
          }}>
            Bool
          </div>
          {BOOL_OPS.map(({ op, label }) => {
            const Icon = BOOL_OP_ICONS[op];
            return (
            <button
              key={op}
              title={label}
              onClick={() => runBoolOp(op)}
              style={{
                width: 36, height: 36, borderRadius: 6, border: "none",
                background: "transparent", color: theme.text, cursor: "pointer",
                display: "flex", alignItems: "center", justifyContent: "center",
              }}
            >
              <Icon size={18} aria-hidden focusable={false} />
            </button>
            );
          })}
        </>
      )}
    </div>
  );
}
