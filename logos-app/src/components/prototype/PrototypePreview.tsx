/**
 * components/prototype/PrototypePreview.tsx  (P4.8)
 *
 * Fullscreen prototype preview mode.
 * Renders the current frame and its children as HTML/CSS elements.
 * Handles click interactions and CSS transitions between frames.
 */

import React, { useEffect, useCallback, useRef } from "react";
import { useProtoStore } from "../../stores/prototypeStore";
import { useDocumentStore } from "../../stores/documentStore";
import type { Shape } from "../../types/shapes";
import type { PrototypeTransition } from "../../types/prototype";

// ─────────────────────────────────────────────────────────────────────────────
// Shape renderer (HTML/CSS, not WASM)
// ─────────────────────────────────────────────────────────────────────────────

interface ShapeViewProps {
  shape: Shape;
  allShapes: Record<string, Shape>;
  scale: number;
  offsetX: number;
  offsetY: number;
  onClick?: (shapeId: string) => void;
}

function fillColor(shape: Shape): string {
  const f = shape.fills?.[0];
  if (!f) return "transparent";
  return f.color;
}

function ShapeView({ shape, allShapes, scale, offsetX, offsetY, onClick }: ShapeViewProps): React.ReactElement | null {
  if (shape.hidden) return null;

  const left = (shape.bounds.x - offsetX) * scale;
  const top = (shape.bounds.y - offsetY) * scale;
  const width = shape.bounds.w * scale;
  const height = shape.bounds.h * scale;

  const baseStyle: React.CSSProperties = {
    position: "absolute",
    left, top, width, height,
    opacity: shape.opacity,
    transform: shape.rotation ? `rotate(${shape.rotation}deg)` : undefined,
    boxSizing: "border-box",
    overflow: "hidden",
  };

  const bg = fillColor(shape);

  if (shape.type === "text") {
    return (
      <div
        style={{
          ...baseStyle,
          background: "transparent",
          color: bg || "#cdd6f4",
          fontSize: Math.max(10, (shape.bounds.h * scale) * 0.6),
          display: "flex",
          alignItems: "center",
          userSelect: "none",
        }}
        onClick={onClick ? () => onClick(shape.id) : undefined}
      >
        {shape.name}
      </div>
    );
  }

  if (shape.type === "ellipse" || shape.type === "circle") {
    return (
      <div
        style={{ ...baseStyle, background: bg, borderRadius: "50%" }}
        onClick={onClick ? () => onClick(shape.id) : undefined}
      />
    );
  }

  // rect / frame / group / component / instance / other
  const isClickable = !!onClick;
  return (
    <div
      style={{
        ...baseStyle,
        background: bg,
        cursor: isClickable ? "pointer" : undefined,
      }}
      onClick={isClickable ? () => onClick(shape.id) : undefined}
    >
      {/* Render children */}
      {shape.children?.map((childId) => {
        const child = allShapes[childId];
        if (!child) return null;
        return (
          <ShapeView
            key={childId}
            shape={child}
            allShapes={allShapes}
            scale={1}
            offsetX={shape.bounds.x}
            offsetY={shape.bounds.y}
            onClick={onClick}
          />
        );
      })}
    </div>
  );
}

function transitionStyle(transition: PrototypeTransition, duration: number): React.CSSProperties {
  const dur = `${duration}ms`;
  switch (transition) {
    case "instant":
      return {};

    case "dissolve":
      return {
        opacity: 1,
        transition: `opacity ${dur} ease`,
      };

    case "slide-left":
      return {
        transform: "translateX(0)",
        transition: `transform ${dur} ease`,
      };

    case "slide-right":
      return {
        transform: "translateX(0)",
        transition: `transform ${dur} ease`,
      };

    case "push-left":
      return {
        transform: "translateX(0)",
        transition: `transform ${dur} ease`,
      };

    case "push-right":
      return {
        transform: "translateX(0)",
        transition: `transform ${dur} ease`,
      };

    default:
      return {};
  }
}

function getInitialStyle(transition: PrototypeTransition): React.CSSProperties {
  switch (transition) {
    case "dissolve": return { opacity: 0, transition: "none" };
    case "slide-left": return { transform: "translateX(100%)", transition: "none" };
    case "slide-right": return { transform: "translateX(-100%)", transition: "none" };
    case "push-left": return { transform: "translateX(100%)", transition: "none" };
    case "push-right": return { transform: "translateX(-100%)", transition: "none" };
    default: return {};
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame renderer (fits the frame into the preview area)
// ─────────────────────────────────────────────────────────────────────────────

interface FrameRendererProps {
  frameId: string;
  allShapes: Record<string, Shape>;
  canvasWidth: number;
  canvasHeight: number;
  transition: PrototypeTransition;
  duration: number;
  onShapeClick: (shapeId: string) => void;
}

function FrameRenderer({
  frameId, allShapes, canvasWidth, canvasHeight, transition, duration, onShapeClick,
}: FrameRendererProps): React.ReactElement | null {
  const frame = allShapes[frameId];
  if (!frame) return null;

  const ref = useRef<HTMLDivElement>(null);

  // Fit-to-canvas scaling
  const maxW = canvasWidth * 0.9;
  const maxH = canvasHeight * 0.9;
  const scale = Math.min(maxW / frame.bounds.w, maxH / frame.bounds.h, 1);
  const displayW = frame.bounds.w * scale;
  const displayH = frame.bounds.h * scale;

  // Apply enter animation on mount
  useEffect(() => {
    if (transition === "instant") return;
    const el = ref.current;
    if (!el) return;
    // Start at initial (off-screen) position
    Object.assign(el.style, getInitialStyle(transition));
    // Force reflow
    void el.offsetHeight;
    // Animate to final
    const final = transitionStyle(transition, duration);
    Object.assign(el.style, final);
  }, [transition, duration]);

  return (
    <div
      ref={ref}
      style={{
        width: displayW,
        height: displayH,
        position: "relative",
        overflow: "hidden",
        boxShadow: "0 8px 40px rgba(0,0,0,0.6)",
        borderRadius: 4,
        background: fillColor(frame) || "#1e1e2e",
      }}
    >
      {frame.children?.map((childId) => {
        const child = allShapes[childId];
        if (!child) return null;
        return (
          <ShapeView
            key={childId}
            shape={child}
            allShapes={allShapes}
            scale={scale}
            offsetX={frame.bounds.x}
            offsetY={frame.bounds.y}
            onClick={onShapeClick}
          />
        );
      })}

      {/* Frame label overlay */}
      <div style={{
        position: "absolute", bottom: 8, left: 0, right: 0,
        textAlign: "center", fontSize: 10, color: "rgba(205,214,244,0.4)",
        pointerEvents: "none", fontFamily: "monospace",
      }}>
        {frame.name}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Preview container
// ─────────────────────────────────────────────────────────────────────────────

export function PrototypePreview(): React.ReactElement | null {
  const {
    previewOpen, previewCurrentFrame, previewTransition,
    previewDuration, navigate, stopPreview,
  } = useProtoStore();
  const { interactions } = useProtoStore();
  const rawShapes = useDocumentStore((s) => s.shapes);

  const [canvasSize, setCanvasSize] = React.useState({ w: window.innerWidth, h: window.innerHeight });
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = () => setCanvasSize({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", handler);
    return () => window.removeEventListener("resize", handler);
  }, []);

  // Escape key closes preview
  useEffect(() => {
    if (!previewOpen) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") stopPreview(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [previewOpen, stopPreview]);

  const handleShapeClick = useCallback((shapeId: string) => {
    if (!previewCurrentFrame) return;
    // Check interactions on the current frame
    const ixs = interactions[previewCurrentFrame] ?? [];
    // Also check interactions on the clicked shape directly
    const shapeIxs = interactions[shapeId] ?? [];
    const allIxs = [...ixs, ...shapeIxs];

    const clickIx = allIxs.find((ix) => ix.trigger === "click");
    if (!clickIx) return;
    navigate(clickIx.target, clickIx.transition, clickIx.duration);
  }, [previewCurrentFrame, interactions, navigate]);

  if (!previewOpen) return null;

  return (
    <div
      ref={containerRef}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 2000,
        background: "rgba(17,17,27,0.96)",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 0,
      }}
    >
      {/* Top bar */}
      <div style={{
        position: "absolute",
        top: 0, left: 0, right: 0,
        height: 44,
        background: "#1e1e2e",
        borderBottom: "1px solid #313244",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 16px",
        zIndex: 10,
      }}>
        <span style={{ fontSize: 12, color: "#585b70", fontFamily: "monospace" }}>
          ▶ Prototype Preview
        </span>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <span style={{ fontSize: 11, color: "#585b70" }}>
            {previewCurrentFrame ? rawShapes[previewCurrentFrame]?.name ?? previewCurrentFrame : "—"}
          </span>
          <button
            onClick={stopPreview}
            style={{
              background: "#313244", color: "#cdd6f4", border: "none",
              borderRadius: 6, padding: "4px 10px", fontSize: 11, cursor: "pointer",
            }}
          >
            ✕ Close
          </button>
        </div>
      </div>

      {/* Frame canvas */}
      <div
        style={{
          marginTop: 44,
          flex: 1,
          width: "100%",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          overflow: "hidden",
        }}
        onClick={handleShapeClick.bind(null, previewCurrentFrame ?? "")}
      >
        {previewCurrentFrame && (
          <FrameRenderer
            key={previewCurrentFrame}
            frameId={previewCurrentFrame}
            allShapes={rawShapes}
            canvasWidth={canvasSize.w}
            canvasHeight={canvasSize.h - 44}
            transition={previewTransition}
            duration={previewDuration}
            onShapeClick={handleShapeClick}
          />
        )}

        {!previewCurrentFrame && (
          <div style={{ color: "#585b70", fontSize: 14, fontFamily: "monospace" }}>
            No starting frame selected.
          </div>
        )}
      </div>

      {/* Bottom bar with frame navigation hints */}
      <div style={{
        position: "absolute",
        bottom: 0, left: 0, right: 0,
        height: 36,
        background: "#1e1e2e",
        borderTop: "1px solid #313244",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 20,
        fontSize: 10,
        color: "#45475a",
        fontFamily: "monospace",
      }}>
        <span>Click an element to navigate</span>
        <span>·</span>
        <span>Esc to close</span>
      </div>
    </div>
  );
}
