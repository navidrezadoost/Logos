/**
 * render-webgpu/flex-layout-pipeline.ts
 *
 * Phase 5.4 — GPU-accelerated flex layout.
 *
 * GPU port of `rust/logos-layout/src/flex/` — the same algorithm that powers
 * Logos's CPU layout engine, now executed entirely on the GPU via four chained
 * WGSL compute kernels:
 *
 *   Stage 1  cs_layout_data  @wgsize(64)   — per-child constraint resolution
 *   Stage 2  cs_line_scan    @wgsize(1)    — greedy line-break scan
 *   Stage 3  cs_grow_shrink  @wgsize(64)   — per-line flex grow / shrink
 *   Stage 4  cs_place        @wgsize(64)   — per-child final position
 *
 * Typical usage:
 *
 *   const pipeline = new FlexLayoutPipeline();
 *   await pipeline.init(device);
 *
 *   const positions = await pipeline.computeLayout(children, container, w, h);
 *   // positions[i].x/y/w/h are in container-local coordinates.
 *
 * Acceptance criterion:
 *   For a 100-child container: dispatch + readback ≤ 3 ms on desktop GPU.
 *
 * Buffer layout
 * ─────────────
 *   uniforms_buf   64 B   FlexUniforms (uniform)
 *   input_buf      N×64 B ChildInput[]  (read-only storage)  — stage 1 only
 *   child_data_buf N×64 B ChildData[]   (read-write storage) — stages 1-4
 *   line_data_buf  L×32 B LineData[]    (read-write storage) — stages 2-4
 *   line_count_buf 4 B    u32           (read-write storage) — stages 2-4
 *   readback_buf   N×64 B MAP_READ staging copy of child_data_buf
 */

import layoutDataSource from "./shaders/flex-layout-data.wgsl?raw";
import positionsSource   from "./shaders/flex-positions.wgsl?raw";

import {
  FLEX_UNIFORM_BYTES,
  FLEX_CHILD_INPUT_BYTES,
  FLEX_CHILD_DATA_BYTES,
  FLEX_LINE_DATA_BYTES,
  MAX_FLEX_CHILDREN,
  MAX_FLEX_LINES,
} from "./constants";

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/** Sizing mode for a single axis — mirrors `SizingMode` in Rust. */
export type FlexSizingMode = "fix" | "fill" | "auto";

/** Align-self for a child — mirrors `AlignSelf` in Rust. */
export type FlexAlignSelf = "auto" | "start" | "end" | "center" | "stretch";

/** Justify / align content / align items enum. */
export type FlexAlign =
  | "start" | "end" | "center"
  | "space-between" | "space-around" | "space-evenly" | "stretch";

/** Direction of the flex main axis. */
export type FlexDirection = "row" | "row-reverse" | "column" | "column-reverse";

/** Wrapping behaviour. */
export type FlexWrap = "nowrap" | "wrap" | "wrap-reverse";

/**
 * Sizing constraints for one flex child.
 * Axes are given in CSS/design-tool coordinates (width = horizontal).
 * The pipeline internally rotates to main/cross based on `direction`.
 */
export interface FlexChildShape {
  /** Explicit width (undefined = absent / auto-sized). */
  width?:     number;
  /** Explicit height (undefined = absent / auto-sized). */
  height?:    number;
  minWidth?:  number;
  maxWidth?:  number;
  minHeight?: number;
  maxHeight?: number;
  hSizing:    FlexSizingMode;
  vSizing:    FlexSizingMode;
  alignSelf:  FlexAlignSelf;
  /** True → absolutely positioned child, excluded from flex flow. */
  absolute?:  boolean;
}

/** Container-level flex parameters. */
export interface FlexContainerParams {
  direction:       FlexDirection;
  wrap:            FlexWrap;
  alignItems:      FlexAlign;
  alignContent:    FlexAlign;
  justifyContent:  FlexAlign;
  /** Gap between items along the main axis. */
  gapMain:         number;
  /** Gap between lines along the cross axis. */
  gapCross:        number;
}

/** Final resolved position of one flex child, in container-local coordinates. */
export interface ChildFinalPosition {
  x: number;
  y: number;
  w: number;
  h: number;
  /** Index into the original `children` array. */
  index: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Enum encodings (must match WGSL constants)
// ─────────────────────────────────────────────────────────────────────────────

const DIRECTION: Record<FlexDirection, number> = {
  "row":            0,
  "row-reverse":    1,
  "column":         2,
  "column-reverse": 3,
};

const WRAP: Record<FlexWrap, number> = {
  "nowrap":       0,
  "wrap":         1,
  "wrap-reverse": 2,
};

const ALIGN: Record<FlexAlign, number> = {
  "start":        0,
  "end":          1,
  "center":       2,
  "space-between":3,
  "space-around": 4,
  "space-evenly": 5,
  "stretch":      6,
};

const SIZING: Record<FlexSizingMode, number> = {
  "fix":  0,
  "fill": 1,
  "auto": 2,
};

const ALIGN_SELF: Record<FlexAlignSelf, number> = {
  "auto":    0,
  "start":   1,
  "end":     2,
  "center":  3,
  "stretch": 4,
};

const NONE = -1; // sentinel for absent optional f32

// ─────────────────────────────────────────────────────────────────────────────
// FlexLayoutPipeline
// ─────────────────────────────────────────────────────────────────────────────

export class FlexLayoutPipeline {
  private device!: GPUDevice;

  // ── Stage 1: cs_layout_data ──────────────────────────────────────────────
  private bglA!:    GPUBindGroupLayout;
  private pipeA!:   GPUComputePipeline;   // cs_layout_data

  // ── Stages 2-4: positions ────────────────────────────────────────────────
  private bglB!:           GPUBindGroupLayout;
  private pipeLineScan!:   GPUComputePipeline;   // cs_line_scan
  private pipeGrowShrink!: GPUComputePipeline;   // cs_grow_shrink
  private pipePlace!:      GPUComputePipeline;   // cs_place

  // ── GPU buffers (sized at init to MAX_FLEX_CHILDREN) ─────────────────────
  private uniformsBuf!:   GPUBuffer;  // 64 B  uniform
  private inputBuf!:      GPUBuffer;  // N×64  read-only storage
  private childDataBuf!:  GPUBuffer;  // N×64  read-write storage
  private lineDataBuf!:   GPUBuffer;  // L×32  read-write storage
  private lineCountBuf!:  GPUBuffer;  // 4 B   read-write storage
  private readbackBuf!:   GPUBuffer;  // N×64  MAP_READ staging

  // ─────────────────────────────────────────────────────────────────────────
  // Lifecycle
  // ─────────────────────────────────────────────────────────────────────────

  /** Compile all pipelines and allocate GPU buffers.  Await before calling computeLayout(). */
  async init(device: GPUDevice): Promise<void> {
    this.device = device;

    const moduleA = device.createShaderModule({
      label: "logos-flex-layout-data",
      code:  layoutDataSource,
    });
    const moduleB = device.createShaderModule({
      label: "logos-flex-positions",
      code:  positionsSource,
    });

    // ── Bind group layout A — cs_layout_data ──────────────────────────────
    // binding 0: uniform  (FlexUniforms)
    // binding 1: read-only-storage  (ChildInput[])
    // binding 2: storage  (ChildData[])
    this.bglA = device.createBindGroupLayout({
      label: "logos-flex-bgl-a",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      ],
    });

    // ── Bind group layout B — cs_line_scan / cs_grow_shrink / cs_place ────
    // binding 0: uniform  (FlexUniforms)
    // binding 1: storage  (ChildData[])
    // binding 2: storage  (LineData[])
    // binding 3: storage  (line_count u32)
    this.bglB = device.createBindGroupLayout({
      label: "logos-flex-bgl-b",
      entries: [
        { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
        { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
        { binding: 3, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      ],
    });

    const layoutA = device.createPipelineLayout({ bindGroupLayouts: [this.bglA] });
    const layoutB = device.createPipelineLayout({ bindGroupLayouts: [this.bglB] });

    [this.pipeA, this.pipeLineScan, this.pipeGrowShrink, this.pipePlace] =
      await Promise.all([
        device.createComputePipelineAsync({
          label: "logos-cs-layout-data",
          layout: layoutA,
          compute: { module: moduleA, entryPoint: "cs_layout_data" },
        }),
        device.createComputePipelineAsync({
          label: "logos-cs-line-scan",
          layout: layoutB,
          compute: { module: moduleB, entryPoint: "cs_line_scan" },
        }),
        device.createComputePipelineAsync({
          label: "logos-cs-grow-shrink",
          layout: layoutB,
          compute: { module: moduleB, entryPoint: "cs_grow_shrink" },
        }),
        device.createComputePipelineAsync({
          label: "logos-cs-place",
          layout: layoutB,
          compute: { module: moduleB, entryPoint: "cs_place" },
        }),
      ]);

    // ── Allocate persistent GPU buffers ───────────────────────────────────
    const childBytes    = MAX_FLEX_CHILDREN * FLEX_CHILD_INPUT_BYTES;
    const childDataBytes= MAX_FLEX_CHILDREN * FLEX_CHILD_DATA_BYTES;
    const lineBytes     = MAX_FLEX_LINES    * FLEX_LINE_DATA_BYTES;

    this.uniformsBuf = device.createBuffer({
      label: "logos-flex-uniforms",
      size:  FLEX_UNIFORM_BYTES,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });

    this.inputBuf = device.createBuffer({
      label: "logos-flex-child-input",
      size:  childBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    this.childDataBuf = device.createBuffer({
      label: "logos-flex-child-data",
      size:  childDataBytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    });

    this.lineDataBuf = device.createBuffer({
      label: "logos-flex-line-data",
      size:  lineBytes,
      usage: GPUBufferUsage.STORAGE,
    });

    this.lineCountBuf = device.createBuffer({
      label: "logos-flex-line-count",
      size:  4,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
    });

    this.readbackBuf = device.createBuffer({
      label: "logos-flex-readback",
      size:  childDataBytes,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });
  }

  // ─────────────────────────────────────────────────────────────────────────
  // computeLayout
  // ─────────────────────────────────────────────────────────────────────────

  /**
   * Run the four flex-layout compute passes and return final child positions.
   *
   * @param children    Array of per-child sizing constraints.
   * @param container   Flex container parameters.
   * @param availW      Inner container width  (after container padding).
   * @param availH      Inner container height (after container padding).
   * @returns           Resolved (x, y, w, h) for each child in original order.
   */
  async computeLayout(
    children:  FlexChildShape[],
    container: FlexContainerParams,
    availW:    number,
    availH:    number,
  ): Promise<ChildFinalPosition[]> {
    const count = children.length;
    if (count === 0) return [];
    if (count > MAX_FLEX_CHILDREN) {
      throw new RangeError(`FlexLayoutPipeline: child count ${count} exceeds MAX_FLEX_CHILDREN (${MAX_FLEX_CHILDREN})`);
    }

    const device   = this.device;
    const isRow    = container.direction === "row" || container.direction === "row-reverse";
    const availMain  = isRow ? availW : availH;
    const availCross = isRow ? availH : availW;

    // ── Pack uniforms ──────────────────────────────────────────────────────
    const uniforms = new ArrayBuffer(FLEX_UNIFORM_BYTES);
    const uv = new DataView(uniforms);
    let o = 0;
    uv.setUint32(o,  DIRECTION[container.direction],      true); o += 4;
    uv.setUint32(o,  WRAP[container.wrap],                true); o += 4;
    uv.setUint32(o,  ALIGN[container.alignItems],         true); o += 4;
    uv.setUint32(o,  ALIGN[container.alignContent],       true); o += 4;
    uv.setUint32(o,  ALIGN[container.justifyContent],     true); o += 4;
    uv.setUint32(o,  count,                               true); o += 4;
    uv.setFloat32(o, container.gapMain,                   true); o += 4;
    uv.setFloat32(o, container.gapCross,                  true); o += 4;
    uv.setFloat32(o, availMain,                           true); o += 4;
    uv.setFloat32(o, availCross,                          true); o += 4;
    // remaining 24 bytes = padding (DataView default = 0)
    device.queue.writeBuffer(this.uniformsBuf, 0, uniforms);

    // ── Pack child inputs ──────────────────────────────────────────────────
    const inputBytes  = count * FLEX_CHILD_INPUT_BYTES;
    const inputData   = new ArrayBuffer(inputBytes);
    const iv          = new DataView(inputData);

    for (let i = 0; i < count; i++) {
      const c   = children[i];
      let   base = i * FLEX_CHILD_INPUT_BYTES;

      // Rotate axes: for row containers main=width, cross=height; column: reversed.
      const mainSize   = isRow ? c.width   : c.height;
      const crossSize  = isRow ? c.height  : c.width;
      const mainMinC   = isRow ? c.minWidth  : c.minHeight;
      const mainMaxC   = isRow ? c.maxWidth  : c.maxHeight;
      const crossMinC  = isRow ? c.minHeight : c.minWidth;
      const crossMaxC  = isRow ? c.maxHeight : c.maxWidth;
      const mainSizing = isRow ? c.hSizing   : c.vSizing;
      const crossSizing= isRow ? c.vSizing   : c.hSizing;

      iv.setFloat32(base +  0, mainSize  ?? NONE, true);
      iv.setFloat32(base +  4, crossSize ?? NONE, true);
      iv.setFloat32(base +  8, mainMinC  ?? NONE, true);
      iv.setFloat32(base + 12, mainMaxC  ?? NONE, true);
      iv.setFloat32(base + 16, crossMinC ?? NONE, true);
      iv.setFloat32(base + 20, crossMaxC ?? NONE, true);
      iv.setUint32( base + 24, SIZING[mainSizing],          true);
      iv.setUint32( base + 28, SIZING[crossSizing],         true);
      iv.setUint32( base + 32, ALIGN_SELF[c.alignSelf],     true);
      iv.setUint32( base + 36, c.absolute ? 1 : 0,          true);
      // bytes 40-63: padding (zero)
    }
    device.queue.writeBuffer(this.inputBuf, 0, inputData);

    // ── Reset line_count to 0 ─────────────────────────────────────────────
    const zero = new Uint32Array([0]);
    device.queue.writeBuffer(this.lineCountBuf, 0, zero);

    // ── Bind groups ────────────────────────────────────────────────────────
    const bgA = device.createBindGroup({
      label:  "logos-flex-bg-a",
      layout: this.bglA,
      entries: [
        { binding: 0, resource: { buffer: this.uniformsBuf } },
        { binding: 1, resource: { buffer: this.inputBuf,     size: inputBytes } },
        { binding: 2, resource: { buffer: this.childDataBuf, size: count * FLEX_CHILD_DATA_BYTES } },
      ],
    });

    const bgB = device.createBindGroup({
      label:  "logos-flex-bg-b",
      layout: this.bglB,
      entries: [
        { binding: 0, resource: { buffer: this.uniformsBuf } },
        { binding: 1, resource: { buffer: this.childDataBuf, size: count * FLEX_CHILD_DATA_BYTES } },
        { binding: 2, resource: { buffer: this.lineDataBuf } },
        { binding: 3, resource: { buffer: this.lineCountBuf } },
      ],
    });

    // ── Encode + submit ─────────────────────────────────────────────────────
    const enc = device.createCommandEncoder({ label: "logos-flex-layout" });
    const pass = enc.beginComputePass({ label: "logos-flex-compute" });

    // Stage 1: cs_layout_data — one thread per child.
    pass.setPipeline(this.pipeA);
    pass.setBindGroup(0, bgA);
    pass.dispatchWorkgroups(Math.ceil(count / 64));

    // Barrier (implicit between dispatchWorkgroups in WGSL storage).
    // Stage 2: cs_line_scan — serial, single thread.
    pass.setPipeline(this.pipeLineScan);
    pass.setBindGroup(0, bgB);
    pass.dispatchWorkgroups(1);

    // Stage 3: cs_grow_shrink — one thread per line.
    pass.setPipeline(this.pipeGrowShrink);
    pass.setBindGroup(0, bgB);
    pass.dispatchWorkgroups(Math.ceil(MAX_FLEX_LINES / 64));

    // Stage 4: cs_place — one thread per line (same dispatch pattern as gs).
    pass.setPipeline(this.pipePlace);
    pass.setBindGroup(0, bgB);
    pass.dispatchWorkgroups(Math.ceil(MAX_FLEX_LINES / 64));

    pass.end();

    // Copy child_data → readback staging buffer.
    enc.copyBufferToBuffer(
      this.childDataBuf, 0,
      this.readbackBuf,  0,
      count * FLEX_CHILD_DATA_BYTES,
    );

    device.queue.submit([enc.finish()]);

    // ── Readback ────────────────────────────────────────────────────────────
    await this.readbackBuf.mapAsync(GPUMapMode.READ, 0, count * FLEX_CHILD_DATA_BYTES);
    const raw = new Float32Array(
      this.readbackBuf.getMappedRange(0, count * FLEX_CHILD_DATA_BYTES).slice(0),
    );
    this.readbackBuf.unmap();

    // ── Decode results ──────────────────────────────────────────────────────
    // ChildData stride = FLEX_CHILD_DATA_BYTES / 4 = 16 f32 words.
    // Offsets (words):
    //   0=main_min, 1=main_max, 2=cross_min, 3=cross_max,
    //   4=flex_grow, 5=flex_shrink, 6=flex_basis,
    //   7=main_fill(u32), 8=cross_fill(u32), 9=absolute(u32), 10=align_self(u32),
    //   11=line_idx(u32),
    //   12=main_size, 13=cross_size, 14=main_offset, 15=cross_offset

    const STRIDE = FLEX_CHILD_DATA_BYTES / 4; // 16
    const MAIN_SIZE_W   = 12;
    const CROSS_SIZE_W  = 13;
    const MAIN_OFFSET_W = 14;
    const CROSS_OFFSET_W= 15;

    const positions: ChildFinalPosition[] = new Array(count);

    for (let i = 0; i < count; i++) {
      const base = i * STRIDE;
      const mainSize    = raw[base + MAIN_SIZE_W];
      const crossSize   = raw[base + CROSS_SIZE_W];
      const mainOffset  = raw[base + MAIN_OFFSET_W];
      const crossOffset = raw[base + CROSS_OFFSET_W];

      // Convert back from main/cross to x/y
      positions[i] = isRow
        ? { x: mainOffset,  y: crossOffset, w: mainSize,  h: crossSize,  index: i }
        : { x: crossOffset, y: mainOffset,  w: crossSize, h: mainSize,   index: i };
    }

    return positions;
  }

  // ─────────────────────────────────────────────────────────────────────────
  // Cleanup
  // ─────────────────────────────────────────────────────────────────────────

  destroy(): void {
    this.uniformsBuf?.destroy();
    this.inputBuf?.destroy();
    this.childDataBuf?.destroy();
    this.lineDataBuf?.destroy();
    this.lineCountBuf?.destroy();
    this.readbackBuf?.destroy();
  }
}
