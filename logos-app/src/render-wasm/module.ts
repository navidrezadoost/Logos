/**
 * render-wasm/module.ts
 *
 * TypeScript port of the Emscripten module loader from:
 *   frontend/src/app/render_wasm/wasm.cljs        (module lifecycle)
 *   frontend/src/app/render_wasm/api.cljs          (init sequence)
 *
 * Protocol:
 *   1. Dynamic-import the ES6 Emscripten bundle (render-wasm.js).
 *   2. Call createRustSkiaModule({locateFile}) → resolves to the Module.
 *   3. Register a WebGL2 context via Module.GL.
 *   4. Call Module._init(width, height).
 *   5. Draw shapes, call Module._render_sync().
 */

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/** Subset of the Emscripten Module we actually use. */
export interface RenderWasmModule {
  GL: {
    registerContext(ctx: WebGL2RenderingContext, opts: { majorVersion: 2 }): number;
    makeContextCurrent(handle: number): void;
    deleteContext(handle: number): void;
  };
  HEAPU8: Uint8Array;
  HEAP32: Int32Array;
  HEAPU32: Uint32Array;
  HEAPF32: Float32Array;
  /** Allocate `size` bytes in the WASM heap; returns a byte offset into HEAPU8. */
  _alloc_bytes(size: number): number;
  /** Free the last allocation (must pair with _alloc_bytes). */
  _free_bytes(): void;
  // Lifecycle
  _init(width: number, height: number): void;
  _clean_up(): void;
  _set_render_options(debug: number, dpr: number): void;
  _set_browser(browser: number): void;
  // Viewport
  _resize_viewbox(width: number, height: number): void;
  _set_canvas_background(color: number): void;
  _set_view(zoom: number, x: number, y: number): void;
  // Shapes pool
  _init_shapes_pool(total: number): void;

  /**
   * Batched shape base-props — reads 104 bytes from the WASM heap
   * (previously written via _alloc_bytes + HEAPU8 writes) and applies
   * them to the selected shape.
   *
   * Layout: matches render-wasm/src/wasm/shapes/base_props.rs BASE_PROPS_SIZE=104
   *   [0..16)  id UUID (4×u32 LE)
   *   [16..32) parent_id UUID (4×u32 LE)
   *   [32]     shape_type u8
   *   [33]     flags u8  (bit0=clip, bit1=hidden)
   *   [34]     blend_mode u8
   *   [35]     constraint_h u8  (0xFF = None)
   *   [36]     constraint_v u8  (0xFF = None)
   *   [37..40) padding
   *   [40..44) opacity f32 LE
   *   [44..48) rotation f32 LE (degrees)
   *   [48..72) transform 6×f32 LE (a,b,c,d,e,f)
   *   [72..88) selrect 4×f32 LE (x1,y1,x2,y2)
   *   [88..104) corners 4×f32 LE (r1,r2,r3,r4)
   */
  _set_shape_base_props(): void;

  // Legacy individual shape-property setters (still exported, used as fallback)
  /** Select the active shape by its UUID (four u32 words). */
  _use_shape(a: number, b: number, c: number, d: number): void;
  _set_shape_type(type: number): void;
  /** Selrect: x1, y1, x2, y2 */
  _set_shape_selrect(x1: number, y1: number, x2: number, y2: number): void;
  _set_shape_transform(a: number, b: number, c: number, d: number, e: number, f: number): void;
  _set_shape_rotation(r: number): void;
  _set_shape_clip_content(clip: boolean): void;
  _set_shape_opacity(opacity: number): void;
  _set_shape_hidden(hidden: boolean): void;
  _set_shape_corners(r1: number, r2: number, r3: number, r4: number): void;
  _set_parent(a: number, b: number, c: number, d: number): void;
  /** Write fill data from the current alloc buffer into the active shape. */
  _set_shape_fills(): void;
  // Render
  _render_sync(): void;
  _render(timestamp: number): void;
}

// Shape type constants (from render-wasm/src/wasm/fills/shared.js)
export const SHAPE_TYPE = {
  frame:   0,
  group:   1,
  bool:    2,
  rect:    3,
  path:    4,
  text:    5,
  circle:  6,
  svgRaw:  7,
} as const;

// Fill type discriminants (from RawFillData repr(C, u8, align(4)))
export const FILL_TYPE = {
  solid:   0x00,
  linear:  0x01,
  radial:  0x02,
  image:   0x03,
} as const;

// The size of one RawFillData in bytes:
//   1 (discriminant u8) + 3 (padding to align 4) + 4 (largest payload: color u32) = 8
export const RAW_FILL_DATA_SIZE = 8;

// ─────────────────────────────────────────────────────────────────────────────
// Module singleton
// ─────────────────────────────────────────────────────────────────────────────

let _module: RenderWasmModule | null = null;
let _glHandle: number | undefined;
let _loadPromise: Promise<RenderWasmModule | null> | null = null;

export function getModule(): RenderWasmModule | null {
  return _module;
}

/**
 * Dynamically load the Emscripten render-wasm bundle.
 *
 * The Emscripten output file is compiled with:
 *   -sEXPORT_NAME=createRustSkiaModule
 *   -sMODULARIZE=1
 *   -sEXPORT_ES6=1
 *
 * So the default export is a factory function: `(opts) => Promise<Module>`.
 *
 * @param jsUrl    URL of the Emscripten JS glue (typically `/js/render-wasm.js`)
 * @param wasmUrl  URL of the companion .wasm binary
 */
export async function loadRenderWasm(
  jsUrl: string,
  wasmUrl: string
): Promise<RenderWasmModule | null> {
  if (_module) return _module;
  if (_loadPromise) return _loadPromise;

  _loadPromise = (async () => {
    try {
      // Probe whether the Emscripten artefact exists before attempting to
      // load it.  We use XMLHttpRequest (HEAD) rather than fetch() because
      // Vite's dev-server middleware intercepts fetch() requests that map to
      // its publicDir and throws a build-time error even for runtime calls.
      const wasmExists = await new Promise<boolean>((resolve) => {
        const xhr = new XMLHttpRequest();
        xhr.open("HEAD", jsUrl, /* async= */ true);
        xhr.onload = () => resolve(xhr.status >= 200 && xhr.status < 300);
        xhr.onerror = () => resolve(false);
        xhr.send();
      });

      if (!wasmExists) {
        console.warn(
          "[logos-app] render-wasm not available — Canvas 2D fallback active.\n" +
          "  To enable: build render-wasm with EMSDK and copy\n" +
          "  render-wasm.{js,wasm} to frontend/resources/public/js/"
        );
        return null;
      }

      // Load the Emscripten JS bundle as raw text, then create a blob: URL
      // so the dynamic import goes through the browser's native module loader
      // and completely bypasses Vite's transform pipeline.
      const xhr2 = new XMLHttpRequest();
      const jsText = await new Promise<string | null>((resolve) => {
        xhr2.open("GET", jsUrl, true);
        xhr2.onload = () => xhr2.status === 200 ? resolve(xhr2.responseText) : resolve(null);
        xhr2.onerror = () => resolve(null);
        xhr2.send();
      });
      if (!jsText) return null;

      const blob = new Blob([jsText], { type: "text/javascript" });
      const blobUrl = URL.createObjectURL(blob);

      // Dynamic-import the blob URL — Vite never intercepts blob: imports.
      let createRustSkiaModule: ((opts?: object) => Promise<RenderWasmModule>) | undefined;
      try {
        ({ default: createRustSkiaModule } = await import(/* @vite-ignore */ blobUrl) as { default: typeof createRustSkiaModule });
      } finally {
        URL.revokeObjectURL(blobUrl);
      }

      if (typeof createRustSkiaModule !== "function") {
        console.warn("[logos-app] render-wasm bundle has no default export — Canvas 2D fallback active.");
        return null;
      }

      // Boot the module; `locateFile` tells Emscripten where the .wasm binary is.
      const mod: RenderWasmModule = await createRustSkiaModule({
        locateFile: (_filename: string) => wasmUrl,
      });

      _module = mod;
      return mod;
    } catch (err) {
      console.warn(
        "[logos-app] render-wasm not available — Canvas 2D fallback active.\n" +
        "  To enable: build render-wasm with EMSDK and copy artefacts to\n" +
        "  frontend/resources/public/js/render-wasm.{js,wasm}\n",
        err
      );
      return null;
    }
  })();

  return _loadPromise;
}

// ─────────────────────────────────────────────────────────────────────────────
// WebGL context setup  (mirrors init-canvas-context in api.cljs)
// ─────────────────────────────────────────────────────────────────────────────

const DEFAULT_CONTEXT_OPTIONS: WebGLContextAttributes = {
  alpha: true,
  antialias: false,
  depth: false,
  stencil: true,
  premultipliedAlpha: false,
  preserveDrawingBuffer: true,
  powerPreference: "high-performance",
  desynchronized: true,
};

/**
 * Initialise WebGL2 on `canvas` and call Module._init.
 * Must be called after `loadRenderWasm()` succeeds.
 *
 * @returns true if WebGL2 context was obtained.
 */
export function initCanvasContext(
  mod: RenderWasmModule,
  canvas: HTMLCanvasElement,
  dpr: number = 1
): boolean {
  const ctx = canvas.getContext("webgl2", DEFAULT_CONTEXT_OPTIONS);
  if (!ctx) {
    console.error("[logos-app] WebGL2 context unavailable.");
    return false;
  }

  // Register the context with Emscripten's GL subsystem
  const handle = mod.GL.registerContext(ctx, { majorVersion: 2 });
  mod.GL.makeContextCurrent(handle);
  _glHandle = handle;

  // Force WEBGL_debug_renderer_info (Emscripten requires it)
  ctx.getExtension("WEBGL_debug_renderer_info");

  const w = canvas.width;
  const h = canvas.height;

  // Init the Skia surface
  mod._init(w / dpr, h / dpr);
  mod._set_render_options(0, dpr);
  // 0 = browser unknown (safe default)
  mod._set_browser(0);

  return true;
}

/**
 * Clean up the WebGL context resources.
 */
export function cleanUp(mod: RenderWasmModule, canvas: HTMLCanvasElement): void {
  mod._clean_up();
  if (_glHandle !== undefined) {
    try {
      const ctx = canvas.getContext("webgl2");
      if (ctx) {
        const loseExt = ctx.getExtension("WEBGL_lose_context");
        loseExt?.loseContext();
      }
    } finally {
      mod.GL.deleteContext(_glHandle);
      _glHandle = undefined;
    }
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fill helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Write a solid fill buffer into the WASM heap and apply it to the current shape.
 *
 * Buffer layout (from render-wasm/src/wasm/fills.rs):
 *
 *   Offset 0  : u8  — num_fills
 *   Offset 1-3: --- padding
 *   For each fill (RAW_FILL_DATA_SIZE = 8 bytes):
 *     Offset 0  : u8  — discriminant (0 = solid, 1 = linear, …)
 *     Offset 1-3: --- padding (align 4)
 *     Offset 4-7: u32 — color (ARGB, little-endian)
 *
 * @param mod   The loaded Emscripten module.
 * @param color 32-bit ARGB colour, e.g. 0xFF0000FF for opaque blue.
 */
export function applySolidFill(mod: RenderWasmModule, color: number): void {
  const NUM_FILLS = 1;
  const HEADER_SIZE = 4; // num_fills u8 + 3 padding bytes
  const totalBytes = HEADER_SIZE + NUM_FILLS * RAW_FILL_DATA_SIZE; // 4 + 8 = 12

  const ptr = mod._alloc_bytes(totalBytes);
  const heap = mod.HEAPU8;

  // Header
  heap[ptr + 0] = NUM_FILLS; // num_fills
  heap[ptr + 1] = 0;
  heap[ptr + 2] = 0;
  heap[ptr + 3] = 0;

  // Fill 0 — Solid (discriminant = 0x00)
  heap[ptr + 4] = FILL_TYPE.solid; // discriminant
  heap[ptr + 5] = 0;               // padding
  heap[ptr + 6] = 0;               // padding
  heap[ptr + 7] = 0;               // padding

  // Color as u32 little-endian (ARGB)
  const colorView = new DataView(heap.buffer, ptr + 8, 4);
  colorView.setUint32(0, color, true /* LE */);

  mod._set_shape_fills();
  mod._free_bytes();
}

// ─────────────────────────────────────────────────────────────────────────────
// Convenience: draw one hardcoded rectangle
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Draw a single hardcoded blue rectangle via the Rust/Skia engine.
 *
 * UUID: [0, 0, 0, 1] (arbitrary but stable)
 * Rect: x=50, y=50, w=200, h=100  →  selrect x1=50, y1=50, x2=250, y2=150
 * Fill: opaque blue  #0000ff  → ARGB 0xFF0000FF
 */
export function drawHardcodedRect(mod: RenderWasmModule): void {
  // One shape in the pool
  mod._init_shapes_pool(1);

  // Identity transform
  mod._use_shape(0, 0, 0, 1);
  mod._set_shape_type(SHAPE_TYPE.rect);
  mod._set_shape_selrect(50, 50, 250, 150);          // x1 y1 x2 y2
  mod._set_shape_rotation(0);
  mod._set_shape_transform(1, 0, 0, 1, 0, 0);        // identity matrix
  mod._set_shape_clip_content(false);

  // Opaque blue: ARGB = 0xFF_00_00_FF
  applySolidFill(mod, 0xff0000ff);

  mod._render_sync();
}
