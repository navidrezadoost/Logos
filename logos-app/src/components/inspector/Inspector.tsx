/**
 * components/inspector/Inspector.tsx
 *
 * Right-side inspector panel. Shows properties for the selected shape.
 * M2 scope: position/size editing, fill color picker.
 */

import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import type { FontVariationAxis, Shape, SolidFill } from "../../types/shapes";

// ---------------------------------------------------------------------------
// Well-known variable font axes (fvar table metadata when font introspection
// is unavailable). Covers the five registered OpenType axes plus common custom
// axes used by Google Fonts variable fonts.
// ---------------------------------------------------------------------------
const KNOWN_AXES: Omit<FontVariationAxis, "value">[] = [
  { tag: "wght", name: "Weight",       min: 100,  max: 900,  default: 400 },
  { tag: "wdth", name: "Width",        min: 75,   max: 125,  default: 100 },
  { tag: "slnt", name: "Slant",        min: -90,  max: 90,   default: 0   },
  { tag: "opsz", name: "Optical Size", min: 6,    max: 144,  default: 14  },
  { tag: "ital", name: "Italic",       min: 0,    max: 1,    default: 0   },
];

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

  // Variable font axes — only shown for text shapes
  const isText = shape.type === "text";
  const variationSettings = shape.fontVariationSettings ?? {};
  // Build displayed axes: known axes + any extra axes already stored on the shape
  const displayAxes: FontVariationAxis[] = [
    ...KNOWN_AXES.map((known) => ({
      ...known,
      value: variationSettings[known.tag] ?? known.default ?? 0,
    })),
    // Extra axes set on the shape that aren't in KNOWN_AXES
    ...Object.entries(variationSettings)
      .filter(([tag]) => !KNOWN_AXES.some((k) => k.tag === tag))
      .map(([tag, value]) => ({ tag, name: tag, value, min: -1000, max: 1000 })),
  ];

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

  function patchVariationAxis(tag: string, value: number) {
    updateShape(shape.id, {
      fontVariationSettings: { ...variationSettings, [tag]: value },
    });
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

      {isText && (
        <Section title="Variable Axes">
          {displayAxes.map((axis) => (
            <AxisSlider
              key={axis.tag}
              axis={axis}
              onChange={(v) => patchVariationAxis(axis.tag, v)}
            />
          ))}
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

/**
 * A single variable-font axis row: label on left, range slider in the middle,
 * and a numeric input on the right.
 */
function AxisSlider({
  axis,
  onChange,
}: {
  axis: FontVariationAxis;
  onChange: (v: number) => void;
}): React.ReactElement {
  const min = axis.min ?? -1000;
  const max = axis.max ?? 1000;
  const step = (max - min) <= 1 ? 0.01 : 1;

  return (
    <div style={{ padding: "4px 12px" }}>
      {/* Row 1: tag + name + numeric input */}
      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 3 }}>
        <span style={{ fontSize: 10, color: "#585b70", fontFamily: "monospace", flexShrink: 0 }}>
          {axis.tag}
        </span>
        <span style={{ fontSize: 11, color: "#a6adc8", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {axis.name && axis.name !== axis.tag ? axis.name : ""}
        </span>
        <input
          type="number"
          value={axis.tag === "ital" ? axis.value : Math.round(axis.value)}
          min={min}
          max={max}
          step={step}
          onChange={(e) => {
            const v = parseFloat(e.target.value);
            if (!isNaN(v)) onChange(Math.min(max, Math.max(min, v)));
          }}
          style={{
            width: 52,
            background: "#313244",
            border: "1px solid #45475a",
            borderRadius: 4,
            color: "#cdd6f4",
            fontSize: 11,
            padding: "2px 4px",
            outline: "none",
            textAlign: "right",
            flexShrink: 0,
          }}
        />
      </div>
      {/* Row 2: range slider */}
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={axis.value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        style={{
          width: "100%",
          accentColor: "#89b4fa",
          cursor: "pointer",
        }}
      />
    </div>
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
