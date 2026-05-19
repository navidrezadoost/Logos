/**
 * render-webgpu/tile-pipeline.ts
 *
 * WebGPU render pipeline for tiled shape rendering.
 *
 * Each tile is a 512×512 render target. Shapes are drawn as instanced quads
 * (6 vertices × shapeCount instances, no index buffer) using tile.wgsl.
 *
 * Usage pattern:
 *   const tp = new TilePipeline(device, format);
 *   tp.setShapeBuffer(shapeBuffer, shapeCount);
 *   tp.renderTile(encoder, tileTexture, tileOriginX, tileOriginY, tileSizePx, scale);
 */

import tileShaderSource from "./shaders/tile.wgsl?raw";
import { TILE_SIZE_PX } from "./constants";

// Uniform buffer layout (must match tile.wgsl `TileUniforms`):
//   [tile_origin_x, tile_origin_y, tile_size, scale,
//    viewport_w, viewport_h, global_opacity, _pad]   → 8 × f32 = 32 bytes
const UNIFORM_BYTES = 32;

export class TilePipeline {
  private pipeline!:       GPURenderPipeline;
  private uniformBuffer!:  GPUBuffer;
  private bindGroupLayout!: GPUBindGroupLayout;

  // Set externally before each flush.
  private shapeBuffer: GPUBuffer | null  = null;
  private shapeCount  = 0;

  constructor(
    private readonly device: GPUDevice,
    private readonly format: GPUTextureFormat,
  ) {
    this._build();
  }

  // ── Build pipeline ────────────────────────────────────────────────────────

  private _build(): void {
    const { device, format } = this;

    const module = device.createShaderModule({
      label:  "logos-tile-shader",
      code:   tileShaderSource,
    });

    this.bindGroupLayout = device.createBindGroupLayout({
      label: "logos-tile-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT,
          buffer: { type: "read-only-storage" } },
      ],
    });

    const pipelineLayout = device.createPipelineLayout({
      label:              "logos-tile-layout",
      bindGroupLayouts:   [this.bindGroupLayout],
    });

    this.pipeline = device.createRenderPipeline({
      label:  "logos-tile-pipeline",
      layout: pipelineLayout,
      vertex: {
        module,
        entryPoint: "vs_main",
      },
      fragment: {
        module,
        entryPoint: "fs_main",
        targets: [{
          format,
          blend: {
            color: {
              srcFactor:  "src-alpha",
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
      primitive: {
        topology: "triangle-list",
      },
    });

    this.uniformBuffer = device.createBuffer({
      label: "logos-tile-uniforms",
      size:  UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
  }

  // ── External interface ────────────────────────────────────────────────────

  setShapeBuffer(buffer: GPUBuffer, count: number): void {
    this.shapeBuffer = buffer;
    this.shapeCount  = count;
  }

  /**
   * Encode the render pass for one tile.
   *
   * @param encoder        Active GPUCommandEncoder.
   * @param targetView     The tile's texture view to render into.
   * @param tileOriginX    Canvas-space X of the tile's top-left corner.
   * @param tileOriginY    Canvas-space Y of the tile's top-left corner.
   * @param tileSizePx     Canvas-space size of the tile (default 512).
   * @param scale          Current zoom factor.
   * @param viewportW      Device-pixel viewport width.
   * @param viewportH      Device-pixel viewport height.
   * @param globalOpacity  0–1 multiplier applied to all shapes.
   * @param clearColor     RGBA clear color before drawing (default transparent).
   */
  renderTile(
    encoder:       GPUCommandEncoder,
    targetView:    GPUTextureView,
    tileOriginX:   number,
    tileOriginY:   number,
    tileSizePx:    number = TILE_SIZE_PX,
    scale:         number = 1,
    viewportW:     number = tileSizePx,
    viewportH:     number = tileSizePx,
    globalOpacity: number = 1,
    clearColor:    GPUColorDict = { r: 0, g: 0, b: 0, a: 0 },
  ): void {
    if (!this.shapeBuffer || this.shapeCount === 0) return;

    const { device } = this;

    // Upload uniforms.
    const unis = new Float32Array([
      tileOriginX, tileOriginY,
      tileSizePx,  scale,
      viewportW,   viewportH,
      globalOpacity, 0,
    ]);
    device.queue.writeBuffer(this.uniformBuffer, 0, unis);

    // Build bind group with current shape buffer.
    const bindGroup = device.createBindGroup({
      layout: this.bindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer } },
        { binding: 1, resource: { buffer: this.shapeBuffer } },
      ],
    });

    const pass = encoder.beginRenderPass({
      label: `logos-tile-pass@(${tileOriginX},${tileOriginY})`,
      colorAttachments: [{
        view:       targetView,
        clearValue: clearColor,
        loadOp:     "clear",
        storeOp:    "store",
      }],
    });

    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, bindGroup);
    // 6 vertices × shapeCount instances (instanced quads).
    pass.draw(6, this.shapeCount);
    pass.end();
  }

  destroy(): void {
    this.uniformBuffer.destroy();
  }
}
