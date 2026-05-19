/**
 * render-webgpu/useWebGPURenderer.ts
 *
 * React hook that initialises and manages the WebGPU TileRenderer lifecycle.
 *
 * The hook is a feature-gated drop-in alongside the existing Skia/WebGL path:
 * - When WebGPU is unavailable, the hook returns `null` immediately and the
 *   calling component continues using the Skia renderer unchanged.
 * - When WebGPU is available, the renderer is lazily initialised on the first
 *   render and kept alive until the canvas unmounts.
 *
 * Usage (in Canvas.tsx):
 * ─────────────────────────────────────────────────────────────────────────────
 *   const gpuRenderer = useWebGPURenderer(canvasRef, shapes, zoom, panX, panY);
 *
 *   // Overlay indicator:
 *   if (gpuRenderer?.active) {
 *     <div className="webgpu-badge">GPU ⚡</div>
 *   }
 *
 *   // Replace shapes.find() with async GPU hit-test:
 *   const handleMouseMove = async (e) => {
 *     if (gpuRenderer?.active) {
 *       const idx = await gpuRenderer.hitTest(canvasX, canvasY);
 *       ...
 *     }
 *   };
 * ─────────────────────────────────────────────────────────────────────────────
 */

import { useEffect, useRef, useState, useCallback } from "react";
import type { Shape } from "../types/shapes";
import { TileRenderer } from "./tile-renderer";
import { isWebGPUSupported } from "./adapter";
import type { SnapResult } from "./snap-pipeline";

export interface WebGPURendererHandle {
  /** True once the renderer is fully initialised and rendering. */
  active: boolean;
  /** Async GPU-side hit test. Returns shape index or -1. */
  hitTest:  (canvasX: number, canvasY: number) => Promise<number>;
  /** Async GPU-side snap candidate. Returns snap point or null. */
  findSnap: (canvasX: number, canvasY: number, threshold?: number) => Promise<SnapResult | null>;
}

/**
 * Initialise and drive the WebGPU TileRenderer for a given canvas.
 *
 * @param canvasRef  React ref to the `<canvas>` element.
 * @param shapes     Current page shapes (flat array, bottom-to-top order).
 * @param zoom       Current zoom factor.
 * @param panX       Horizontal pan offset (screen px).
 * @param panY       Vertical pan offset (screen px).
 *
 * @returns A handle with `active`, `hitTest`, and `findSnap`, or `null` when
 *          WebGPU is not supported.
 */
export function useWebGPURenderer(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
  shapes:    Shape[],
  zoom:      number,
  panX:      number,
  panY:      number,
): WebGPURendererHandle | null {
  const rendererRef = useRef<TileRenderer | null>(null);
  const [active, setActive] = useState(false);

  // ── Initialise renderer once the canvas is mounted ────────────────────────
  useEffect(() => {
    if (!isWebGPUSupported()) return;

    const canvas = canvasRef.current;
    if (!canvas) return;

    let cancelled = false;

    TileRenderer.create(canvas).then((renderer) => {
      if (cancelled || !renderer) return;
      rendererRef.current = renderer;
      setActive(true);
      console.info("[logos/webgpu] TileRenderer active.");
    });

    return () => {
      cancelled = true;
      rendererRef.current?.destroy();
      rendererRef.current = null;
      setActive(false);
    };
    // Canvas ref identity is stable; only run once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canvasRef]);

  // ── Upload shapes whenever the document changes ───────────────────────────
  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) return;
    renderer.upload(shapes);
  }, [shapes]);

  // ── Re-render on every frame when zoom/pan/shapes change ─────────────────
  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) return;
    renderer.renderFrame(zoom, panX, panY);
  }, [shapes, zoom, panX, panY]);

  // ── Stable callbacks ──────────────────────────────────────────────────────
  const hitTest = useCallback(
    (canvasX: number, canvasY: number) =>
      rendererRef.current?.hitTest(canvasX, canvasY) ?? Promise.resolve(-1),
    []
  );

  const findSnap = useCallback(
    (canvasX: number, canvasY: number, threshold?: number) =>
      rendererRef.current?.findSnap(canvasX, canvasY, threshold) ?? Promise.resolve(null),
    []
  );

  // Return null early when WebGPU is unavailable (avoid object allocation).
  if (!isWebGPUSupported()) return null;

  return { active, hitTest, findSnap };
}
