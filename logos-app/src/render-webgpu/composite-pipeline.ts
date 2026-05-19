/**
 * render-webgpu/composite-pipeline.ts
 *
 * Phase 5.1 — Compositing Pipeline
 *
 * Renders a pre-rasterised tile texture onto the swap-chain at the correct
 * screen position for the current zoom/pan.  This replaces the stub
 * `_blitTile` method in `tile-renderer.ts` with a real GPU render pass.
 *
 * Algorithm
 * ─────────
 *   For each visible tile:
 *     screen_x = tile_origin_canvas_x × zoom + panX
 *     screen_y = tile_origin_canvas_y × zoom + panY
 *     screen_w = tile_size_canvas     × zoom
 *     screen_h = tile_size_canvas     × zoom
 *
 *   A fullscreen quad is drawn with its corners at (screen_x, screen_y) →
 *   (screen_x+screen_w, screen_y+screen_h), sampling the tile texture.
 *
 * Usage
 * ─────
 *   const cp = new CompositePipeline(device, format);
 *
 *   // Once per frame, for each visible tile:
 *   cp.blitTile(
 *     encoder, swapChainView, tileTexture,
 *     screenX, screenY, screenW, screenH,
 *     viewportW, viewportH
 *   );
 */

import compositeSource from "./shaders/composite.wgsl?raw";

// Uniform layout (must match composite.wgsl `CompositeUniforms`):
//   [screen_x, screen_y, screen_w, screen_h,
//    viewport_w, viewport_h, opacity, _pad]   → 8 × f32 = 32 bytes
const UNIFORM_BYTES = 32;

export class CompositePipeline {
  private pipeline!:   GPURenderPipeline;
  private sampler!:    GPUSampler;
  private bgl!:        GPUBindGroupLayout;

  // Pool of uniform buffers — one per in-flight blit.
  private uniformPool: GPUBuffer[] = [];
  private poolCursor  = 0;

  constructor(
    private readonly device: GPUDevice,
    private readonly format: GPUTextureFormat,
  ) {
    this._build();
  }

  // ── Build ─────────────────────────────────────────────────────────────────

  private _build(): void {
    const { device, format } = this;

    const module = device.createShaderModule({
      label: "logos-composite-shader",
      code:  compositeSource,
    });

    this.bgl = device.createBindGroupLayout({
      label: "logos-composite-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "2d" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT,
          sampler: { type: "filtering" } },
      ],
    });

    this.pipeline = device.createRenderPipeline({
      label:  "logos-composite-pipeline",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.bgl] }),
      vertex:   { module, entryPoint: "vs_main" },
      fragment: {
        module,
        entryPoint: "fs_main",
        targets: [{
          format,
          blend: {
            color: {
              srcFactor:  "one",          // Source is already premultiplied.
              dstFactor:  "one-minus-src-alpha",
              operation:  "add",
            },
            alpha: {
              srcFactor:  "one",
              dstFactor:  "one-minus-src-alpha",
              operation:  "add",
            },
          },
        }],
      },
      primitive: { topology: "triangle-list" },
    });

    // Bilinear sampler for sub-pixel zoom levels.
    this.sampler = device.createSampler({
      label:          "logos-composite-sampler",
      magFilter:      "linear",
      minFilter:      "linear",
      mipmapFilter:   "linear",
      addressModeU:   "clamp-to-edge",
      addressModeV:   "clamp-to-edge",
    });
  }

  // ── Blit ──────────────────────────────────────────────────────────────────

  /**
   * Encode a compositing render pass that draws `tileTexture` onto the
   * swap-chain at `(screenX, screenY)` with size `(screenW, screenH)`.
   *
   * @param encoder     Active command encoder.
   * @param targetView  Swap-chain texture view.
   * @param tileTexture The rasterised tile texture.
   * @param screenX     Tile left edge in device pixels.
   * @param screenY     Tile top edge in device pixels.
   * @param screenW     Tile width in device pixels.
   * @param screenH     Tile height in device pixels.
   * @param viewportW   Canvas device-pixel width.
   * @param viewportH   Canvas device-pixel height.
   * @param opacity     Per-tile fade [0, 1] (default 1).
   * @param loadOp      "clear" to clear before blitting (first tile),
   *                    "load" to composite over existing content.
   */
  blitTile(
    encoder:     GPUCommandEncoder,
    targetView:  GPUTextureView,
    tileTexture: GPUTexture,
    screenX:     number,
    screenY:     number,
    screenW:     number,
    screenH:     number,
    viewportW:   number,
    viewportH:   number,
    opacity:     number = 1,
    loadOp:      "clear" | "load" = "load",
  ): void {
    const { device } = this;

    // Get or create a uniform buffer from the pool.
    const ubuf = this._getUniformBuffer();
    device.queue.writeBuffer(
      ubuf, 0,
      new Float32Array([
        screenX, screenY, screenW, screenH,
        viewportW, viewportH, opacity, 0,
      ])
    );

    const bindGroup = device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: ubuf } },
        { binding: 1, resource: tileTexture.createView() },
        { binding: 2, resource: this.sampler },
      ],
    });

    const pass = encoder.beginRenderPass({
      label: `logos-composite-pass@(${screenX},${screenY})`,
      colorAttachments: [{
        view:       targetView,
        clearValue: { r: 0, g: 0, b: 0, a: 0 },
        loadOp,
        storeOp:    "store",
      }],
    });

    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(6); // fullscreen quad: 2 triangles × 3 vertices
    pass.end();
  }

  // ── Uniform buffer pool ───────────────────────────────────────────────────

  // Simple ring-buffer pool: avoids per-frame allocation when rendering
  // many tiles. Grows on demand. Bounded in practice by viewport tile count.
  private _getUniformBuffer(): GPUBuffer {
    if (this.poolCursor >= this.uniformPool.length) {
      this.uniformPool.push(
        this.device.createBuffer({
          label: `logos-composite-ubuf-${this.poolCursor}`,
          size:  UNIFORM_BYTES,
          usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
        })
      );
    }
    const buf = this.uniformPool[this.poolCursor];
    this.poolCursor++;
    return buf;
  }

  /** Must be called at the start of each frame to reset the pool cursor. */
  beginFrame(): void {
    this.poolCursor = 0;
  }

  // ── Cleanup ───────────────────────────────────────────────────────────────

  destroy(): void {
    for (const buf of this.uniformPool) buf.destroy();
    this.uniformPool = [];
  }
}
