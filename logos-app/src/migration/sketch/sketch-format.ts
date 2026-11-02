/**
 * migration/sketch/sketch-format.ts
 *
 * TypeScript types for the Sketch file format (ZIP + JSON).
 *
 * A `.sketch` file is a ZIP archive with:
 *   document.json  — metadata, shared styles, page refs
 *   pages/<id>.json — one file per page, containing the full layer tree
 *   images/        — bitmap assets (not imported by this converter)
 *
 * These types cover the subset of the format needed for lossless migration.
 * See: https://developer.sketch.com/file-format/
 */

// ─── Primitives ───────────────────────────────────────────────────────────────

export interface SketchColor {
  _class: "color";
  red: number;
  green: number;
  blue: number;
  alpha: number;
}

export interface SketchGradientStop {
  _class: "gradientStop";
  color: SketchColor;
  position: number;
}

export interface SketchGradient {
  _class: "gradient";
  /** 0 = linear, 1 = radial, 2 = angular */
  gradientType: 0 | 1 | 2;
  from: string;   // "{x, y}" string, e.g. "{0.5, 0}"
  to: string;
  stops: SketchGradientStop[];
}

export interface SketchFill {
  _class: "fill";
  isEnabled: boolean;
  /**
   * 0 = solid color
   * 1 = gradient
   * 2 = pattern
   * 4 = image
   */
  fillType: 0 | 1 | 2 | 4;
  color: SketchColor;
  gradient: SketchGradient;
  opacity: number;
  contextSettings?: { opacity: number };
}

export interface SketchBorder {
  _class: "border";
  isEnabled: boolean;
  fillType: 0 | 1;
  color: SketchColor;
  position: 0 | 1 | 2;  // 0=center, 1=inside, 2=outside
  thickness: number;
}

export interface SketchShadow {
  _class: "shadow" | "innerShadow";
  isEnabled: boolean;
  color: SketchColor;
  offsetX: number;
  offsetY: number;
  blurRadius: number;
  spread: number;
}

export interface SketchBlur {
  _class: "blur";
  isEnabled: boolean;
  type: 0 | 1 | 2 | 3;  // 0=gaussian, 1=motion, 2=zoom, 3=background
  radius: number;
  motionAngle?: number;
}

// ─── Font / Text ──────────────────────────────────────────────────────────────

export interface SketchFontAttributes {
  name: string;   // e.g. "Inter-Bold"
  size: number;
}

/** Raw text style attributes inside an attributed string. */
export interface SketchTextAttributes {
  MSAttributedStringFontAttribute?: {
    _class: "fontDescriptor";
    attributes: SketchFontAttributes;
  };
  /** NSParagraphStyle: contains alignment */
  NSParagraphStyle?: {
    _archive: string;
  };
  /** Fill color for text */
  MSAttributedStringColorAttribute?: SketchColor;
  /** text decoration — 1=underline, 4=strikethrough */
  NSUnderline?: number;
  NSStrikethrough?: number;
  /** Paragraph spacing */
  paragraphSpacing?: number;
  /** Kerning / letter spacing */
  kerning?: number;
  /** Line height override */
  lineHeight?: number;
}

export interface SketchStringAttribute {
  _class: "stringAttribute";
  length: number;
  location: number;
  attributes: SketchTextAttributes;
}

export interface SketchAttributedString {
  _class: "attributedString";
  string: string;
  attributes: SketchStringAttribute[];
}

// Text alignment
export type SketchTextAlignment = 0 | 1 | 2 | 3;  // 0=left, 1=right, 2=center, 3=justified

export interface SketchTextStyle {
  _class: "textStyle";
  encodedAttributes: SketchTextAttributes;
  verticalAlignment?: 0 | 1 | 2;
}

// ─── Style ────────────────────────────────────────────────────────────────────

export interface SketchStyle {
  _class: "style";
  do_objectID?: string;
  fills: SketchFill[];
  borders: SketchBorder[];
  shadows: SketchShadow[];
  innerShadows?: SketchShadow[];
  blur?: SketchBlur;
  textStyle?: SketchTextStyle;
  contextSettings?: {
    _class: "graphicsContextSettings";
    blendMode: number;
    opacity: number;
  };
  startDecorationType?: number;
  endDecorationType?: number;
}

// ─── Frame ────────────────────────────────────────────────────────────────────

export interface SketchRect {
  _class: "rect";
  x: number;
  y: number;
  width: number;
  height: number;
}

// ─── Smart Layout ─────────────────────────────────────────────────────────────

export interface SketchSmartLayout {
  _class: string;
  /** 0 = horizontal (left-to-right), 1 = vertical (top-to-bottom) */
  axis?: number;
  /** 0 = none, non-zero = has layout */
  layoutAnchor?: number;
}

// ─── Layers ───────────────────────────────────────────────────────────────────

/** Fields common to every Sketch layer. */
export interface SketchLayerBase {
  _class: string;
  do_objectID: string;
  name: string;
  isVisible: boolean;
  isLocked: boolean;
  frame: SketchRect;
  style: SketchStyle;
  rotation: number;
  isFlippedHorizontal: boolean;
  isFlippedVertical: boolean;
  booleanOperation?: number;
  exportOptions?: unknown;
  /** Smart Layout on groups / artboards */
  groupLayout?: SketchSmartLayout;
  /** Legacy Smart Layout location */
  layout?: SketchSmartLayout;
}

export interface SketchGroupLayer extends SketchLayerBase {
  _class: "group" | "artboard" | "symbolMaster" | "page";
  layers: SketchLayer[];
  hasInheritedLayout?: boolean;
  /** For `artboard` — grid/column layout (old layoutGrid) */
  layoutGrid?: unknown;
  /** True if artboard clips its content */
  hasBackgroundColor?: boolean;
  backgroundColor?: SketchColor;
  // Smart Layout fields on newer artboards
  horizontalSpacing?: number;
  verticalSpacing?: number;
}

export interface SketchRectLayer extends SketchLayerBase {
  _class: "rectangle";
  fixedRadius: number;
  pointRadiusBehaviour?: number;
}

export interface SketchOvalLayer extends SketchLayerBase {
  _class: "oval";
}

export interface SketchTextLayer extends SketchLayerBase {
  _class: "text";
  attributedString: SketchAttributedString;
  textBehaviour: number;
  dontSynchroniseWithSymbol?: boolean;
  textStyle?: SketchTextStyle;
}

export interface SketchPathPoint {
  _class: "curvePoint";
  curveMode: number;
  curveFrom: string;  // "{x,y}"
  curveTo: string;
  hasCurveFrom: boolean;
  hasCurveTo: boolean;
  point: string;      // "{x,y}" normalized 0-1 within shape bounds
}

export interface SketchShapePathLayer extends SketchLayerBase {
  _class: "shapePath" | "shapeGroup" | "star" | "polygon" | "triangle";
  points?: SketchPathPoint[];
  layers?: SketchLayer[];  // sub-layers for shapeGroup (boolean ops)
  isClosed?: boolean;
}

export interface SketchSymbolInstanceLayer extends SketchLayerBase {
  _class: "symbolInstance";
  symbolID: string;
  overrideValues: Array<{
    _class: "overrideValue";
    overrideName: string;
    value: string | unknown;
  }>;
  scale: number;
}

export interface SketchSliceLayer extends SketchLayerBase {
  _class: "slice";
}

export interface SketchBitmapLayer extends SketchLayerBase {
  _class: "bitmap";
  image: {
    _class: "MSJSONFileReference" | "MSJSONOriginalDataReference";
    _ref: string;
  };
}

export type SketchLayer =
  | SketchGroupLayer
  | SketchRectLayer
  | SketchOvalLayer
  | SketchTextLayer
  | SketchShapePathLayer
  | SketchSymbolInstanceLayer
  | SketchSliceLayer
  | SketchBitmapLayer
  | (SketchLayerBase & { _class: string });

// ─── Shared styles ────────────────────────────────────────────────────────────

export interface SketchSharedStyle {
  _class: "sharedStyle";
  do_objectID: string;
  name: string;
  value: SketchStyle;
}

export interface SketchSharedStyleContainer {
  _class: "sharedStyleContainer";
  objects: SketchSharedStyle[];
}

// ─── Document ─────────────────────────────────────────────────────────────────

export interface SketchDocumentJson {
  _class: "document";
  do_objectID: string;
  /** Shared layer styles */
  layerStyles: SketchSharedStyleContainer;
  /** Shared text styles */
  layerTextStyles: SketchSharedStyleContainer;
  /** Page refs — each entry has `_ref: "pages/<uuid>"` */
  pages: Array<{ _class: string; _ref: string }>;
  /** Color variables (Sketch >= 69) */
  sharedSwatches?: {
    _class: "swatchContainer";
    objects: Array<{
      _class: "swatch";
      do_objectID: string;
      name: string;
      value: SketchColor;
    }>;
  };
  /** Asset library colors */
  assets?: {
    _class: "assetCollection";
    colors?: Array<{
      _class: "MSImmutableColorAsset";
      name: string;
      color: SketchColor;
    }>;
    colorAssets?: Array<{
      _class: string;
      name: string;
      color: SketchColor;
    }>;
  };
}

export interface SketchPageJson {
  _class: "page";
  do_objectID: string;
  name: string;
  layers: SketchLayer[];
}

// ─── Guards ───────────────────────────────────────────────────────────────────

export function isSketchDocumentJson(data: unknown): data is SketchDocumentJson {
  if (typeof data !== "object" || data === null) return false;
  const d = data as Record<string, unknown>;
  return d["_class"] === "document" && Array.isArray(d["pages"]);
}

export function isSketchPageJson(data: unknown): data is SketchPageJson {
  if (typeof data !== "object" || data === null) return false;
  const d = data as Record<string, unknown>;
  return d["_class"] === "page" && Array.isArray(d["layers"]);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Convert a Sketch normalized RGBA color to a hex string "#rrggbb" or "#rrggbbaa". */
export function sketchColorToHex(c: SketchColor): string {
  const toHex = (n: number) => Math.round(Math.max(0, Math.min(1, n)) * 255).toString(16).padStart(2, "0");
  const hex = "#" + toHex(c.red) + toHex(c.green) + toHex(c.blue);
  return c.alpha < 1 ? hex + toHex(c.alpha) : hex;
}

/**
 * Parse a Sketch point string like "{0.5, 0.25}" → { x: 0.5, y: 0.25 }.
 * Returns { x: 0, y: 0 } if the string is malformed.
 */
export function parseSketchPoint(s: string): { x: number; y: number } {
  const m = s.match(/\{([\d.eE+-]+),\s*([\d.eE+-]+)\}/);
  if (!m) return { x: 0, y: 0 };
  return { x: parseFloat(m[1]), y: parseFloat(m[2]) };
}
