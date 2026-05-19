/**
 * render-webgpu/snap-pipeline.ts
 *
 * GPU compute pipeline for snapping candidate detection.
 *
 * Each shape contributes 8 candidate snap points (corners + edge midpoints).
 * The shader finds the nearest within `thresholdPx` canvas pixels.
 *
 * Usage:
 *   const sp = new SnapPipeline(device);
 *   await sp.init();
 *
 *   const snapped = await sp.findSnap(shapeBuffer, shapeCount, cursorX, cursorY, 8);
 *   if (snapped) {
 *     drawSnapIndicator(snapped.x, snapped.y);
 *   }
 */

import snapSource from "./shaders/snap.wgsl?raw";

// Uniform layout: [cursor_x, cursor_y, threshold2, shape_count] → 4 × f32 = 16 bytes
const UNIFORM_BYTES = 16;
// Result buffer: [best_x_bits, best_y_bits, best_dist2_bits, found_u32] → 4 × u32 = 16 bytes
const RESULT_BYTES  = 16;
// Workgroup size: 64 threads (matches @workgroup_size in snap.wgsl).
const WORKGROUP_SIZE = 64;
// 8 candidates per shape.
const CANDIDATES_PER_SHAPE = 8;

export interface SnapResult {
  x:     number;
  y:     number;
  dist:  number;
}

export class SnapPipeline {
  private pipeline!:      GPUComputePipeline;
  private uniformBuffer!: GPUBuffer;
  private resultBuffer!:  GPUBuffer;
  private readBuffer!:    GPUBuffer;
  private bgl!:           GPUBindGroupLayout;

  constructor(private readonly device: GPUDevice) {}

  async init(): Promise<void> {
    const { device } = this;

    const module = device.createShaderModule({
      label: "logos-snap-shader",
      code:  snapSource,
    });

    this.bgl = device.createBindGroupLayout({
      label: "logos-snap-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      ],
    });

    this.pipeline = await device.createComputePipelineAsync({
      label:  "logos-snap-pipeline",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.bgl] }),
      compute: { module, entryPoint: "cs_snap" },
    });

    this.uniformBuffer = device.createBuffer({
      label: "logos-snap-uniforms",
      size:  UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.resultBuffer = device.createBuffer({
      label: "logos-snap-result",
      size:  RESULT_BYTES,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
    });

    this.readBuffer = device.createBuffer({
      label: "logos-snap-read",
      size:  RESULT_BYTES,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
  }

  /**
   * Find the nearest snap point within `thresholdPx` canvas pixels.
   *
   * @returns Snap coordinates and distance, or `null` if nothing is within range.
   */
  async findSnap(
    shapeBuffer:  GPUBuffer,
    shapeCount:   number,
    cursorX:      number,
    cursorY:      number,
    thresholdPx:  number = 8,
  ): Promise<SnapResult | null> {
    if (shapeCount === 0) return null;

    const { device } = this;
    const threshold2 = thresholdPx * thresholdPx;

    // Upload uniforms.
    device.queue.writeBuffer(
      this.uniformBuffer, 0,
      new Float32Array([cursorX, cursorY, threshold2, shapeCount])
    );

    // Reset result: dist2 = MAX_FLOAT (0x7F7FFFFF), found = 0.
    device.queue.writeBuffer(
      this.resultBuffer, 0,
      new Uint32Array([0, 0, 0x7F7FFFFF, 0])
    );

    const bindGroup = device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer } },
        { binding: 1, resource: { buffer: shapeBuffer } },
        { binding: 2, resource: { buffer: this.resultBuffer } },
      ],
    });

    const enc  = device.createCommandEncoder({ label: "logos-snap-enc" });
    const pass = enc.beginComputePass({ label: "logos-snap-pass" });
    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, bindGroup);
    const totalThreads = shapeCount * CANDIDATES_PER_SHAPE;
    pass.dispatchWorkgroups(Math.ceil(totalThreads / WORKGROUP_SIZE));
    pass.end();

    enc.copyBufferToBuffer(this.resultBuffer, 0, this.readBuffer, 0, RESULT_BYTES);
    device.queue.submit([enc.finish()]);

    // Read back.
    await this.readBuffer.mapAsync(GPUMapMode.READ);
    const u32s   = new Uint32Array(this.readBuffer.getMappedRange());
    const f32s   = new Float32Array(u32s.buffer);
    const found  = u32s[3] !== 0;
    const snapX  = f32s[0];
    const snapY  = f32s[1];
    const dist2  = f32s[2];
    this.readBuffer.unmap();

    if (!found || dist2 > threshold2) return null;
    return { x: snapX, y: snapY, dist: Math.sqrt(dist2) };
  }

  destroy(): void {
    this.uniformBuffer.destroy();
    this.resultBuffer.destroy();
    this.readBuffer.destroy();
  }
}
