/**
 * components/inspector/Inspector.tsx
 *
 * Right-side inspector panel. Shows properties for the selected shape.
 * M2 scope: position/size editing, fill color picker.
 */

import { useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { useComponentStore } from "../../stores/componentStore";
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
  const { shapes, updateShape, promoteToComponent } = useDocumentStore();
  const {
    components,
    instances,
    registerComponent,
    addProperty,
    removeProperty,
    setVariantProperty,
    resetInstance,
  } = useComponentStore();

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

  // ── Component / Instance helpers ─────────────────────────────────────────

  const isComponent = shape.type === "component";
  const isInstance = shape.type === "instance";
  const compRecord = isComponent ? components[shape.id] : null;
  const instRecord = isInstance ? instances[shape.id] : null;
  const linkedComp = instRecord ? components[instRecord.componentId] : null;

  function handleCreateComponent() {
    // Collect child shapes as defaults
    const defaultShapes: Record<string, Shape> = {};
    for (const cid of shape.children) {
      if (shapes[cid]) defaultShapes[cid] = shapes[cid];
    }
    const meta = { properties: {} };
    const snapshot = promoteToComponent(shape.id, meta);
    if (snapshot) {
      registerComponent(shape.id, shape.name, defaultShapes, shape.children, {});
    }
  }

  function handleAddVariantProperty() {
    const name = prompt("Property name (e.g. 'State')")?.trim();
    if (!name) return;
    const valuesRaw = prompt("Comma-separated values (e.g. 'default,hover,active')")?.trim();
    if (!valuesRaw) return;
    const values = valuesRaw.split(",").map((v) => v.trim()).filter(Boolean);
    if (values.length === 0) return;
    const key = name.toLowerCase().replace(/\s+/g, "-");
    addProperty(shape.id, key, {
      kind: "variant",
      name,
      values,
      defaultValue: values[0],
    });
    updateShape(shape.id, {
      componentMeta: {
        properties: {
          ...(shape.componentMeta?.properties ?? {}),
          [key]: { kind: "variant", name, values, defaultValue: values[0] },
        },
      },
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

      {shape.type === "vector-network" && (
        <Section title="Vector Network">
          <Row label="Anchors">
            <span style={{ fontSize: 12, color: "#cdd6f4" }}>
              {shape.vnAnchors?.length ?? 0}
            </span>
          </Row>
          <Row label="Segments">
            <span style={{ fontSize: 12, color: "#cdd6f4" }}>
              {shape.vnSegments?.length ?? 0}
            </span>
          </Row>
          <Row label="Regions">
            <span style={{ fontSize: 12, color: "#cdd6f4" }}>
              {shape.vnRegions?.length ?? 0}
            </span>
          </Row>
          {shape.vnAnchors && shape.vnAnchors.length > 0 && (
            <div style={{ padding: "4px 12px" }}>
              <div style={{ fontSize: 10, color: "#585b70", textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 4 }}>
                Anchors
              </div>
              <div style={{ maxHeight: 120, overflowY: "auto" }}>
                {shape.vnAnchors.map((a, i) => (
                  <div key={i} style={{ display: "flex", gap: 6, fontSize: 11, color: "#a6adc8", padding: "1px 0" }}>
                    <span style={{ color: "#6c7086", width: 16 }}>{i}</span>
                    <span>x {Math.round(a.x)}</span>
                    <span>y {Math.round(a.y)}</span>
                    {(a.hi || a.ho) && (
                      <span style={{ color: "#585b70" }}>~</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
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

      {/* ── P4.4: Create Component button (non-component, non-instance shapes) ── */}
      {!isComponent && !isInstance && (
        <Section title="Component">
          <div style={{ padding: "6px 12px" }}>
            <button
              onClick={handleCreateComponent}
              style={buttonStyle}
            >
              Create Component
            </button>
          </div>
        </Section>
      )}

      {/* ── P4.4: Component master — property editor ── */}
      {isComponent && compRecord && (
        <Section title="Component Properties">
          {Object.entries(compRecord.properties).length === 0 && (
            <div style={{ padding: "4px 12px", fontSize: 11, color: "#585b70" }}>
              No properties defined.
            </div>
          )}
          {Object.entries(compRecord.properties).map(([key, def]) => (
            <div key={key} style={{ padding: "3px 12px", display: "flex", alignItems: "center", gap: 6 }}>
              <span style={{ flex: 1, fontSize: 11, color: "#a6adc8" }}>
                <span style={{ color: "#6c7086", fontSize: 10, marginRight: 4 }}>
                  {def.kind}
                </span>
                {def.name}
                {def.values && (
                  <span style={{ color: "#585b70", marginLeft: 4 }}>
                    [{def.values.join(", ")}]
                  </span>
                )}
              </span>
              <button
                onClick={() => removeProperty(shape.id, key)}
                style={{ ...buttonStyle, padding: "1px 6px", fontSize: 10, background: "#45475a" }}
                title="Remove property"
              >
                ✕
              </button>
            </div>
          ))}
          <div style={{ padding: "6px 12px" }}>
            <button onClick={handleAddVariantProperty} style={buttonStyle}>
              + Add Variant Property
            </button>
          </div>
        </Section>
      )}

      {/* ── P4.4: Instance — variant dropdowns ── */}
      {isInstance && instRecord && linkedComp && (
        <Section title="Variants">
          {Object.entries(linkedComp.properties).map(([key, def]) => {
            const currentValue = instRecord.variantProperties[key] ?? def.defaultValue;
            return (
              <Row key={key} label={def.name}>
                {def.kind === "variant" && def.values ? (
                  <select
                    value={currentValue}
                    onChange={(e) => {
                      setVariantProperty(shape.id, key, e.target.value);
                      updateShape(shape.id, {
                        instanceMeta: {
                          ...instRecord,
                          variantProperties: { ...instRecord.variantProperties, [key]: e.target.value },
                        },
                      });
                    }}
                    style={selectStyle}
                  >
                    {def.values.map((v) => (
                      <option key={v} value={v}>{v}</option>
                    ))}
                  </select>
                ) : def.kind === "boolean" ? (
                  <input
                    type="checkbox"
                    checked={currentValue === "true"}
                    onChange={(e) => {
                      const val = e.target.checked ? "true" : "false";
                      setVariantProperty(shape.id, key, val);
                      updateShape(shape.id, {
                        instanceMeta: {
                          ...instRecord,
                          variantProperties: { ...instRecord.variantProperties, [key]: val },
                        },
                      });
                    }}
                  />
                ) : (
                  <input
                    type="text"
                    value={currentValue}
                    onChange={(e) => {
                      setVariantProperty(shape.id, key, e.target.value);
                    }}
                    style={{ ...selectStyle, padding: "3px 6px" }}
                  />
                )}
              </Row>
            );
          })}
          {Object.keys(instRecord.overrides).length > 0 && (
            <div style={{ padding: "6px 12px" }}>
              <button
                onClick={() => {
                  resetInstance(shape.id);
                  updateShape(shape.id, {
                    instanceMeta: { ...instRecord, overrides: {} },
                  });
                }}
                style={{ ...buttonStyle, background: "#45475a" }}
              >
                Reset Overrides
              </button>
            </div>
          )}
          <div style={{ padding: "4px 12px", fontSize: 10, color: "#585b70" }}>
            Component: {linkedComp.name}
          </div>
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

const buttonStyle: React.CSSProperties = {
  background: "#313244",
  border: "1px solid #45475a",
  borderRadius: 4,
  color: "#cdd6f4",
  fontSize: 11,
  padding: "4px 10px",
  cursor: "pointer",
  width: "100%",
  textAlign: "left",
};

const selectStyle: React.CSSProperties = {
  width: "100%",
  background: "#313244",
  border: "1px solid #45475a",
  borderRadius: 4,
  color: "#cdd6f4",
  fontSize: 12,
  padding: "3px 6px",
  outline: "none",
};

const numInputStyle: React.CSSProperties = {
  width: "100%",
  background: "#313244",
  border: "1px solid #45475a",
  borderRadius: 4,
  color: "#cdd6f4",
  fontSize: 12,
  padding: "3px 6px",
  outline: "none",
};
