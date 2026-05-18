/**
 * components/canvas/Canvas.tsx  (M2 revision)
 *
 * Subscribes to documentStore and re-renders the Rust/Skia scene whenever
 * the shape list changes. Handles the WASM lifecycle (load → init → sync)
 * and a Canvas 2D fallback for environments without the Emscripten build.
 */

import { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
import {
  loadRenderWasm,
  initCanvasContext,
  cleanUp,
  type RenderWasmModule,
} from "../../render-wasm/module";
import { syncScene, syncScene2D } from "../../render-wasm/scene";
import { useCurrentPageShapes } from "../../stores/documentStore";
import { useSelectionStore } from "../../stores/selectionStore";
import { useUiStore } from "../../stores/uiStore";

const WASM_JS_URL: string =
  typeof __RENDER_WASM_JS__ !== "undefined" ? __RENDER_WASM_JS__ : "/js/render-wasm.js";
const WASM_WASM_URL: string =
  typeof __RENDER_WASM_WASM__ !== "undefined" ? __RENDER_WASM_WASM__ : "/js/render-wasm.wasm";

type RenderMode = "loading" | "wasm" | "fallback";

export function Canvas(): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const moduleRef = useRef<RenderWasmModule | null>(null);
  const modeRef = useRef<RenderMode>("loading");

  const [mode, setMode] = useState<RenderMode>("loading");
  const [size, setSize] = useState({ w: 800, h: 600 });

  const shapes = useCurrentPageShapes();
  const clearSelection = useSelectionStore((s) => s.clearSelection);
  const { zoom, panX, panY, activeTool } = useUiStore();

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

  // ── WASM boot (once) ───────────────────────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;

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

  const handleCanvasClick = useCallback(
    (e: React.MouseEvent<HTMLCanvasElement>) => {
      if (e.target === canvasRef.current) clearSelection();
    },
    [clearSelection]
  );

  const cursor =
    activeTool === "hand" ? "grab"
    : activeTool === "rect" || activeTool === "ellipse" || activeTool === "text" ? "crosshair"
    : "default";

  return (
    <div
      ref={containerRef}
      style={{ flex: 1, position: "relative", overflow: "hidden", background: "#313244" }}
    >
      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        onClick={handleCanvasClick}
        style={{ display: "block", cursor, width: "100%", height: "100%" }}
      />

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
