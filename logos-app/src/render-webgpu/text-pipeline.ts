/**
 * render-webgpu/text-pipeline.ts
 *
 * Phase 5.2 — Text Rendering Pipeline
 *
 * Renders all text shapes visible within a tile as instanced glyph quads,
 * sampling glyphs from the shared GlyphAtlas.
 *
 * Relation to TilePipeline
 * ────────────────────────
 *   Text shapes are **excluded** from the main `TilePipeline` shape buffer.
 *   `TileRenderer` separates the uploaded shapes into:
 *     • non-text shapes → TilePipeline
 *     • text shapes     → TextPipeline
 *
 *   `TextPipeline.renderTile()` is called immediately after
 *   `TilePipeline.renderTile()` into the same tile texture, so text
 *   composites on top of background shapes.
 *
 * Usage
 * ─────
 *   const tp = new TextPipeline(device, format, glyphAtlas);
 *
 *   // Every frame, after glyphAtlas.flush():
 *   tp.renderTile(
 *     encoder, tileView,
 *     tileOriginX, tileOriginY, tileSize, scale,
 *     viewportW, viewportH,
 *     textShapesVisibleInTile
 *   );
 */

import textShaderSource from "./shaders/text.wgsl?raw";
import type { GlyphAtlas } from "./glyph-atlas";
import type { Shape } from "../types/shapes";
import {
  TILE_SIZE_PX,
  GLYPH_INSTANCE_F32S,
  GLYPH_INSTANCE_BYTES,
  MAX_GLYPHS_PER_FRAME,
} from "./constants";

// Uniform layout — must match TextUniforms in text.wgsl (8 × f32 = 32 bytes).
const UNIFORM_BYTES = 32;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Parse a CSS hex color to [r, g, b] ∈ [0, 1]. */
function hexToLinear(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const s = h.length === 3 ? h.split("").map((c) => c + c).join("") : h.slice(0, 6);
  return [
    parseInt(s.slice(0, 2), 16) / 255,
    parseInt(s.slice(2, 4), 16) / 255,
    parseInt(s.slice(4, 6), 16) / 255,
  ];
}

// ─────────────────────────────────────────────────────────────────────────────
// TextPipeline
// ─────────────────────────────────────────────────────────────────────────────

export class TextPipeline {
  private pipeline!:         GPURenderPipeline;
  private uniformBuffer!:    GPUBuffer;
  private instanceBuffer!:   GPUBuffer;
  private bgl!:              GPUBindGroupLayout;

  /** CPU-side instance data, reset each tile render. */
  private readonly instanceData = new Float32Array(MAX_GLYPHS_PER_FRAME * GLYPH_INSTANCE_F32S);

  constructor(
    private readonly device: GPUDevice,
    private readonly format: GPUTextureFormat,
    private readonly atlas:  GlyphAtlas,
  ) {
    this._build();
  }

  // ── Build ─────────────────────────────────────────────────────────────────

  private _build(): void {
    const { device, format, atlas } = this;

    const module = device.createShaderModule({
      label: "logos-text-shader",
      code:  textShaderSource,
    });

    this.bgl = device.createBindGroupLayout({
      label: "logos-text-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.FRAGMENT,
          texture: { sampleType: "float", viewDimension: "2d" } },
        { binding: 3, visibility: GPUShaderStage.FRAGMENT,
          sampler: { type: "filtering" } },
      ],
    });

    this.pipeline = device.createRenderPipeline({
      label:  "logos-text-pipeline",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.bgl] }),
      vertex:   { module, entryPoint: "vs_main" },
      fragment: {
        module,
        entryPoint: "fs_main",
        targets: [{
          format,
          blend: {
            // Text glyphs are output pre-multiplied by text.wgsl.
            color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
            alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
          },
        }],
      },
      primitive: { topology: "triangle-list" },
    });

    this.uniformBuffer = device.createBuffer({
      label: "logos-text-uniforms",
      size:  UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.instanceBuffer = device.createBuffer({
      label: "logos-text-instances",
      size:  MAX_GLYPHS_PER_FRAME * GLYPH_INSTANCE_BYTES,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    void atlas; // Atlas is used when building bind groups at render time.
  }

  // ── Render ────────────────────────────────────────────────────────────────

  /**
   * Encode the text render pass for one tile.
   *
   * @param encoder       Active GPUCommandEncoder.
   * @param targetView    The tile's texture view (rendered into after TilePipeline).
   * @param tileOriginX   Canvas-space X of the tile's top-left corner.
   * @param tileOriginY   Canvas-space Y of the tile's top-left corner.
   * @param tileSizePx    Canvas-space tile size (typically 512).
   * @param scale         Zoom factor.
   * @param viewportW     Device-pixel viewport width.
   * @param viewportH     Device-pixel viewport height.
   * @param textShapes    Text shapes to render.  Caller filters to visible ones.
   * @param globalOpacity Global opacity multiplier.
   */
  renderTile(
    encoder:       GPUCommandEncoder,
    targetView:    GPUTextureView,
    tileOriginX:   number,
    tileOriginY:   number,
    tileSizePx:    number,
    scale:         number,
    viewportW:     number,
    viewportH:     number,
    textShapes:    Shape[],
    globalOpacity: number = 1,
  ): void {
    if (textShapes.length === 0) return;

    const { device, atlas } = this;

    // ── 1. Pack glyph instances ─────────────────────────────────────────
    const iData = this.instanceData;
    let glyphCount = 0;

    for (const shape of textShapes) {
      if (shape.hidden) continue;

      const text       = shape.text ?? shape.name;
      const family     = shape.fontFamily  ?? "sans-serif";
      const weight     = shape.fontWeight  ?? 400;
      const sizePx     = shape.fontSize    ?? 16;
      const shapeOpacity = shape.opacity ?? 1;

      // Derive fill colour.
      let r = 0, g = 0, b = 0, a = 1;
      if (shape.textColor) {
        [r, g, b] = hexToLinear(shape.textColor);
        a = shape.textOpacity ?? 1;
      } else if (shape.fills.length > 0 && shape.fills[0].type === "solid") {
        const f = shape.fills[0];
        [r, g, b] = hexToLinear(f.color);
        a = f.opacity;
      }

      // Split on newlines.
      const lines = text.split("\n");
      const lineHeight = sizePx * 1.3;

      for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
        const line      = lines[lineIdx];
        const baselineY = shape.bounds.y + sizePx + lineIdx * lineHeight;
        const glyphs    = atlas.layoutText(line, family, weight, sizePx, shape.bounds.x, baselineY);

        for (const glyph of glyphs) {
          if (glyphCount >= MAX_GLYPHS_PER_FRAME) {
            console.warn("[logos/webgpu] TextPipeline: MAX_GLYPHS_PER_FRAME exceeded.");
            break;
          }

          const off = glyphCount * GLYPH_INSTANCE_F32S;
          iData[off + 0]  = glyph.x;
          iData[off + 1]  = glyph.y;
          iData[off + 2]  = glyph.w;
          iData[off + 3]  = glyph.h;
          iData[off + 4]  = glyph.uv.u;
          iData[off + 5]  = glyph.uv.v;
          iData[off + 6]  = glyph.uv.uw;
          iData[off + 7]  = glyph.uv.vh;
          iData[off + 8]  = r;
          iData[off + 9]  = g;
          iData[off + 10] = b;
          iData[off + 11] = a;
          iData[off + 12] = shapeOpacity;
          iData[off + 13] = 0;
          iData[off + 14] = 0;
          iData[off + 15] = 0;
          glyphCount++;
        }
      }
    }

    if (glyphCount === 0) return;

    // ── 2. Upload to GPU ────────────────────────────────────────────────
    device.queue.writeBuffer(
      this.uniformBuffer, 0,
      new Float32Array([
        tileOriginX, tileOriginY, tileSizePx, scale,
        viewportW,   viewportH,   globalOpacity, 0,
      ])
    );

    device.queue.writeBuffer(
      this.instanceBuffer, 0,
      iData.subarray(0, glyphCount * GLYPH_INSTANCE_F32S)
    );

    // ── 3. Bind group ───────────────────────────────────────────────────
    const bindGroup = device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer } },
        { binding: 1, resource: { buffer: this.instanceBuffer } },
        { binding: 2, resource: atlas.texture.createView() },
        { binding: 3, resource: atlas.sampler },
      ],
    });

    // ── 4. Render pass — loadOp "load" to composite over tile shapes ──
    const pass = encoder.beginRenderPass({
      label: `logos-text-pass@(${tileOriginX},${tileOriginY})`,
      colorAttachments: [{
        view:    targetView,
        loadOp:  "load",
        storeOp: "store",
      }],
    });

    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(6, glyphCount); // 6 vertices per quad × N glyph instances
    pass.end();
  }

  // ── Cleanup ───────────────────────────────────────────────────────────────

  destroy(): void {
    this.uniformBuffer.destroy();
    this.instanceBuffer.destroy();
  }
}
