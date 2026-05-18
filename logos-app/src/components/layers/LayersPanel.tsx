/**
 * components/layers/LayersPanel.tsx
 *
 * Layers panel — scrollable list of shapes on the current page.
 * Plain CSS-scroll (no react-window dependency) for M2; virtualize in M3+.
 */

import { useCurrentPageShapes, useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore, useIsSelected } from "../../stores/selectionStore";
import type { Shape } from "../../types/shapes";

export function LayersPanel(): React.ReactElement {
  const shapes = useCurrentPageShapes();

  return (
    <div style={panelStyle}>
      <div style={headerStyle}>Layers</div>
      {shapes.length === 0 ? (
        <div style={{ padding: 12, color: "#45475a", fontSize: 11 }}>
          No shapes yet. Press R to add one.
        </div>
      ) : (
        <div style={{ overflowY: "auto", flex: 1 }}>
          {shapes.map((shape) => (
            <ShapeRow key={shape.id} shape={shape} />
          ))}
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────

function ShapeRow({ shape }: { shape: Shape }): React.ReactElement {
  const isSelected = useIsSelected(shape.id);
  const { select } = useSelectionStore();
  const { removeShape } = useDocumentStore();

  return (
    <div
      onClick={() => select(shape.id)}
      style={{
        display: "flex",
        alignItems: "center",
        height: 32,
        padding: "0 8px",
        gap: 6,
        cursor: "pointer",
        background: isSelected ? "#313244" : "transparent",
        borderLeft: isSelected ? "2px solid #cba6f7" : "2px solid transparent",
        fontSize: 12,
        color: "#cdd6f4",
        userSelect: "none",
      }}
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
    case "ellipse": return "○";
    case "text": return "T";
    case "path": return "✏";
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
