/**
 * render-webgpu/adapter.ts
 *
 * WebGPU device initialisation and capability detection.
 *
 * Usage
 * ────────────────────────────────────────────────────────────────────────────
 *   const gpu = await requestWebGPUDevice();
 *   if (!gpu) {
 *     // Fall back to the Skia/WebGL renderer.
 *     return;
 *   }
 *   const { device, format } = gpu;
 */

export interface WebGPUHandle {
  adapter: GPUAdapter;
  device:  GPUDevice;
  /** Preferred texture format for the swap-chain / canvas context. */
  format:  GPUTextureFormat;
  /** True if the device exposes timestamp queries (used for perf profiling). */
  hasTimestamps: boolean;
}

/**
 * Request a WebGPU adapter and device.
 *
 * Returns `null` when:
 *  - The browser does not support WebGPU (`navigator.gpu` absent).
 *  - No suitable adapter is available.
 *  - Device request is rejected (driver error, permissions).
 *
 * The caller is responsible for falling back to the Skia/WebGL path.
 */
export async function requestWebGPUDevice(): Promise<WebGPUHandle | null> {
  if (typeof navigator === "undefined" || !("gpu" in navigator)) {
    console.info("[logos/webgpu] navigator.gpu not available — skipping WebGPU path.");
    return null;
  }

  let adapter: GPUAdapter | null;
  try {
    adapter = await navigator.gpu.requestAdapter({
      powerPreference: "high-performance",
    });
  } catch (err) {
    console.warn("[logos/webgpu] requestAdapter threw:", err);
    return null;
  }

  if (!adapter) {
    console.info("[logos/webgpu] No adapter found.");
    return null;
  }

  // Detect optional features.
  const hasTimestamps = adapter.features.has("timestamp-query");

  const requiredFeatures: GPUFeatureName[] = [];
  if (hasTimestamps) requiredFeatures.push("timestamp-query");

  let device: GPUDevice;
  try {
    device = await adapter.requestDevice({
      requiredFeatures,
      label: "logos-webgpu",
    });
  } catch (err) {
    console.warn("[logos/webgpu] requestDevice failed:", err);
    return null;
  }

  device.lost.then((info) => {
    console.error("[logos/webgpu] Device lost:", info.reason, info.message);
  });

  const format = navigator.gpu.getPreferredCanvasFormat();

  console.info(
    `[logos/webgpu] Device acquired. Format=${format} timestamps=${hasTimestamps}`,
    adapter.info ?? {}
  );

  return { adapter, device, format, hasTimestamps };
}

/**
 * Check if WebGPU is likely supported without requesting a device.
 * Useful for feature-flag UI (e.g. "Enable WebGPU rendering" checkbox).
 */
export function isWebGPUSupported(): boolean {
  return typeof navigator !== "undefined" && "gpu" in navigator;
}
