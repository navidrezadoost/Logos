/**
 * render-webgpu/hit-test-pipeline.ts
 *
 * GPU compute pipeline for AABB + ellipse hit-testing.
 *
 * Replaces the synchronous JS `shapes.find()` loop with a parallel shader
 * dispatch.  The result is read back asynchronously; callers await a Promise.
 *
 * Usage:
 *   const htp = new HitTestPipeline(device);
 *   await htp.init();
 *
 *   // On mouse-move (debounced):
 *   const hitIndex = await htp.test(shapeBuffer, shapeCount, cursorX, cursorY);
 *   // hitIndex === -1 → miss; otherwise index into the shapes array.
 */

import hitTestSource from "./shaders/hit-test.wgsl?raw";

// Uniform layout: [cursor_x, cursor_y, shape_count, _pad] → 4 × f32 = 16 bytes
const UNIFORM_BYTES  = 16;
// Result buffer: 1 × u32 (atomic)
const RESULT_BYTES   = 4;
// Sentinel value meaning "no hit".
const NO_HIT_SENTINEL = 0xFFFF_FFFF;
// Workgroup size must match @workgroup_size in hit-test.wgsl.
const WORKGROUP_SIZE  = 64;

export class HitTestPipeline {
  private pipeline!:      GPUComputePipeline;
  private uniformBuffer!: GPUBuffer;
  private resultBuffer!:  GPUBuffer;
  private readBuffer!:    GPUBuffer;
  private bgl!:           GPUBindGroupLayout;

  constructor(private readonly device: GPUDevice) {}

  async init(): Promise<void> {
    const { device } = this;

    const module = device.createShaderModule({
      label: "logos-hit-test-shader",
      code:  hitTestSource,
    });

    this.bgl = device.createBindGroupLayout({
      label: "logos-hit-test-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      ],
    });

    this.pipeline = await device.createComputePipelineAsync({
      label:  "logos-hit-test-pipeline",
      layout: device.createPipelineLayout({ bindGroupLayouts: [this.bgl] }),
      compute: { module, entryPoint: "cs_hit_test" },
    });

    this.uniformBuffer = device.createBuffer({
      label: "logos-hit-test-uniforms",
      size:  UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.resultBuffer = device.createBuffer({
      label: "logos-hit-test-result",
      size:  RESULT_BYTES,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
    });

    this.readBuffer = device.createBuffer({
      label:            "logos-hit-test-read",
      size:             RESULT_BYTES,
      usage:            GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
  }

  /**
   * Run the hit-test for (cursorX, cursorY) against the shape buffer.
   *
   * @returns The index of the topmost hit shape, or -1 if none.
   */
  async test(
    shapeBuffer: GPUBuffer,
    shapeCount:  number,
    cursorX:     number,
    cursorY:     number,
  ): Promise<number> {
    if (shapeCount === 0) return -1;

    const { device } = this;

    // Upload uniforms.
    device.queue.writeBuffer(
      this.uniformBuffer, 0,
      new Float32Array([cursorX, cursorY, shapeCount, 0])
    );

    // Reset result to sentinel (no hit).
    device.queue.writeBuffer(
      this.resultBuffer, 0,
      new Uint32Array([NO_HIT_SENTINEL])
    );

    const bindGroup = device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer } },
        { binding: 1, resource: { buffer: shapeBuffer } },
        { binding: 2, resource: { buffer: this.resultBuffer } },
      ],
    });

    const enc = device.createCommandEncoder({ label: "logos-hit-test-enc" });
    const pass = enc.beginComputePass({ label: "logos-hit-test-pass" });
    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.dispatchWorkgroups(Math.ceil(shapeCount / WORKGROUP_SIZE));
    pass.end();

    enc.copyBufferToBuffer(this.resultBuffer, 0, this.readBuffer, 0, RESULT_BYTES);
    device.queue.submit([enc.finish()]);

    // Read back result.
    await this.readBuffer.mapAsync(GPUMapMode.READ);
    const view   = new Uint32Array(this.readBuffer.getMappedRange());
    const result = view[0];
    this.readBuffer.unmap();

    return result === NO_HIT_SENTINEL ? -1 : result;
  }

  destroy(): void {
    this.uniformBuffer.destroy();
    this.resultBuffer.destroy();
    this.readBuffer.destroy();
  }
}
