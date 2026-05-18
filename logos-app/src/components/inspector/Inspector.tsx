/**
 * components/inspector/Inspector.tsx
 *
 * Right-side inspector panel. Shows properties for the selected shape.
 * M2 scope: position/size editing, fill color picker.
 */

import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import type { Shape, SolidFill } from "../../types/shapes";

export function Inspector(): React.ReactElement {
  const selectedIds = useSelectionStore((s) => s.selectedIds);
  const { shapes, updateShape } = useDocumentStore();

  const selected: Shape[] = selectedIds.flatMap((id) =>
    shapes[id] ? [shapes[id]] : []
  );

  if (selected.length === 0) {
    return (
      <div style={panelStyle}>
        <Header title="No selection" />
        <div style={{ padding: 16, color: "#45475a", fontSize: 12 }}>
          Click a shape to inspect it.
        </div>
      </div>
    );
  }

  if (selected.length > 1) {
    return (
      <div style={panelStyle}>
        <Header title={`${selected.length} shapes`} />
        <div style={{ padding: 16, color: "#a6adc8", fontSize: 12 }}>
          Multiple shapes selected.
        </div>
      </div>
    );
  }

  const shape = selected[0];
  const solidFill = shape.fills.find((f): f is SolidFill => f.type === "solid");

  function patchBounds(key: "x" | "y" | "w" | "h", value: number) {
    updateShape(shape.id, { bounds: { ...shape.bounds, [key]: value } });
  }

  function patchFillColor(color: string) {
    updateShape(shape.id, {
      fills: shape.fills.map((f) =>
        f.type === "solid" ? { ...f, color } : f
      ),
    });
  }

  function patchOpacity(opacity: number) {
    updateShape(shape.id, { opacity });
  }

  return (
    <div style={panelStyle}>
      <Header title={shape.name} />

      <Section title="Layout">
        <Row label="X">
          <NumInput value={shape.bounds.x} onChange={(v) => patchBounds("x", v)} />
        </Row>
        <Row label="Y">
          <NumInput value={shape.bounds.y} onChange={(v) => patchBounds("y", v)} />
        </Row>
        <Row label="W">
          <NumInput value={shape.bounds.w} min={1} onChange={(v) => patchBounds("w", v)} />
        </Row>
        <Row label="H">
          <NumInput value={shape.bounds.h} min={1} onChange={(v) => patchBounds("h", v)} />
        </Row>
      </Section>

      {solidFill && (
        <Section title="Fill">
          <Row label="Color">
            <input
              type="color"
              value={solidFill.color}
              onChange={(e) => patchFillColor(e.target.value)}
              style={{ width: "100%", height: 28, border: "1px solid #313244", borderRadius: 4, cursor: "pointer", background: "none" }}
            />
          </Row>
          <Row label="Opacity">
            <NumInput
              value={Math.round(shape.opacity * 100)}
              min={0}
              max={100}
              onChange={(v) => patchOpacity(v / 100)}
            />
          </Row>
        </Section>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sub-components
// ─────────────────────────────────────────────────────────────────────────────

function Header({ title }: { title: string }): React.ReactElement {
  return (
    <div style={{ padding: "8px 12px", borderBottom: "1px solid #313244" }}>
      <span style={{ fontSize: 11, fontWeight: 600, color: "#7f849c", letterSpacing: "0.05em", textTransform: "uppercase" }}>
        {title}
      </span>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div style={{ borderBottom: "1px solid #313244", paddingBottom: 8 }}>
      <div style={{ padding: "8px 12px 4px", fontSize: 10, color: "#585b70", textTransform: "uppercase", letterSpacing: "0.06em" }}>
        {title}
      </div>
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div style={{ display: "flex", alignItems: "center", padding: "2px 12px", gap: 8 }}>
      <span style={{ width: 28, fontSize: 11, color: "#6c7086", flexShrink: 0 }}>{label}</span>
      <div style={{ flex: 1 }}>{children}</div>
    </div>
  );
}

function NumInput({
  value,
  min,
  max,
  onChange,
}: {
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
}): React.ReactElement {
  return (
    <input
      type="number"
      value={Math.round(value)}
      min={min}
      max={max}
      onChange={(e) => {
        const v = parseFloat(e.target.value);
        if (!isNaN(v)) onChange(v);
      }}
      style={{
        width: "100%",
        background: "#313244",
        border: "1px solid #45475a",
        borderRadius: 4,
        color: "#cdd6f4",
        fontSize: 12,
        padding: "3px 6px",
        outline: "none",
      }}
    />
  );
}

const panelStyle: React.CSSProperties = {
  width: 220,
  background: "#181825",
  borderLeft: "1px solid #313244",
  display: "flex",
  flexDirection: "column",
  flexShrink: 0,
  overflowY: "auto",
};
