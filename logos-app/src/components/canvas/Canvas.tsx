/**
 * Canvas.tsx
 *
 * Phase M1 spike component.
 *
 * Renders one hardcoded rectangle using either:
 *   A) The Rust/Skia render-wasm engine (WebGL2, via Emscripten Module), or
 *   B) Canvas 2D API (fallback when render-wasm has not been built yet).
 *
 * The presence of the rectangle in either path validates the architecture.
 * Path A requires `frontend/resources/public/js/render-wasm.{js,wasm}` to
 * exist (built via `cd render-wasm && ./build`).
 *
 * Initialization sequence (Path A):
 *   loadRenderWasm(jsUrl, wasmUrl)
 *     └─ dynamic import render-wasm.js (Emscripten ES6 module)
 *     └─ createRustSkiaModule({locateFile}) → Module
 *   initCanvasContext(mod, canvas)
 *     └─ getContext("webgl2") → ctx
 *     └─ mod.GL.registerContext + makeContextCurrent
 *     └─ mod._init(width, height)
 *     └─ mod._set_render_options(0, dpr)
 *   drawHardcodedRect(mod)
 *     └─ mod._init_shapes_pool(1)
 *     └─ mod._use_shape(0,0,0,1)   ← UUID
 *     └─ mod._set_shape_type(3)    ← rect
 *     └─ mod._set_shape_selrect(50,50,250,150)
 *     └─ applySolidFill(mod, 0xFF0000FF)  ← blue ARGB
 *     └─ mod._render_sync()
 */

import { useEffect, useRef, useState } from "react";
import {
  loadRenderWasm,
  initCanvasContext,
  cleanUp,
  drawHardcodedRect,
  type RenderWasmModule,
} from "../../render-wasm/module";

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const CANVAS_W = 800;
const CANVAS_H = 600;

const WASM_JS_URL: string =
  typeof __RENDER_WASM_JS__ !== "undefined"
    ? __RENDER_WASM_JS__
    : "/js/render-wasm.js";

const WASM_WASM_URL: string =
  typeof __RENDER_WASM_WASM__ !== "undefined"
    ? __RENDER_WASM_WASM__
    : "/js/render-wasm.wasm";

// ─────────────────────────────────────────────────────────────────────────────
// Canvas 2D fallback  (visible when render-wasm is not yet built)
// ─────────────────────────────────────────────────────────────────────────────

function drawFallback(canvas: HTMLCanvasElement): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;

  // Clear to dark background matching the app theme
  ctx.fillStyle = "#1e1e2e";
  ctx.fillRect(0, 0, CANVAS_W, CANVAS_H);

  // The hardcoded rectangle — same geometry as the WASM path
  ctx.fillStyle = "#0000ff";
  ctx.fillRect(50, 50, 200, 100); // x, y, w, h

  // Label (present only in fallback mode)
  ctx.fillStyle = "rgba(255,255,255,0.5)";
  ctx.font = "13px monospace";
  ctx.fillText("Canvas 2D fallback — build render-wasm to enable Skia renderer", 50, 200);
}

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

type RenderMode = "loading" | "wasm" | "fallback";

export function Canvas(): React.ReactElement {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const moduleRef = useRef<RenderWasmModule | null>(null);
  const [mode, setMode] = useState<RenderMode>("loading");

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let cancelled = false;

    (async () => {
      // 1. Try to load the Emscripten module
      const mod = await loadRenderWasm(WASM_JS_URL, WASM_WASM_URL);

      if (cancelled) return;

      if (mod !== null) {
        // Path A: Rust/Skia renderer
        const ok = initCanvasContext(mod, canvas);
        if (ok) {
          moduleRef.current = mod;
          drawHardcodedRect(mod);
          setMode("wasm");
          return;
        }
        // WebGL2 unavailable on this GPU — fall through to Canvas 2D
        console.warn("[logos-app] WebGL2 init failed, using Canvas 2D fallback.");
      }

      // Path B: Canvas 2D fallback
      drawFallback(canvas);
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
  }, []);

  return (
    <div style={{ position: "relative", display: "inline-block" }}>
      <canvas
        ref={canvasRef}
        width={CANVAS_W}
        height={CANVAS_H}
        style={{
          display: "block",
          border: "1px solid #45475a",
          borderRadius: "4px",
        }}
      />

      {/* Status badge */}
      <span
        style={{
          position: "absolute",
          top: 8,
          right: 8,
          padding: "2px 8px",
          borderRadius: "4px",
          fontSize: "11px",
          fontFamily: "monospace",
          background:
            mode === "wasm"
              ? "#a6e3a1"
              : mode === "fallback"
                ? "#f38ba8"
                : "#cdd6f4",
          color: "#1e1e2e",
        }}
      >
        {mode === "loading" && "⏳ loading…"}
        {mode === "wasm"     && "✓ render-wasm / Skia"}
        {mode === "fallback" && "⚠ Canvas 2D fallback"}
      </span>
    </div>
  );
}
