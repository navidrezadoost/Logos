/**
 * render-webgpu/webgpu-types.d.ts
 *
 * Minimal WebGPU ambient declarations for the Phase 5 PoC.
 *
 * Replace with:
 *   npm install --save-dev @webgpu/types
 *   // vite-env.d.ts: /// <reference types="@webgpu/types" />
 * when network access is available.
 *
 * This stub covers only the APIs used by logos-app/src/render-webgpu/*.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Enums / flag objects
// ─────────────────────────────────────────────────────────────────────────────

declare const GPUBufferUsage: {
  readonly MAP_READ:    number;
  readonly MAP_WRITE:   number;
  readonly COPY_SRC:    number;
  readonly COPY_DST:    number;
  readonly INDEX:       number;
  readonly VERTEX:      number;
  readonly UNIFORM:     number;
  readonly STORAGE:     number;
  readonly INDIRECT:    number;
  readonly QUERY_RESOLVE: number;
};

declare const GPUShaderStage: {
  readonly VERTEX:   number;
  readonly FRAGMENT: number;
  readonly COMPUTE:  number;
};

declare const GPUTextureUsage: {
  readonly COPY_SRC:          number;
  readonly COPY_DST:          number;
  readonly TEXTURE_BINDING:   number;
  readonly STORAGE_BINDING:   number;
  readonly RENDER_ATTACHMENT: number;
};

declare const GPUMapMode: {
  readonly READ:  number;
  readonly WRITE: number;
};

// ─────────────────────────────────────────────────────────────────────────────
// Primitive types
// ─────────────────────────────────────────────────────────────────────────────

type GPUTextureFormat = string;
type GPUFeatureName   = string;

interface GPUColorDict { r: number; g: number; b: number; a: number }

// ─────────────────────────────────────────────────────────────────────────────
// Core objects
// ─────────────────────────────────────────────────────────────────────────────

interface GPUObjectBase {
  label?: string;
}

interface GPU {
  requestAdapter(options?: { powerPreference?: "high-performance" | "low-power" }): Promise<GPUAdapter | null>;
  getPreferredCanvasFormat(): GPUTextureFormat;
}

interface GPUAdapterInfo {
  vendor?: string;
  architecture?: string;
  device?: string;
  description?: string;
}

interface GPUAdapter {
  readonly features: ReadonlySet<string>;
  readonly info?: GPUAdapterInfo;
  requestDevice(descriptor?: {
    requiredFeatures?: GPUFeatureName[];
    label?: string;
  }): Promise<GPUDevice>;
}

interface GPUDeviceLostInfo {
  readonly reason: string;
  readonly message: string;
}

interface GPUQueue extends GPUObjectBase {
  writeBuffer(buffer: GPUBuffer, bufferOffset: number, data: ArrayBufferView | ArrayBuffer, dataOffset?: number, size?: number): void;
  submit(commandBuffers: GPUCommandBuffer[]): void;
}

interface GPUDevice extends GPUObjectBase {
  readonly queue:   GPUQueue;
  readonly lost:    Promise<GPUDeviceLostInfo>;
  readonly features: ReadonlySet<string>;

  createBuffer(descriptor: {
    label?: string; size: number; usage: number; mappedAtCreation?: boolean;
  }): GPUBuffer;

  createTexture(descriptor: {
    label?: string; size: [number, number] | [number, number, number]; format: GPUTextureFormat; usage: number; mipLevelCount?: number; sampleCount?: number;
  }): GPUTexture;

  createShaderModule(descriptor: { label?: string; code: string }): GPUShaderModule;

  createBindGroupLayout(descriptor: {
    label?: string;
    entries: GPUBindGroupLayoutEntry[];
  }): GPUBindGroupLayout;

  createPipelineLayout(descriptor: {
    label?: string;
    bindGroupLayouts: GPUBindGroupLayout[];
  }): GPUPipelineLayout;

  createRenderPipeline(descriptor: GPURenderPipelineDescriptor): GPURenderPipeline;

  createComputePipelineAsync(descriptor: {
    label?: string;
    layout: GPUPipelineLayout;
    compute: { module: GPUShaderModule; entryPoint: string };
  }): Promise<GPUComputePipeline>;

  createBindGroup(descriptor: {
    label?: string;
    layout: GPUBindGroupLayout;
    entries: Array<{ binding: number; resource: { buffer: GPUBuffer; offset?: number; size?: number } | GPUTextureView | GPUSampler }>;
  }): GPUBindGroup;

  createCommandEncoder(descriptor?: { label?: string }): GPUCommandEncoder;
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipelines
// ─────────────────────────────────────────────────────────────────────────────

interface GPUShaderModule extends GPUObjectBase {}
interface GPUBindGroupLayout extends GPUObjectBase {}
interface GPUBindGroup extends GPUObjectBase {}
interface GPUPipelineLayout extends GPUObjectBase {}
interface GPURenderPipeline extends GPUObjectBase {}
interface GPUComputePipeline extends GPUObjectBase {}

interface GPUBindGroupLayoutEntry {
  binding:    number;
  visibility: number;
  buffer?: { type: "uniform" | "read-only-storage" | "storage" };
  texture?: { sampleType?: string; viewDimension?: string };
  sampler?: { type?: string };
}

interface GPURenderPipelineDescriptor {
  label?:  string;
  layout:  GPUPipelineLayout;
  vertex:  { module: GPUShaderModule; entryPoint: string; buffers?: unknown[] };
  fragment?: {
    module: GPUShaderModule;
    entryPoint: string;
    targets: Array<{
      format: GPUTextureFormat;
      blend?: {
        color: { srcFactor: string; dstFactor: string; operation: string };
        alpha: { srcFactor: string; dstFactor: string; operation: string };
      };
    }>;
  };
  primitive?: { topology?: string; cullMode?: string };
  depthStencil?: unknown;
  multisample?: unknown;
}

// ─────────────────────────────────────────────────────────────────────────────
// Buffers & textures
// ─────────────────────────────────────────────────────────────────────────────

interface GPUBuffer extends GPUObjectBase {
  readonly size:  number;
  readonly usage: number;
  mapAsync(mode: number, offset?: number, size?: number): Promise<void>;
  getMappedRange(offset?: number, size?: number): ArrayBuffer;
  unmap(): void;
  destroy(): void;
}

interface GPUTexture extends GPUObjectBase {
  createView(descriptor?: { label?: string; dimension?: string; format?: GPUTextureFormat }): GPUTextureView;
  destroy(): void;
}

interface GPUTextureView extends GPUObjectBase {}
interface GPUSampler extends GPUObjectBase {}

// ─────────────────────────────────────────────────────────────────────────────
// Encoders & passes
// ─────────────────────────────────────────────────────────────────────────────

interface GPUCommandBuffer {}

interface GPUCommandEncoder extends GPUObjectBase {
  beginRenderPass(descriptor: {
    label?: string;
    colorAttachments: Array<{
      view: GPUTextureView;
      clearValue?: GPUColorDict;
      loadOp: "clear" | "load";
      storeOp: "store" | "discard";
    }>;
    depthStencilAttachment?: unknown;
  }): GPURenderPassEncoder;

  beginComputePass(descriptor?: { label?: string }): GPUComputePassEncoder;

  copyBufferToBuffer(
    source: GPUBuffer, sourceOffset: number,
    destination: GPUBuffer, destinationOffset: number,
    size: number
  ): void;

  copyTextureToTexture(source: unknown, destination: unknown, copySize: unknown): void;

  finish(descriptor?: { label?: string }): GPUCommandBuffer;
}

interface GPURenderPassEncoder extends GPUObjectBase {
  setPipeline(pipeline: GPURenderPipeline): void;
  setBindGroup(index: number, bindGroup: GPUBindGroup, dynamicOffsets?: number[]): void;
  setVertexBuffer(slot: number, buffer: GPUBuffer, offset?: number, size?: number): void;
  setIndexBuffer(buffer: GPUBuffer, indexFormat: string, offset?: number, size?: number): void;
  draw(vertexCount: number, instanceCount?: number, firstVertex?: number, firstInstance?: number): void;
  drawIndexed(indexCount: number, instanceCount?: number, firstIndex?: number, baseVertex?: number, firstInstance?: number): void;
  end(): void;
}

interface GPUComputePassEncoder extends GPUObjectBase {
  setPipeline(pipeline: GPUComputePipeline): void;
  setBindGroup(index: number, bindGroup: GPUBindGroup, dynamicOffsets?: number[]): void;
  dispatchWorkgroups(x: number, y?: number, z?: number): void;
  end(): void;
}

// ─────────────────────────────────────────────────────────────────────────────
// Canvas context
// ─────────────────────────────────────────────────────────────────────────────

interface GPUCanvasConfiguration {
  device:     GPUDevice;
  format:     GPUTextureFormat;
  alphaMode?: "opaque" | "premultiplied";
  usage?:     number;
}

interface GPUCanvasContext {
  configure(configuration: GPUCanvasConfiguration): void;
  unconfigure(): void;
  getCurrentTexture(): GPUTexture;
}

// ─────────────────────────────────────────────────────────────────────────────
// navigator.gpu
// ─────────────────────────────────────────────────────────────────────────────

interface Navigator {
  readonly gpu: GPU;
}

interface HTMLCanvasElement {
  getContext(contextId: "webgpu"): GPUCanvasContext | null;
}
