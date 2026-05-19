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

// P5.1 — compositing + compute layout
export { CompositePipeline } from "./composite-pipeline";
export { LayoutPipeline }    from "./layout-pipeline";
export type { LayoutBoundsResult, LayoutPadding } from "./layout-pipeline";

// P5.2 — text rendering + gradient fills
export { GlyphAtlas }    from "./glyph-atlas";
export { GradientAtlas } from "./gradient-atlas";
export { TextPipeline }  from "./text-pipeline";
export type { GlyphUVRect, GlyphLayout } from "./glyph-atlas";

// P5.4 — GPU-accelerated flex layout
export { FlexLayoutPipeline } from "./flex-layout-pipeline";
export type {
  FlexChildShape,
  FlexContainerParams,
  ChildFinalPosition,
  FlexDirection,
  FlexWrap,
  FlexAlign,
  FlexSizingMode,
  FlexAlignSelf,
} from "./flex-layout-pipeline";

export {
  TILE_SIZE_PX,
  SNAP_THRESHOLD_PX,
  MAX_SHAPES,
  MAX_TILE_CACHE,
  GRADIENT_ATLAS_W,
  GRADIENT_ATLAS_H,
  MAX_GRADIENTS,
  GLYPH_ATLAS_SIZE,
  MAX_GLYPHS_PER_FRAME,
  FLEX_UNIFORM_BYTES,
  FLEX_CHILD_INPUT_BYTES,
  FLEX_CHILD_DATA_BYTES,
  FLEX_LINE_DATA_BYTES,
  MAX_FLEX_CHILDREN,
  MAX_FLEX_LINES,
} from "./constants";
