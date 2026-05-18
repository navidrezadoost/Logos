/**
 * components/toolbar/Toolbar.tsx
 *
 * Vertical icon toolbar (left edge of the workspace).
 * Keyboard shortcuts: V=select, R=rect, O=ellipse, T=text, H=hand
 */

import { useEffect } from "react";
import { useUiStore, type Tool } from "../../stores/uiStore";
import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";

interface ToolButton {
  tool: Tool;
  icon: string;
  label: string;
  shortcut: string;
}

const TOOLS: ToolButton[] = [
  { tool: "select",  icon: "↖",  label: "Select",   shortcut: "V" },
  { tool: "rect",    icon: "▭",  label: "Rectangle", shortcut: "R" },
  { tool: "ellipse", icon: "○",  label: "Ellipse",   shortcut: "O" },
  { tool: "text",    icon: "T",  label: "Text",      shortcut: "T" },
  { tool: "path",    icon: "✏",  label: "Path",      shortcut: "P" },
  { tool: "hand",    icon: "✋", label: "Pan",       shortcut: "H" },
];

export function Toolbar(): React.ReactElement {
  const { activeTool, setTool, resetView } = useUiStore();
  const { addRect } = useDocumentStore();
  const { clearSelection } = useSelectionStore();

  // ── Keyboard shortcuts ─────────────────────────────────────────────────────
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      // Ignore when typing in an input
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;

      switch (e.key.toLowerCase()) {
        case "v": setTool("select"); break;
        case "r":
          setTool("rect");
          // Immediately add a rectangle at a default position
          addRect({ x: 50 + Math.random() * 200, y: 50 + Math.random() * 150, w: 200, h: 100 });
          setTool("select");
          break;
        case "o": setTool("ellipse"); break;
        case "t": setTool("text"); break;
        case "p": setTool("path"); break;
        case "h": setTool("hand"); break;
        case "0": resetView(); break;
        case "escape": clearSelection(); setTool("select"); break;
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setTool, addRect, clearSelection, resetView]);

  // ── Handle toolbar button click ────────────────────────────────────────────
  function handleToolClick(btn: ToolButton) {
    setTool(btn.tool);
    if (btn.tool === "rect") {
      addRect({ x: 50 + Math.random() * 200, y: 50 + Math.random() * 150, w: 200, h: 100 });
      setTool("select");
    }
  }

  return (
    <div
      style={{
        width: 48,
        background: "#1e1e2e",
        borderRight: "1px solid #313244",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        paddingTop: 8,
        gap: 4,
        flexShrink: 0,
      }}
    >
      {TOOLS.map((btn) => (
        <button
          key={btn.tool}
          title={`${btn.label} (${btn.shortcut})`}
          onClick={() => handleToolClick(btn)}
          style={{
            width: 36,
            height: 36,
            borderRadius: 6,
            border: "none",
            background: activeTool === btn.tool ? "#cba6f7" : "transparent",
            color: activeTool === btn.tool ? "#1e1e2e" : "#cdd6f4",
            fontSize: 16,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            transition: "background 0.1s",
          }}
        >
          {btn.icon}
        </button>
      ))}

      {/* Divider */}
      <div style={{ width: 28, height: 1, background: "#313244", margin: "4px 0" }} />

      {/* Reset view */}
      <button
        title="Reset view (0)"
        onClick={resetView}
        style={{
          width: 36, height: 36, borderRadius: 6, border: "none",
          background: "transparent", color: "#6c7086", fontSize: 11,
          cursor: "pointer", fontFamily: "monospace",
        }}
      >
        1:1
      </button>
    </div>
  );
}
