/**
 * render-webgpu/index.ts
 *
 * Public API surface for the Phase 5 WebGPU tile renderer.
 *
 * Import from this barrel file rather than from individual modules:
 *
 *   import { TileRenderer, isWebGPUSupported } from "../render-webgpu";
 */

export { TileRenderer } from "./tile-renderer";
export { isWebGPUSupported, requestWebGPUDevice } from "./adapter";
export type { WebGPUHandle } from "./adapter";
export type { SnapResult }   from "./snap-pipeline";
export { TILE_SIZE_PX, SNAP_THRESHOLD_PX, MAX_SHAPES } from "./constants";
