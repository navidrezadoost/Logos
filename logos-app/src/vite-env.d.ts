/// <reference types="vite/client" />

// Injected by vite.config.ts → define
declare const __RENDER_WASM_JS__: string;
declare const __RENDER_WASM_WASM__: string;
declare const __LOGOS_LAYOUT_WASM__: string;

// WGSL shader imports via Vite's `?raw` suffix.
declare module "*.wgsl?raw" {
  const source: string;
  export default source;
}

// Minimal WebGPU ambient declarations.
// Replace with `/// <reference types="@webgpu/types" />` once the package is installed.
// See logos-app/src/render-webgpu/webgpu-types.d.ts for the stub source.

