/**
 * components/assets/AssetsPanel.tsx  (P4.4 — Component Variants)
 *
 * Left-side assets panel — shows the component library.
 *
 * Features:
 *  - Lists all registered components from componentStore.
 *  - Click a component row to place an instance at a default position.
 *  - Drag a component row onto the canvas to place the instance at a
 *    custom position (uses HTML5 drag-and-drop; the Canvas picks up the
 *    "logos/component" data transfer to complete the drop).
 */

import { useComponentStore } from "../../stores/componentStore";
import { useDocumentStore } from "../../stores/documentStore";
import type { Shape } from "../../types/shapes";
import { theme } from "../../theme/colors";

// ─────────────────────────────────────────────────────────────────────────────
// Drag payload key shared with Canvas.tsx
// ─────────────────────────────────────────────────────────────────────────────
export const DRAG_COMPONENT_TYPE = "logos/component";

export function AssetsPanel(): React.ReactElement {
  const { components } = useComponentStore();
  const compList = Object.values(components);

  return (
    <div style={panelStyle}>
      <div style={headerStyle}>
        <span style={headerLabel}>Components</span>
        <span style={{ fontSize: 10, color: "#585b70" }}>{compList.length}</span>
      </div>

      {compList.length === 0 ? (
        <div style={emptyStyle}>
          No components yet.{"\n"}
          Select a frame and click{"\n"}
          "Create Component" in the Inspector.
        </div>
      ) : (
        <div style={{ padding: "4px 0" }}>
          {compList.map((comp) => (
            <ComponentRow key={comp.id} componentId={comp.id} name={comp.name} />
          ))}
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// ComponentRow
// ─────────────────────────────────────────────────────────────────────────────

function ComponentRow({
  componentId,
  name,
}: {
  componentId: string;
  name: string;
}): React.ReactElement {
  const { createInstance } = useComponentStore();
  const { shapes, addInstanceShape } = useDocumentStore();

  // Compute a suitable default bounds for a new instance: same size as the
  // component master, placed at a small offset so it's visible.
  function buildDefaultBounds() {
    const compShape: Shape | undefined = shapes[componentId];
    const w = compShape?.bounds.w ?? 200;
    const h = compShape?.bounds.h ?? 100;
    return { x: 20, y: 20, w, h };
  }

  function placeInstance() {
    const bounds = buildDefaultBounds();
    const instanceId = crypto.randomUUID();
    const meta = createInstance(instanceId, componentId);
    addInstanceShape(instanceId, name, bounds, meta);
  }

  function onDragStart(e: React.DragEvent<HTMLDivElement>) {
    e.dataTransfer.setData(DRAG_COMPONENT_TYPE, componentId);
    e.dataTransfer.effectAllowed = "copy";
  }

  return (
    <div
      draggable
      onDragStart={onDragStart}
      onClick={placeInstance}
      style={rowStyle}
      title={`Click to place instance of "${name}"`}
    >
      {/* Component icon */}
      <span style={iconStyle}>◈</span>
      <span style={rowNameStyle}>{name}</span>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Styles
// ─────────────────────────────────────────────────────────────────────────────

const panelStyle: React.CSSProperties = {
  width: 200,
  background: theme.panel,
  borderRight: `1px solid ${theme.border}`,
  display: "flex",
  flexDirection: "column",
  flexShrink: 0,
  overflowY: "auto",
};

const headerStyle: React.CSSProperties = {
  padding: "8px 12px",
  borderBottom: `1px solid ${theme.border}`,
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
};

const headerLabel: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  color: theme.textMuted,
  letterSpacing: "0.05em",
  textTransform: "uppercase",
};

const emptyStyle: React.CSSProperties = {
  padding: "16px 12px",
  fontSize: 11,
  color: "#45475a",
  lineHeight: 1.6,
  whiteSpace: "pre-line",
};

const rowStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "6px 12px",
  cursor: "grab",
  color: theme.text,
  fontSize: 12,
  userSelect: "none",
};

const iconStyle: React.CSSProperties = {
  fontSize: 14,
  color: theme.accent,
  flexShrink: 0,
};

const rowNameStyle: React.CSSProperties = {
  flex: 1,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};
