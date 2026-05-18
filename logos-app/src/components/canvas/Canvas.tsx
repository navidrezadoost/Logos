/**
 * components/canvas/Canvas.tsx  (M3 revision)
 *
 * Subscribes to documentStore and re-renders the Rust/Skia scene whenever
 * the shape list changes. Handles the WASM lifecycle (load → init → sync)
 * and a Canvas 2D fallback for environments without the Emscripten build.
 *
 * M3 additions:
 *  - Mouse-drag shape creation (tools: rect, ellipse)
 *  - Preview overlay div during drag
 *  - Worker pool initialization on mount
 */

import { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
import {
  loadRenderWasm,
  initCanvasContext,
  cleanUp,
  type RenderWasmModule,
} from "../../render-wasm/module";
import { syncScene, syncScene2D } from "../../render-wasm/scene";
import { useCurrentPageShapes, useDocumentStore } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { useUiStore } from "../../stores/uiStore";
import { workerPool } from "../../worker";
import { createRect } from "../../types/shapes";

const WASM_JS_URL: string =
  typeof __RENDER_WASM_JS__ !== "undefined" ? __RENDER_WASM_JS__ : "/js/render-wasm.js";
const WASM_WASM_URL: string =
  typeof __RENDER_WASM_WASM__ !== "undefined" ? __RENDER_WASM_WASM__ : "/js/render-wasm.wasm";

type RenderMode = "loading" | "wasm" | "fallback";

interface DragState {
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
}

export function Canvas(): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const moduleRef = useRef<RenderWasmModule | null>(null);
  const modeRef = useRef<RenderMode>("loading");

  const [mode, setMode] = useState<RenderMode>("loading");
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [drag, setDrag] = useState<DragState | null>(null);

  const shapes = useCurrentPageShapes();
  const clearSelection = useSelectionStore((s) => s.clearSelection);
  const { select } = useSelectionStore();
  const { zoom, panX, panY, activeTool, setTool } = useUiStore();
  const { addRect, addShape } = useDocumentStore();

  // ── Resize observer ────────────────────────────────────────────────────────
  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setSize({ w: Math.floor(width), h: Math.floor(height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── WASM boot + worker pool init (once) ───────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;

    // Initialize background workers (non-blocking)
    workerPool.init();

    (async () => {
      const mod = await loadRenderWasm(WASM_JS_URL, WASM_WASM_URL);
      if (cancelled) return;
      if (mod !== null) {
        const ok = initCanvasContext(mod, canvas);
        if (ok) {
          moduleRef.current = mod;
          modeRef.current = "wasm";
          setMode("wasm");
          return;
        }
      }
      modeRef.current = "fallback";
      setMode("fallback");
    })();

    return () => {
      cancelled = true;
      const mod = moduleRef.current;
      if (mod && canvasRef.current) {
        cleanUp(mod, canvasRef.current);
        moduleRef.current = null;
      }
      workerPool.terminate();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Re-render on store changes ─────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || modeRef.current === "loading") return;

    if (modeRef.current === "wasm" && moduleRef.current) {
      syncScene(moduleRef.current, shapes, size.w, size.h);
    } else {
      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.save();
        ctx.translate(panX, panY);
        ctx.scale(zoom, zoom);
        syncScene2D(ctx, shapes, size.w / zoom, size.h / zoom);
        ctx.restore();
      }
    }
  }, [shapes, size, zoom, panX, panY, mode]);

  // ── Mouse handlers for draw tools ─────────────────────────────────────────

  /** Convert mouse event coords to canvas-local coordinates (applying pan/zoom). */
  function toCanvas(e: React.MouseEvent): { x: number; y: number } {
    const rect = containerRef.current!.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left - panX) / zoom,
      y: (e.clientY - rect.top  - panY) / zoom,
    };
  }

  const isDrawTool = activeTool === "rect" || activeTool === "ellipse";

  const handleMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!isDrawTool) return;
      const { x, y } = toCanvas(e);
      setDrag({ startX: x, startY: y, currentX: x, currentY: y });
      e.preventDefault();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [isDrawTool, panX, panY, zoom]
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!drag) return;
      const { x, y } = toCanvas(e);
      setDrag((d) => d ? { ...d, currentX: x, currentY: y } : null);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [drag, panX, panY, zoom]
  );

  const handleMouseUp = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!drag) {
        // No drag — if select tool, clear selection on empty canvas click
        if (activeTool === "select" && e.target === containerRef.current) {
          clearSelection();
        }
        return;
      }

      const minX = Math.min(drag.startX, drag.currentX);
      const minY = Math.min(drag.startY, drag.currentY);
      const w = Math.abs(drag.currentX - drag.startX);
      const h = Math.abs(drag.currentY - drag.startY);

      setDrag(null);

      // Ignore tiny accidental drags
      if (w < 4 || h < 4) return;

      const bounds = { x: minX, y: minY, w, h };

      if (activeTool === "rect") {
        const id = addRect(bounds);
        select(id);
      } else if (activeTool === "ellipse") {
        const id = crypto.randomUUID();
        const count = shapes.filter((s) => s.type === "ellipse").length + 1;
        const shape = createRect(id, `Ellipse ${count}`, bounds, "#0000ff");
        addShape({ ...shape, type: "ellipse" });
        select(id);
      }

      // Return to select tool after drawing
      setTool("select");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [drag, activeTool, addRect, addShape, select, setTool, shapes, clearSelection]
  );

  const cursor =
    activeTool === "hand" ? "grab"
    : isDrawTool ? "crosshair"
    : activeTool === "text" ? "text"
    : "default";

  // Preview rect during drag (in pan/zoom space)
  const previewStyle: React.CSSProperties | null = drag
    ? (() => {
        const x = Math.min(drag.startX, drag.currentX) * zoom + panX;
        const y = Math.min(drag.startY, drag.currentY) * zoom + panY;
        const w = Math.abs(drag.currentX - drag.startX) * zoom;
        const h = Math.abs(drag.currentY - drag.startY) * zoom;
        return {
          position: "absolute" as const,
          left: x, top: y, width: w, height: h,
          border: "1px solid #89b4fa",
          background: "rgba(137,180,250,0.12)",
          pointerEvents: "none" as const,
          boxSizing: "border-box" as const,
        };
      })()
    : null;

  return (
    <div
      ref={containerRef}
      style={{ flex: 1, position: "relative", overflow: "hidden", background: "#313244", cursor }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block", width: "100%", height: "100%", pointerEvents: "none" }}
      />

      {/* Drag preview overlay */}
      {previewStyle && <div style={previewStyle} />}

      {shapes.length === 0 && mode !== "loading" && (
        <div style={{
          position: "absolute", inset: 0, display: "flex",
          alignItems: "center", justifyContent: "center",
          pointerEvents: "none", color: "#585b70",
          fontSize: "13px", fontFamily: "monospace",
        }}>
          Press R to add a rectangle
        </div>
      )}

      <span style={{
        position: "absolute", bottom: 8, right: 8, padding: "2px 8px",
        borderRadius: "4px", fontSize: "11px", fontFamily: "monospace",
        background: mode === "wasm" ? "#a6e3a1" : mode === "fallback" ? "#f38ba8" : "#cdd6f4",
        color: "#1e1e2e",
      }}>
        {mode === "loading" && "⏳ loading…"}
        {mode === "wasm"     && "✓ render-wasm / Skia"}
        {mode === "fallback" && "⚠ Canvas 2D fallback"}
      </span>
    </div>
  );
}
