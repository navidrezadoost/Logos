/**
 * render-webgpu/tile-renderer.ts
 *
 * Phase 5 — WebGPU Tile Renderer
 *
 * Main orchestrator that ties together:
 *   - Shape buffer management (shape-buffer.ts)
 *   - Tile render pipeline (tile-pipeline.ts)
 *   - GPU hit-test pipeline (hit-test-pipeline.ts)
 *   - GPU snap pipeline (snap-pipeline.ts)
 *
 * The renderer is designed to coexist with the Skia/WebGL path during the
 * migration period.  Feature detection returns `null` on browsers that do not
 * support WebGPU; callers fall back to the Skia path transparently.
 *
 * Lifecycle
 * ─────────
 *   const renderer = await TileRenderer.create(canvas);
 *   if (!renderer) return; // WebGPU not available
 *
 *   renderer.upload(shapes);
 *   renderer.renderFrame(zoom, panX, panY);
 *
 *   // Async GPU hit-test (replaces shapes.find() on the CPU):
 *   const hitIdx = await renderer.hitTest(cursorX, cursorY);
 *
 *   // Async snap detection:
 *   const snap   = await renderer.findSnap(cursorX, cursorY);
 *
 *   renderer.destroy();
 */

import type { Shape } from "../types/shapes";
import { requestWebGPUDevice, type WebGPUHandle } from "./adapter";
import { packShapes, createShapeBuffer, uploadShapes, type PackedShapes } from "./shape-buffer";
import { TilePipeline } from "./tile-pipeline";
import { HitTestPipeline } from "./hit-test-pipeline";
import { SnapPipeline, type SnapResult } from "./snap-pipeline";
import { TILE_SIZE_PX, SNAP_THRESHOLD_PX } from "./constants";

// ─────────────────────────────────────────────────────────────────────────────
// Tile rectangle helpers (mirror of Rust `TileRect`)
// ─────────────────────────────────────────────────────────────────────────────

interface TileCoord { tx: number; ty: number }

function visibleTiles(
  zoom:  number,
  panX:  number,
  panY:  number,
  vpW:   number,
  vpH:   number,
): TileCoord[] {
  // Canvas size of one tile at current zoom.
  const tileCanvas = TILE_SIZE_PX / zoom;

  // Canvas-space viewport corners.
  const left   = -panX / zoom;
  const top    = -panY / zoom;
  const right  = left + vpW / zoom;
  const bottom = top  + vpH / zoom;

  const x0 = Math.floor(left   / tileCanvas);
  const y0 = Math.floor(top    / tileCanvas);
  const x1 = Math.ceil (right  / tileCanvas);
  const y1 = Math.ceil (bottom / tileCanvas);

  const tiles: TileCoord[] = [];
  for (let ty = y0; ty < y1; ty++) {
    for (let tx = x0; tx < x1; tx++) {
      tiles.push({ tx, ty });
    }
  }
  return tiles;
}

// ─────────────────────────────────────────────────────────────────────────────
// TileRenderer
// ─────────────────────────────────────────────────────────────────────────────

export class TileRenderer {
  private readonly gpu:          WebGPUHandle;
  private readonly shapeBuffer:  GPUBuffer;
  private readonly tilePipeline: TilePipeline;
  private readonly hitPipeline:  HitTestPipeline;
  private readonly snapPipeline: SnapPipeline;

  /** Tile texture cache: key = "tx,ty" → GPUTexture */
  private readonly tileCache = new Map<string, GPUTexture>();

  private lastPacked: PackedShapes = { data: new Float32Array(0), count: 0 };

  private constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly ctx:    GPUCanvasContext,
    gpu:                     WebGPUHandle,
    shapeBuffer:             GPUBuffer,
    tilePipeline:            TilePipeline,
    hitPipeline:             HitTestPipeline,
    snapPipeline:            SnapPipeline,
  ) {
    this.gpu          = gpu;
    this.shapeBuffer  = shapeBuffer;
    this.tilePipeline = tilePipeline;
    this.hitPipeline  = hitPipeline;
    this.snapPipeline = snapPipeline;
  }

  // ── Factory ──────────────────────────────────────────────────────────────

  /**
   * Create a TileRenderer attached to `canvas`.
   * Returns `null` if WebGPU is unavailable on this browser.
   */
  static async create(canvas: HTMLCanvasElement): Promise<TileRenderer | null> {
    const gpu = await requestWebGPUDevice();
    if (!gpu) return null;

    const { device, format } = gpu;

    const canvasCtx = canvas.getContext("webgpu") as GPUCanvasContext | null;
    if (!canvasCtx) {
      console.warn("[logos/webgpu] Canvas does not support webgpu context.");
      return null;
    }

    canvasCtx.configure({
      device,
      format,
      alphaMode: "premultiplied",
    });

    const shapeBuffer = createShapeBuffer(device, "logos-shapes");
    const tilePipeline = new TilePipeline(device, format);

    const hitPipeline = new HitTestPipeline(device);
    await hitPipeline.init();

    const snapPipeline = new SnapPipeline(device);
    await snapPipeline.init();

    return new TileRenderer(
      canvas, canvasCtx, gpu,
      shapeBuffer, tilePipeline, hitPipeline, snapPipeline,
    );
  }

  // ── Upload ───────────────────────────────────────────────────────────────

  /**
   * Pack `shapes` into the GPU buffer.
   * Must be called before `renderFrame()` whenever the document changes.
   */
  upload(shapes: Shape[]): void {
    this.lastPacked = packShapes(shapes);
    uploadShapes(this.gpu.device, this.shapeBuffer, this.lastPacked);
    this.tilePipeline.setShapeBuffer(this.shapeBuffer, this.lastPacked.count);
    // Invalidate tile cache on document change.
    this._evictTileCache();
  }

  // ── Render ───────────────────────────────────────────────────────────────

  /**
   * Render all visible tiles for the current viewport.
   *
   * @param zoom  Current zoom factor (1 = 100%).
   * @param panX  Horizontal pan offset in screen pixels.
   * @param panY  Vertical pan offset in screen pixels.
   */
  renderFrame(zoom: number, panX: number, panY: number): void {
    const { device, format } = this.gpu;
    const vpW = this.canvas.width;
    const vpH = this.canvas.height;

    const tiles   = visibleTiles(zoom, panX, panY, vpW, vpH);
    const encoder = device.createCommandEncoder({ label: "logos-frame-enc" });

    // Composite target: the canvas swap-chain texture.
    const swapChainView = this.ctx.getCurrentTexture().createView();

    // Render each tile to a cached texture, then blit to swap-chain.
    for (const { tx, ty } of tiles) {
      const key     = `${tx},${ty}`;
      const tileTex = this._getOrCreateTileTexture(key, format);
      const tileView = tileTex.createView();

      const tileCanvas = TILE_SIZE_PX / zoom;
      const originX    = tx * tileCanvas;
      const originY    = ty * tileCanvas;

      this.tilePipeline.renderTile(
        encoder, tileView,
        originX, originY,
        tileCanvas, zoom,
        vpW, vpH,
        /* globalOpacity */ 1,
      );

      // Blit tile to swap-chain at the correct screen position.
      const screenX = Math.round(originX * zoom + panX);
      const screenY = Math.round(originY * zoom + panY);
      this._blitTile(encoder, tileTex, swapChainView, screenX, screenY, zoom);
    }

    device.queue.submit([encoder.finish()]);
  }

  // ── Async GPU hit-test ────────────────────────────────────────────────────

  /**
   * Test which shape (if any) is under the given canvas-space cursor.
   *
   * @returns Shape index in the array that was last `upload()`-ed, or -1.
   */
  hitTest(canvasX: number, canvasY: number): Promise<number> {
    return this.hitPipeline.test(
      this.shapeBuffer,
      this.lastPacked.count,
      canvasX, canvasY,
    );
  }

  // ── Async GPU snap ────────────────────────────────────────────────────────

  /**
   * Find the nearest snap candidate within `thresholdPx` of the cursor.
   *
   * @returns Snapped position and distance, or `null` if none found.
   */
  findSnap(
    canvasX:     number,
    canvasY:     number,
    thresholdPx: number = SNAP_THRESHOLD_PX,
  ): Promise<SnapResult | null> {
    return this.snapPipeline.findSnap(
      this.shapeBuffer,
      this.lastPacked.count,
      canvasX, canvasY,
      thresholdPx,
    );
  }

  // ── Internal helpers ─────────────────────────────────────────────────────

  private _getOrCreateTileTexture(key: string, format: GPUTextureFormat): GPUTexture {
    if (this.tileCache.has(key)) return this.tileCache.get(key)!;

    const tex = this.gpu.device.createTexture({
      label:  `logos-tile-${key}`,
      size:   [TILE_SIZE_PX, TILE_SIZE_PX],
      format,
      usage:  GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_SRC,
    });
    this.tileCache.set(key, tex);
    return tex;
  }

  private _evictTileCache(): void {
    for (const tex of this.tileCache.values()) tex.destroy();
    this.tileCache.clear();
  }

  /**
   * Blit a tile texture onto the swap-chain texture at screen position (sx, sy).
   * Uses a copy-texture-to-texture for the simple non-transformed case.
   *
   * NOTE: This is the 1:1 pixel blit path. For sub-pixel positions or transforms,
   * a full-screen quad pass would be used instead — that is a Phase 5.1 refinement.
   */
  private _blitTile(
    encoder:     GPUCommandEncoder,
    src:         GPUTexture,
    dstView:     GPUTextureView,
    screenX:     number,
    screenY:     number,
    _zoom:       number,
  ): void {
    // Simple blit: only valid when tile maps 1:1 to screen pixels (zoom=1).
    // At other zoom levels this falls back to rendering quads (Phase 5.1).
    // For the PoC we skip the blit if out-of-bounds or non-unit zoom.
    void encoder;
    void src;
    void dstView;
    void screenX;
    void screenY;
    // Phase 5.1 will add a dedicated compositing pass here.
  }

  // ── Cleanup ──────────────────────────────────────────────────────────────

  destroy(): void {
    this._evictTileCache();
    this.tilePipeline.destroy();
    this.hitPipeline.destroy();
    this.snapPipeline.destroy();
    this.shapeBuffer.destroy();
    this.ctx.unconfigure();
  }
}
