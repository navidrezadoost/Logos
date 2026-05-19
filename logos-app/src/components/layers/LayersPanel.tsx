/**
 * components/layers/LayersPanel.tsx  (M3 revision)
 *
 * Virtualized layers panel using @tanstack/react-virtual.
 * Supports 10 000+ shapes without DOM overhead — only the visible rows
 * are mounted.
 */

import { useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCurrentPageShapes, useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore, useIsSelected } from "../../stores/selectionStore";
import type { Shape } from "../../types/shapes";
import { MultiSelectChips, type ScrollMode } from "../ui/MultiSelectChips";

const ITEM_HEIGHT = 32;

export function LayersPanel(): React.ReactElement {
  const shapes = useCurrentPageShapes();
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastSelectedIdRef = useRef<string | null>(null);

  // ── Type filter ───────────────────────────────────────────────────────────
  const allTypes = Array.from(new Set(shapes.map((s) => s.type))).sort() as string[];
  const [filteredTypes, setFilteredTypes] = useState<string[]>([]);
  const [filterScrollMode, setFilterScrollMode] = useState<ScrollMode>("wrap");

  const visibleShapes =
    filteredTypes.length === 0
      ? shapes
      : shapes.filter((s) => filteredTypes.includes(s.type));

  function handleRowSelect(e: React.MouseEvent<HTMLDivElement>, shape: Shape) {
    const { select, toggleSelect, selectRange } = useSelectionStore.getState();

    if (e.shiftKey && lastSelectedIdRef.current) {
      const start = visibleShapes.findIndex((s) => s.id === lastSelectedIdRef.current);
      const end = visibleShapes.findIndex((s) => s.id === shape.id);
      if (start !== -1 && end !== -1) {
        const [from, to] = start < end ? [start, end] : [end, start];
          selectRange(visibleShapes.slice(from, to + 1).map((s) => s.id));
        return;
      }
    }

    if (e.metaKey || e.ctrlKey) {
      toggleSelect(shape.id);
      lastSelectedIdRef.current = shape.id;
      return;
    }

    select(shape.id);
    lastSelectedIdRef.current = shape.id;
  }

  const virtualizer = useVirtualizer({
    count: visibleShapes.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ITEM_HEIGHT,
    overscan: 8,
  });

  return (
    <div style={panelStyle}>
      <div style={headerStyle}>Layers</div>

      {/* Type filter bar — only shown when shapes exist */}
      {allTypes.length > 0 && (
        <div style={{ padding: "6px 8px", borderBottom: "1px solid #313244" }}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
            <span style={{ fontSize: 10, color: "#585b70", textTransform: "uppercase", letterSpacing: "0.05em" }}>
              Filter
            </span>
            <div style={{ display: "flex", gap: 2 }}>
              {(["wrap", "scroll", "truncate"] as ScrollMode[]).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setFilterScrollMode(mode)}
                  title={`${mode} mode`}
                  style={{
                    background: filterScrollMode === mode ? "#313244" : "transparent",
                    border: "1px solid",
                    borderColor: filterScrollMode === mode ? "#45475a" : "transparent",
                    borderRadius: 3,
                    color: filterScrollMode === mode ? "#cdd6f4" : "#45475a",
                    fontSize: 10,
                    padding: "1px 4px",
                    cursor: "pointer",
                  }}
                >
                  {mode[0].toUpperCase()}
                </button>
              ))}
            </div>
          </div>
          <MultiSelectChips
            options={allTypes}
            selected={filteredTypes}
            onChange={setFilteredTypes}
            getLabel={(v) => v}
            getIcon={(v) => shapeIcon(v)}
            scrollMode={filterScrollMode}
            maxVisible={3}
            allowEmpty
          />
        </div>
      )}
      {visibleShapes.length === 0 ? (
        <div style={{ padding: 12, color: "#45475a", fontSize: 11 }}>
          {filteredTypes.length > 0 ? "No shapes match the active filter." : "No shapes yet. Press R to add one."}
        </div>
      ) : (
        <div ref={scrollRef} style={{ overflowY: "auto", flex: 1 }}>
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((vRow) => {
              const shape = visibleShapes[vRow.index];
              return (
                <div
                  key={shape.id}
                  style={{
                    position: "absolute",
                    top: vRow.start,
                    left: 0,
                    right: 0,
                    height: ITEM_HEIGHT,
                  }}
                >
                  <ShapeRow shape={shape} onSelect={handleRowSelect} />
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function ShapeRow({
  shape,
  onSelect,
}: {
  shape: Shape;
  onSelect: (e: React.MouseEvent<HTMLDivElement>, shape: Shape) => void;
}): React.ReactElement {
  const isSelected = useIsSelected(shape.id);
  const { removeShape } = useDocumentStore();

  return (
    <div
      onClick={(e) => onSelect(e, shape)}
      style={{
        display: "flex",
        alignItems: "center",
        height: ITEM_HEIGHT,
        padding: "0 8px",
        gap: 6,
        cursor: "pointer",
        background: isSelected ? "#313244" : "transparent",
        borderLeft: isSelected ? "2px solid #cba6f7" : "2px solid transparent",
        fontSize: 12,
        color: "#cdd6f4",
        userSelect: "none",
      }}
      title="Click to select · Ctrl/Cmd-click to toggle · Shift-click for range"
      onMouseEnter={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = "#1e1e2e";
      }}
      onMouseLeave={(e) => {
        if (!isSelected) (e.currentTarget as HTMLDivElement).style.background = "transparent";
      }}
    >
      <span style={{ color: "#7f849c", fontSize: 13 }}>{shapeIcon(shape.type)}</span>
      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {shape.name}
      </span>
      <button
        onClick={(e) => { e.stopPropagation(); removeShape(shape.id); }}
        title="Delete"
        style={{
          background: "none",
          border: "none",
          color: "#585b70",
          cursor: "pointer",
          padding: "0 2px",
          fontSize: 14,
          lineHeight: 1,
        }}
        onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = "#f38ba8")}
        onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = "#585b70")}
      >
        ×
      </button>
    </div>
  );
}

function shapeIcon(type: string): string {
  switch (type) {
    case "rect": return "▭";
    case "ellipse":
    case "circle": return "○";
    case "text": return "T";
    case "path":
    case "bool": return "✏";
    case "frame": return "⬜";
    case "group": return "▤";
    default: return "◻";
  }
}

const panelStyle: React.CSSProperties = {
  width: 220,
  background: "#181825",
  borderRight: "1px solid #313244",
  display: "flex",
  flexDirection: "column",
  flexShrink: 0,
};

const headerStyle: React.CSSProperties = {
  padding: "8px 12px",
  fontSize: 11,
  fontWeight: 600,
  color: "#7f849c",
  letterSpacing: "0.05em",
  textTransform: "uppercase",
  borderBottom: "1px solid #313244",
};
