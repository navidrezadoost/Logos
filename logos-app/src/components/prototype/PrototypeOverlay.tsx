/**
 * components/prototype/PrototypeOverlay.tsx  (P4.8)
 *
 * SVG overlay rendered on top of the canvas in prototype tool mode.
 * - Shows arrows between connected frames.
 * - Renders the in-progress drag arrow while the user draws a new connection.
 * - Shows a config popover for the selected connection.
 */

import React from "react";
import { useProtoStore } from "../../stores/prototypeStore";
import type { Shape } from "../../types/shapes";
import type { PrototypeTransition, PrototypeTrigger } from "../../types/prototype";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

function toScreen(cx: number, cy: number, zoom: number, panX: number, panY: number) {
  return { sx: cx * zoom + panX, sy: cy * zoom + panY };
}

function shapeCenterScreen(s: Shape, zoom: number, panX: number, panY: number) {
  const cx = s.bounds.x + s.bounds.w / 2;
  const cy = s.bounds.y + s.bounds.h / 2;
  return toScreen(cx, cy, zoom, panX, panY);
}

/** Shorten a line so the arrowhead stops at the target frame edge. */
function shortenEnd(
  x1: number, y1: number, x2: number, y2: number,
  target: Shape, zoom: number, panX: number, panY: number
): { ex: number; ey: number } {
  const tw = (target.bounds.w / 2) * zoom;
  const th = (target.bounds.h / 2) * zoom;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.hypot(dx, dy);
  if (len === 0) return { ex: x2, ey: y2 };
  // Stop at the intersection with the target bounding box
  const margin = Math.min(Math.abs(dx / len) * tw, Math.abs(dy / len) * th) + 8;
  const t = Math.max(0, (len - margin) / len);
  return { ex: x1 + dx * t, ey: y1 + dy * t };
}

/** Build SVG arrowhead polygon points at (ex, ey) facing direction (dx, dy). */
function arrowheadPoints(ex: number, ey: number, dx: number, dy: number): string {
  const len = Math.hypot(dx, dy);
  if (len === 0) return "";
  const ux = dx / len;
  const uy = dy / len;
  const size = 9;
  const hw = 5;
  const p1 = `${ex},${ey}`;
  const p2 = `${ex - ux * size - uy * hw},${ey - uy * size + ux * hw}`;
  const p3 = `${ex - ux * size + uy * hw},${ey - uy * size - ux * hw}`;
  return `${p1} ${p2} ${p3}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Config popover for selected connection
// ─────────────────────────────────────────────────────────────────────────────

const TRIGGERS: PrototypeTrigger[] = ["click", "hover", "delay"];
const TRANSITIONS: PrototypeTransition[] = [
  "instant", "dissolve", "slide-left", "slide-right", "push-left", "push-right",
];

interface ConfigPanelProps {
  sourceId: string;
  index: number;
  midX: number;
  midY: number;
}

function ConfigPanel({ sourceId, index, midX, midY }: ConfigPanelProps) {
  const { interactions, updateInteraction, removeInteraction, clearConnectionSelection } =
    useProtoStore();
  const interaction = (interactions[sourceId] ?? [])[index];
  if (!interaction) return null;

  return (
    <div
      style={{
        position: "absolute",
        left: midX + 8,
        top: midY - 80,
        zIndex: 600,
        background: "#1e1e2e",
        border: "1px solid #45475a",
        borderRadius: 8,
        padding: "10px 12px",
        minWidth: 200,
        boxShadow: "0 4px 16px rgba(0,0,0,0.5)",
        color: "#cdd6f4",
        fontSize: 12,
        fontFamily: "'Inter', system-ui, sans-serif",
      }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {/* Header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 8 }}>
        <span style={{ fontWeight: 600, fontSize: 11, color: "#89b4fa" }}>Interaction</span>
        <button
          onClick={() => { removeInteraction(sourceId, index); clearConnectionSelection(); }}
          style={{ background: "none", border: "none", color: "#f38ba8", cursor: "pointer", fontSize: 14, lineHeight: 1, padding: 0 }}
          title="Remove interaction"
        >✕</button>
      </div>

      {/* Trigger */}
      <div style={{ marginBottom: 6 }}>
        <label style={{ color: "#585b70", fontSize: 10, display: "block", marginBottom: 2 }}>TRIGGER</label>
        <select
          value={interaction.trigger}
          onChange={(e) => updateInteraction(sourceId, index, { trigger: e.target.value as PrototypeTrigger })}
          style={{
            width: "100%", background: "#313244", color: "#cdd6f4",
            border: "1px solid #45475a", borderRadius: 4, padding: "3px 6px", fontSize: 11,
          }}
        >
          {TRIGGERS.map((t) => <option key={t} value={t}>{t}</option>)}
        </select>
      </div>

      {/* Delay (only when trigger === delay) */}
      {interaction.trigger === "delay" && (
        <div style={{ marginBottom: 6 }}>
          <label style={{ color: "#585b70", fontSize: 10, display: "block", marginBottom: 2 }}>DELAY (MS)</label>
          <input
            type="number"
            value={interaction.delay ?? 1000}
            min={0}
            step={100}
            onChange={(e) => updateInteraction(sourceId, index, { delay: Number(e.target.value) })}
            style={{
              width: "100%", background: "#313244", color: "#cdd6f4",
              border: "1px solid #45475a", borderRadius: 4, padding: "3px 6px", fontSize: 11, boxSizing: "border-box",
            }}
          />
        </div>
      )}

      {/* Transition */}
      <div style={{ marginBottom: 6 }}>
        <label style={{ color: "#585b70", fontSize: 10, display: "block", marginBottom: 2 }}>TRANSITION</label>
        <select
          value={interaction.transition}
          onChange={(e) => updateInteraction(sourceId, index, { transition: e.target.value as PrototypeTransition })}
          style={{
            width: "100%", background: "#313244", color: "#cdd6f4",
            border: "1px solid #45475a", borderRadius: 4, padding: "3px 6px", fontSize: 11,
          }}
        >
          {TRANSITIONS.map((t) => <option key={t} value={t}>{t.replace(/-/g, " ")}</option>)}
        </select>
      </div>

      {/* Duration (hidden for instant) */}
      {interaction.transition !== "instant" && (
        <div>
          <label style={{ color: "#585b70", fontSize: 10, display: "block", marginBottom: 2 }}>DURATION (MS)</label>
          <input
            type="number"
            value={interaction.duration}
            min={50}
            max={3000}
            step={50}
            onChange={(e) => updateInteraction(sourceId, index, { duration: Number(e.target.value) })}
            style={{
              width: "100%", background: "#313244", color: "#cdd6f4",
              border: "1px solid #45475a", borderRadius: 4, padding: "3px 6px", fontSize: 11, boxSizing: "border-box",
            }}
          />
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main overlay
// ─────────────────────────────────────────────────────────────────────────────

export interface PrototypeOverlayProps {
  shapes: Shape[];
  zoom: number;
  panX: number;
  panY: number;
  /** Screen-space cursor position during in-progress arrow drag. */
  arrowDragEnd: { x: number; y: number } | null;
}

export function PrototypeOverlay({
  shapes, zoom, panX, panY, arrowDragEnd,
}: PrototypeOverlayProps): React.ReactElement {
  const { interactions, pendingSource, selectedConnection, selectConnection, clearConnectionSelection } =
    useProtoStore();

  const shapeMap = React.useMemo(
    () => Object.fromEntries(shapes.map((s) => [s.id, s])),
    [shapes]
  );

  // Config panel state
  const [configPanel, setConfigPanel] = React.useState<{
    sourceId: string; index: number; midX: number; midY: number;
  } | null>(null);

  React.useEffect(() => {
    if (!selectedConnection) { setConfigPanel(null); return; }
    const { sourceId, index } = selectedConnection;
    const src = shapeMap[sourceId];
    const tgt = shapeMap[(interactions[sourceId] ?? [])[index]?.target];
    if (!src || !tgt) { setConfigPanel(null); return; }
    const { sx: x1, sy: y1 } = shapeCenterScreen(src, zoom, panX, panY);
    const { sx: x2, sy: y2 } = shapeCenterScreen(tgt, zoom, panX, panY);
    setConfigPanel({ sourceId, index, midX: (x1 + x2) / 2, midY: (y1 + y2) / 2 });
  }, [selectedConnection, shapeMap, interactions, zoom, panX, panY]);

  return (
    <>
      {/* SVG arrow layer */}
      <svg
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
          pointerEvents: "none",
          overflow: "visible",
        }}
      >
        <defs>
          <marker id="proto-arrow" markerWidth="10" markerHeight="10" refX="5" refY="5" orient="auto">
            <path d="M0,0 L10,5 L0,10 Z" fill="#cba6f7" />
          </marker>
          <marker id="proto-arrow-selected" markerWidth="10" markerHeight="10" refX="5" refY="5" orient="auto">
            <path d="M0,0 L10,5 L0,10 Z" fill="#f5c2e7" />
          </marker>
        </defs>

        {/* Committed arrows */}
        {Object.entries(interactions).map(([sourceId, ixs]) =>
          ixs.map((ix, idx) => {
            const src = shapeMap[sourceId];
            const tgt = shapeMap[ix.target];
            if (!src || !tgt) return null;

            const { sx: x1, sy: y1 } = shapeCenterScreen(src, zoom, panX, panY);
            const { sx: x2, sy: y2 } = shapeCenterScreen(tgt, zoom, panX, panY);
            const { ex, ey } = shortenEnd(x1, y1, x2, y2, tgt, zoom, panX, panY);

            const isSelected =
              selectedConnection?.sourceId === sourceId && selectedConnection?.index === idx;
            const color = isSelected ? "#f5c2e7" : "#cba6f7";
            const points = arrowheadPoints(ex, ey, x2 - x1, y2 - y1);

            return (
              <g
                key={`${sourceId}-${idx}`}
                style={{ pointerEvents: "all", cursor: "pointer" }}
                onClick={(e) => {
                  e.stopPropagation();
                  if (isSelected) clearConnectionSelection();
                  else selectConnection(sourceId, idx);
                }}
              >
                {/* Invisible wider hit area */}
                <line x1={x1} y1={y1} x2={ex} y2={ey} stroke="transparent" strokeWidth={12} />
                {/* Visible line */}
                <line
                  x1={x1} y1={y1} x2={ex} y2={ey}
                  stroke={color} strokeWidth={isSelected ? 2 : 1.5}
                  strokeDasharray={ix.transition === "instant" ? undefined : "5 3"}
                />
                {/* Arrowhead */}
                {points && <polygon points={points} fill={color} />}
                {/* Trigger label at midpoint */}
                <text
                  x={(x1 + ex) / 2}
                  y={(y1 + ey) / 2 - 6}
                  fill={color}
                  fontSize={9}
                  fontFamily="'Inter', system-ui, sans-serif"
                  textAnchor="middle"
                >
                  {ix.trigger}
                </text>
              </g>
            );
          })
        )}

        {/* In-progress drag arrow */}
        {pendingSource && arrowDragEnd && (() => {
          const src = shapeMap[pendingSource];
          if (!src) return null;
          const { sx: x1, sy: y1 } = shapeCenterScreen(src, zoom, panX, panY);
          const { x: x2, y: y2 } = arrowDragEnd;
          const points = arrowheadPoints(x2, y2, x2 - x1, y2 - y1);
          return (
            <g>
              <line
                x1={x1} y1={y1} x2={x2} y2={y2}
                stroke="#89b4fa" strokeWidth={1.5}
                strokeDasharray="5 3"
              />
              {points && <polygon points={points} fill="#89b4fa" />}
            </g>
          );
        })()}

        {/* Highlight top-level frames in prototype mode */}
        {shapes
          .filter((s) => s.parentId === null)
          .map((s) => {
            const sx = s.bounds.x * zoom + panX;
            const sy = s.bounds.y * zoom + panY;
            const sw = s.bounds.w * zoom;
            const sh = s.bounds.h * zoom;
            const isSource = pendingSource === s.id;
            return (
              <rect
                key={s.id}
                x={sx} y={sy} width={sw} height={sh}
                fill="none"
                stroke={isSource ? "#89b4fa" : "#cba6f7"}
                strokeWidth={isSource ? 2 : 1}
                strokeOpacity={0.4}
                strokeDasharray={isSource ? undefined : "3 3"}
                rx={2}
                style={{ pointerEvents: "none" }}
              />
            );
          })}
      </svg>

      {/* Config panel (HTML, not SVG, for proper form inputs) */}
      {configPanel && (
        <ConfigPanel
          sourceId={configPanel.sourceId}
          index={configPanel.index}
          midX={configPanel.midX}
          midY={configPanel.midY}
        />
      )}
    </>
  );
}
