/**
 * render-webgpu/layout-pipeline.ts
 *
 * Phase 5.1 — GPU Compute Layout Bounds
 *
 * GPU port of the `compute_bounds` function in
 * `rust/logos-layout/src/flex/bounds.rs`.
 *
 * Two-pass compute dispatch:
 *   pass 1 (cs_bounds)    — parallel max-reduction over all children's
 *                           right/bottom edges, using a 64-lane workgroup.
 *   pass 2 (cs_finalise)  — single thread: applies padding + explicit-size
 *                           override, writes final {width, height}.
 *
 * The result is read back from the GPU via a staging buffer.  This is an
 * async operation; callers `await computeBounds(...)`.
 *
 * Buffer layout
 * ─────────────
 *   Uniform (binding 0):
 *     avail_w      f32
 *     avail_h      f32
 *     pad_top      f32
 *     pad_right    f32
 *     pad_bottom   f32
 *     pad_left     f32
 *     child_count  u32
 *     _pad         u32
 *     → 32 bytes
 *
 *   Children (binding 1):  array<vec4f>  (x, y, w, h per child)
 *     child_count × 16 bytes
 *
 *   Result (binding 2):  array<atomic<u32>, 4>
 *     [0] → final width  (f32 bits)
 *     [1] → final height (f32 bits)
 *     [2] → max_right    (f32 bits, intermediate)
 *     [3] → max_bottom   (f32 bits, intermediate)
 *     → 16 bytes
 */

import layoutSource from "./shaders/layout-bounds.wgsl?raw";

const UNIFORM_BYTES = 32;
const RESULT_BYTES  = 16; // 4 × u32

const MAX_CHILDREN = 65_536; // hard cap; one binding per pipeline instance

export interface LayoutBoundsResult {
  width:  number;
  height: number;
}

export interface LayoutPadding {
  top:    number;
  right:  number;
  bottom: number;
  left:   number;
}

export class LayoutPipeline {
  private device!:          GPUDevice;
  private bgl!:             GPUBindGroupLayout;
  private boundsPipeline!:  GPUComputePipeline;
  private finalisePipeline!:GPUComputePipeline;

  private uniformBuffer!:   GPUBuffer;
  private childBuffer!:     GPUBuffer;
  private resultBuffer!:    GPUBuffer;   // storage / atomics
  private readbackBuffer!:  GPUBuffer;   // MAP_READ staging

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  /**
   * Async initialisation — await before calling `computeBounds`.
   */
  async init(device: GPUDevice): Promise<void> {
    this.device = device;

    const module = device.createShaderModule({
      label: "logos-layout-bounds-shader",
      code:  layoutSource,
    });

    // Both compute pass use the same bind group layout.
    this.bgl = device.createBindGroupLayout({
      label: "logos-layout-bgl",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE,
          buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE,
          buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE,
          buffer: { type: "storage" } },
      ],
    });

    const layout = device.createPipelineLayout({ bindGroupLayouts: [this.bgl] });

    [this.boundsPipeline, this.finalisePipeline] = await Promise.all([
      device.createComputePipelineAsync({
        label:   "logos-cs-bounds",
        layout,
        compute: { module, entryPoint: "cs_bounds" },
      }),
      device.createComputePipelineAsync({
        label:   "logos-cs-finalise",
        layout,
        compute: { module, entryPoint: "cs_finalise" },
      }),
    ]);

    // Static-size GPU buffers (re-used every call).
    this.uniformBuffer = device.createBuffer({
      label: "logos-layout-uniforms",
      size:  UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.childBuffer = device.createBuffer({
      label: "logos-layout-children",
      size:  MAX_CHILDREN * 16, // 16 bytes per child (x,y,w,h)
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    this.resultBuffer = device.createBuffer({
      label: "logos-layout-result",
      size:  RESULT_BYTES,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST,
    });

    this.readbackBuffer = device.createBuffer({
      label: "logos-layout-readback",
      size:  RESULT_BYTES,
      usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
    });
  }

  // ── Compute ───────────────────────────────────────────────────────────────

  /**
   * GPU-compute the bounding box of `children` (mirroring
   * `logos-layout/flex/bounds.rs::compute_bounds`).
   *
   * @param children Float32Array packed as [x0,y0,w0,h0, x1,y1,w1,h1, …]
   *                 (at most MAX_CHILDREN = 65 536 entries).
   * @param availW   Available container width  (used when explicit sizing omitted).
   * @param availH   Available container height.
   * @param padding  Padding struct {top, right, bottom, left}.
   * @returns        Resolved {width, height} from GPU.
   */
  async computeBounds(
    children: Float32Array,
    availW:   number,
    availH:   number,
    padding:  LayoutPadding,
  ): Promise<LayoutBoundsResult> {
    const { device } = this;
    const childCount = children.length / 4;

    if (childCount > MAX_CHILDREN) {
      throw new RangeError(
        `LayoutPipeline: too many children (${childCount} > ${MAX_CHILDREN})`
      );
    }

    // ── 1. Upload uniforms
    device.queue.writeBuffer(
      this.uniformBuffer, 0,
      new Float32Array([
        availW, availH,
        padding.top, padding.right, padding.bottom, padding.left,
      ])
    );
    // child_count + pad as u32 at byte offset 24
    device.queue.writeBuffer(
      this.uniformBuffer, 24,
      new Uint32Array([childCount, 0])
    );

    // ── 2. Upload children
    device.queue.writeBuffer(this.childBuffer, 0, children);

    // ── 3. Reset result buffer (all zeros = 0.0f for max-reduction seed)
    device.queue.writeBuffer(this.resultBuffer, 0, new Uint32Array(4));

    // ── 4. Build bind group
    const bindGroup = device.createBindGroup({
      layout: this.bgl,
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuffer } },
        { binding: 1, resource: { buffer: this.childBuffer } },
        { binding: 2, resource: { buffer: this.resultBuffer } },
      ],
    });

    // ── 5. Encode two compute passes (sequential — finalise reads bounds result)
    const encoder = device.createCommandEncoder({ label: "logos-layout-encoder" });

    // Pass A: parallel max-reduction (64-lane workgroups)
    {
      const pass = encoder.beginComputePass({ label: "logos-cs-bounds-pass" });
      pass.setPipeline(this.boundsPipeline);
      pass.setBindGroup(0, bindGroup);
      pass.dispatchWorkgroups(Math.ceil(childCount / 64));
      pass.end();
    }

    // Pass B: single-thread finalise (applies padding + explicit-size logic)
    {
      const pass = encoder.beginComputePass({ label: "logos-cs-finalise-pass" });
      pass.setPipeline(this.finalisePipeline);
      pass.setBindGroup(0, bindGroup);
      pass.dispatchWorkgroups(1);
      pass.end();
    }

    // Copy result[0..1] (width, height) to readback buffer
    encoder.copyBufferToBuffer(this.resultBuffer, 0, this.readbackBuffer, 0, RESULT_BYTES);

    device.queue.submit([encoder.finish()]);

    // ── 6. Read back result
    await this.readbackBuffer.mapAsync(GPUMapMode.READ, 0, RESULT_BYTES);
    const raw    = new Float32Array(this.readbackBuffer.getMappedRange(0, RESULT_BYTES));
    const width  = raw[0];
    const height = raw[1];
    this.readbackBuffer.unmap();

    return { width, height };
  }

  // ── Cleanup ───────────────────────────────────────────────────────────────

  destroy(): void {
    this.uniformBuffer?.destroy();
    this.childBuffer?.destroy();
    this.resultBuffer?.destroy();
    this.readbackBuffer?.destroy();
  }
}
