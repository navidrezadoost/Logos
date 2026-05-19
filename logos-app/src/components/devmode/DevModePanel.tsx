/**
 * components/devmode/DevModePanel.tsx
 *
 * P4.9 Dev Mode — Right-side inspection panel.
 *
 * When the "Dev" tool is active, this panel replaces the standard Inspector
 * and shows copy-pasteable CSS properties for the selected (or hovered) shape.
 *
 * Features
 * ─────────
 * - Grouped CSS properties (Layout, Fill, Opacity, Transform, …)
 * - Click-to-copy individual prop lines with "Copied!" flash feedback
 * - "Copy CSS Block" button copies a full `.class-name { … }` rule
 * - Hover-inspect: hovering a shape in Dev mode highlights it
 * - Shape metadata header: type pill, name, and UUID
 */

import React, { useCallback } from "react";
import { useSelectionStore } from "../../stores/selectionStore";
import { useDocumentStore } from "../../stores/documentStore";
import { useDevModeStore } from "../../stores/devModeStore";
import { generateCssGroups, generateCssBlock } from "../../utils/cssCodegen";
import type { Shape } from "../../types/shapes";
import type { CssGroup } from "../../utils/cssCodegen";

// ─────────────────────────────────────────────────────────────────────────────
// Styles
// ─────────────────────────────────────────────────────────────────────────────

const panelStyle: React.CSSProperties = {
  width: 248,
  minWidth: 248,
  background: "#181825",
  borderLeft: "1px solid #313244",
  display: "flex",
  flexDirection: "column",
  flexShrink: 0,
  overflowY: "auto",
  fontFamily: "'Inter', system-ui, sans-serif",
  fontSize: 12,
  color: "#cdd6f4",
};

// ─────────────────────────────────────────────────────────────────────────────
// Sub-components
// ─────────────────────────────────────────────────────────────────────────────

/** Mode banner at the very top. */
function DevModeBanner(): React.ReactElement {
  return (
    <div
      style={{
        background: "#1e1e2e",
        borderBottom: "1px solid #313244",
        padding: "6px 12px",
        display: "flex",
        alignItems: "center",
        gap: 6,
      }}
    >
      <span style={{ color: "#a6e3a1", fontWeight: 700, fontSize: 10, letterSpacing: 1 }}>
        DEV MODE
      </span>
      <span style={{ color: "#45475a", fontSize: 10 }}>— inspection only</span>
    </div>
  );
}

/** Shape metadata header (name, type pill, id). */
function ShapeHeader({ shape }: { shape: Shape }): React.ReactElement {
  const typePillColors: Record<string, string> = {
    frame: "#cba6f7",
    rect: "#89b4fa",
    circle: "#89dceb",
    ellipse: "#89dceb",
    text: "#f9e2af",
    path: "#fab387",
    group: "#a6e3a1",
    component: "#cba6f7",
    instance: "#b4befe",
    "vector-network": "#f38ba8",
    "svg-raw": "#74c7ec",
    bool: "#eba0ac",
  };
  const pill = typePillColors[shape.type] ?? "#6c7086";

  return (
    <div
      style={{
        padding: "10px 12px 8px",
        borderBottom: "1px solid #313244",
      }}
    >
      {/* Name + type pill */}
      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
        <span
          style={{
            background: pill + "22",
            border: `1px solid ${pill}66`,
            color: pill,
            borderRadius: 4,
            padding: "1px 6px",
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: 0.5,
            textTransform: "uppercase",
            flexShrink: 0,
          }}
        >
          {shape.type}
        </span>
        <span
          style={{
            color: "#cdd6f4",
            fontWeight: 600,
            fontSize: 13,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {shape.name}
        </span>
      </div>

      {/* UUID */}
      <div
        style={{
          color: "#585b70",
          fontSize: 10,
          fontFamily: "monospace",
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {shape.id}
      </div>

      {/* Size summary */}
      <div
        style={{
          marginTop: 4,
          color: "#a6adc8",
          fontSize: 11,
        }}
      >
        {Math.round(shape.bounds.w)} × {Math.round(shape.bounds.h)}
        {shape.rotation !== 0 && ` · ${Math.round(shape.rotation)}°`}
      </div>
    </div>
  );
}

/** Copy button with flash feedback. */
function CopyButton({
  propKey,
  text,
  label = "Copy",
}: {
  propKey: string;
  text: string;
  label?: string;
}): React.ReactElement {
  const { copiedProp, flashCopied } = useDevModeStore();
  const copied = copiedProp === propKey;

  const handleClick = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Fallback for non-secure contexts.
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    flashCopied(propKey);
  }, [propKey, text, flashCopied]);

  return (
    <button
      onClick={handleClick}
      title={`Copy: ${text}`}
      style={{
        background: copied ? "#a6e3a122" : "transparent",
        border: `1px solid ${copied ? "#a6e3a166" : "#313244"}`,
        borderRadius: 3,
        color: copied ? "#a6e3a1" : "#585b70",
        fontSize: 10,
        padding: "1px 6px",
        cursor: "pointer",
        transition: "all 0.15s",
        flexShrink: 0,
        whiteSpace: "nowrap",
      }}
    >
      {copied ? "✓ Copied" : label}
    </button>
  );
}

/** One row: property name + value + copy button. */
function PropRow({ group, idx, prop, value }: {
  group: string;
  idx: number;
  prop: string;
  value: string;
}): React.ReactElement {
  const propKey = `${group}.${idx}`;
  const isComment = prop.startsWith("/*");

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "3px 12px",
        gap: 6,
        borderRadius: 4,
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLDivElement).style.background = "#1e1e2e";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLDivElement).style.background = "transparent";
      }}
    >
      {/* property name */}
      <span
        style={{
          color: isComment ? "#585b70" : "#89b4fa",
          fontFamily: "monospace",
          fontSize: 11,
          flexShrink: 0,
        }}
      >
        {isComment ? prop : `${prop}:`}
      </span>

      {/* value */}
      <span
        style={{
          color: "#cba6f7",
          fontFamily: "monospace",
          fontSize: 11,
          flexGrow: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {value}
      </span>

      {/* copy button */}
      {!isComment && (
        <CopyButton
          propKey={propKey}
          text={`${prop}: ${value};`}
        />
      )}
    </div>
  );
}

/** One CSS group section. */
function GroupSection({ group }: { group: CssGroup }): React.ReactElement {
  return (
    <div style={{ marginBottom: 2 }}>
      {/* Section label */}
      <div
        style={{
          padding: "8px 12px 4px",
          color: "#6c7086",
          fontSize: 10,
          fontWeight: 700,
          letterSpacing: 0.8,
          textTransform: "uppercase",
        }}
      >
        {group.label}
      </div>

      {/* Property rows */}
      {group.props.map((p, i) => (
        <PropRow
          key={i}
          group={group.label}
          idx={i}
          prop={p.property}
          value={p.value}
        />
      ))}
    </div>
  );
}

/** Full CSS block preview textarea. */
function CssBlockPreview({ shape }: { shape: Shape }): React.ReactElement {
  const block = generateCssBlock(shape);
  const { copiedProp, flashCopied } = useDevModeStore();
  const copied = copiedProp === "__block__";

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(block);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = block;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      document.body.removeChild(ta);
    }
    flashCopied("__block__");
  }, [block, flashCopied]);

  return (
    <div
      style={{
        margin: "8px 12px",
        borderRadius: 6,
        border: "1px solid #313244",
        overflow: "hidden",
      }}
    >
      {/* Header */}
      <div
        style={{
          background: "#1e1e2e",
          borderBottom: "1px solid #313244",
          padding: "5px 10px",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <span style={{ color: "#6c7086", fontSize: 10, fontWeight: 700, letterSpacing: 0.8, textTransform: "uppercase" }}>
          CSS Block
        </span>
        <button
          onClick={handleCopy}
          style={{
            background: copied ? "#a6e3a122" : "#313244",
            border: `1px solid ${copied ? "#a6e3a166" : "#45475a"}`,
            borderRadius: 3,
            color: copied ? "#a6e3a1" : "#a6adc8",
            fontSize: 10,
            padding: "2px 8px",
            cursor: "pointer",
            transition: "all 0.15s",
          }}
        >
          {copied ? "✓ Copied!" : "Copy all"}
        </button>
      </div>

      {/* Code block */}
      <pre
        style={{
          margin: 0,
          padding: "8px 10px",
          background: "#11111b",
          color: "#cdd6f4",
          fontSize: 10,
          fontFamily: "monospace",
          overflowX: "auto",
          lineHeight: 1.6,
          maxHeight: 160,
          overflowY: "auto",
          whiteSpace: "pre",
        }}
      >
        {block}
      </pre>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main component
// ─────────────────────────────────────────────────────────────────────────────

export function DevModePanel(): React.ReactElement {
  const selectedIds = useSelectionStore((s) => s.selectedIds);
  const { shapes } = useDocumentStore();
  const inspectedShapeId = useDevModeStore((s) => s.inspectedShapeId);

  // Prefer the hovered shape, fall back to first selected.
  const targetId = inspectedShapeId ?? selectedIds[0] ?? null;
  const shape: Shape | undefined = targetId ? shapes[targetId] : undefined;

  const groups = shape ? generateCssGroups(shape) : [];

  return (
    <div style={panelStyle}>
      <DevModeBanner />

      {!shape ? (
        <div style={{ padding: 16, color: "#45475a", lineHeight: 1.6 }}>
          <div style={{ fontWeight: 600, color: "#6c7086", marginBottom: 6 }}>No selection</div>
          <div>Select or hover a shape on the canvas to inspect its CSS properties.</div>
        </div>
      ) : (
        <>
          <ShapeHeader shape={shape} />

          {/* Property groups */}
          <div style={{ flex: 1, paddingTop: 4, paddingBottom: 8 }}>
            {groups.map((g) => (
              <GroupSection key={g.label} group={g} />
            ))}
          </div>

          {/* Divider */}
          <div style={{ borderTop: "1px solid #313244" }} />

          {/* Full CSS block */}
          <CssBlockPreview shape={shape} />
        </>
      )}
    </div>
  );
}
