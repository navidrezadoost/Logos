/**
 * migration/xd/xd-format.ts
 *
 * TypeScript types for the Adobe XD file format (OPC ZIP + JSON).
 *
 * An `.xd` file is a ZIP archive (Open Packaging Conventions) with:
 *   manifest.json                  — version + resource list
 *   resources/graphic/graphicContent.agx  — shapes, artboards, groups (JSON)
 *   resources/                     — colors, character styles, symbols
 *   interactions/interactions.json — prototype links
 *
 * These types cover the subset needed for lossless migration of:
 *   artboards, shapes, groups, text, symbols, color resources, char styles.
 *
 * Reference: https://github.com/nicehash/xd-parser (community docs)
 *            https://helpx.adobe.com/xd/developerportal/
 */

// ─── Primitives ───────────────────────────────────────────────────────────────

/** XD colors are stored as RGBA objects, all channels 0–1. */
export interface XdColor {
  r: number;
  g: number;
  b: number;
  a?: number;
  mode?: "RGB" | "HSL" | "HSB";
}

export interface XdGradientStop {
  offset: number;
  color: XdColor;
}

export interface XdGradient {
  /** "linear" | "radial" */
  type: string;
  x1?: number; y1?: number;
  x2?: number; y2?: number;
  cx?: number; cy?: number;
  r?: number;
  stops: XdGradientStop[];
}

// ─── Fill / Style ─────────────────────────────────────────────────────────────

export type XdFillType = "solid" | "gradient" | "image" | "none";

export interface XdFill {
  type: XdFillType;
  color?: XdColor;
  gradient?: XdGradient;
}

export interface XdStroke {
  type: "solid" | "none";
  color?: XdColor;
  width?: number;
  align?: "inside" | "outside" | "center";
  dash?: number[];
  cap?: "butt" | "round" | "square";
  join?: "miter" | "round" | "bevel";
}

export interface XdShadow {
  enabled?: boolean;
  color: XdColor;
  x: number;
  y: number;
  blur: number;
  spread?: number;
}

export interface XdBlur {
  /** "object" | "background" */
  type: string;
  visible?: boolean;
  amount?: number;
  brightness?: number;
}

// ─── Transform ────────────────────────────────────────────────────────────────

/**
 * XD transform — a 3×2 affine matrix stored as a flat array [a,b,c,d,e,f].
 * Identical layout to the CSS matrix() function.
 */
export interface XdTransform {
  a: number; b: number;
  c: number; d: number;
  tx: number; ty: number;
}

// ─── Typography ───────────────────────────────────────────────────────────────

export type XdTextAlign = "left" | "center" | "right" | "justify";

export interface XdCharacterAttributes {
  fontFamily?: string;
  fontStyle?: string;    // "Bold", "Regular", "Italic", etc.
  fontSize?: number;
  fill?: XdFill;
  underline?: boolean;
  lineSpacing?: number;
  letterSpacing?: number;
  textTransform?: "none" | "uppercase" | "lowercase";
  strikethrough?: boolean;
}

export interface XdParagraph {
  align?: XdTextAlign;
  content?: string;
}

// ─── Layout ──────────────────────────────────────────────────────────────────

/** Responsive Layout constraints */
export interface XdConstraints {
  horizontal?: "none" | "left" | "right" | "leftRight" | "center" | "scale";
  vertical?: "none" | "top" | "bottom" | "topBottom" | "center" | "scale";
}

/** Repeat Grid metadata */
export interface XdRepeatGrid {
  rows?: number;
  columns?: number;
  horizontalSpacing?: number;
  verticalSpacing?: number;
}

// ─── Node / Shape ─────────────────────────────────────────────────────────────

/**
 * Fields present on every XD node.
 * XD uses the term "node" for any document element.
 */
export interface XdNodeBase {
  id: string;
  name?: string;
  type: string;   // "artboard" | "group" | "shape" | "text" | "symbolInstance" | ...
  visible?: boolean;
  locked?: boolean;
  opacity?: number;
  blendMode?: string;
  transform?: XdTransform;
  /** Responsive layout constraints on this node. */
  meta?: {
    ux?: {
      constraints?: XdConstraints;
      repeatGrid?: XdRepeatGrid;
      componentId?: string;    // present on symbol instances
      isMaster?: boolean;      // true on symbol masters
      symbolId?: string;       // canonical symbol ID for both masters + instances
    };
  };
}

/** Artboard = Logos Frame */
export interface XdArtboard extends XdNodeBase {
  type: "artboard";
  width: number;
  height: number;
  fill?: XdFill;
  children?: XdNode[];
}

/** Generic group */
export interface XdGroup extends XdNodeBase {
  type: "group";
  children?: XdNode[];
  /** RepeatGrid root */
  repeatGrid?: XdRepeatGrid;
}

/** Boolean group (union / subtract / intersect / exclude) */
export interface XdBooleanGroup extends XdNodeBase {
  type: "BooleanGroup";
  /** "add" | "subtract" | "intersect" | "exclude" */
  shapeOperation?: "add" | "subtract" | "intersect" | "exclude";
  children?: XdNode[];
  style?: XdStyle;
}

/** Shape node (rectangle, ellipse, polygon, path, line) */
export interface XdShape extends XdNodeBase {
  type: "shape";
  /** "rect" | "ellipse" | "polygon" | "path" | "line" | "compound" */
  shape?: {
    type: string;
    x?: number; y?: number;
    width?: number; height?: number;
    r?: number[];              // corner radii [tl, tr, br, bl]
    path?: string;             // SVG path data for path/compound
    points?: Array<{ x: number; y: number }>;  // polygon vertices
    cx?: number; cy?: number;  // ellipse center
    rx?: number; ry?: number;  // ellipse radii
  };
  style?: XdStyle;
}

/** Text node */
export interface XdText extends XdNodeBase {
  type: "text";
  text?: {
    rawText?: string;
    paragraphs?: Array<{
      lines?: Array<Array<{
        content?: string;
        characterAttributes?: XdCharacterAttributes;
      }>>;
    }>;
  };
  style?: XdStyle;
}

/** Symbol instance (component instance) */
export interface XdSymbolInstance extends XdNodeBase {
  type: "symbolInstance";
  /** The symbol master's canonical ID (same as master's meta.ux.symbolId) */
  symbolId?: string;
  children?: XdNode[];
  style?: XdStyle;
}

/** Style block shared across all node types that support fills */
export interface XdStyle {
  fill?: XdFill;
  stroke?: XdStroke;
  shadow?: XdShadow[];
  blur?: XdBlur;
  opacity?: number;
  font?: {
    family?: string;
    style?: string;
    size?: number;
  };
}

export type XdNode =
  | XdArtboard
  | XdGroup
  | XdBooleanGroup
  | XdShape
  | XdText
  | XdSymbolInstance
  | (XdNodeBase & { children?: XdNode[]; style?: XdStyle });

// ─── Resources ────────────────────────────────────────────────────────────────

export interface XdColorResource {
  /** Mode: "none" | "RGB" | "HSL" */
  mode?: string;
  value?: XdColor;
  /** Named color asset — metadata lives here */
  meta?: { ux?: { localId?: string; name?: string; colorSpace?: string } };
}

export interface XdCharacterStyleResource {
  style?: {
    font?: { family?: string; style?: string; size?: number };
    fill?: XdFill;
    textAttributes?: XdCharacterAttributes;
    para?: { align?: XdTextAlign };
  };
  meta?: { ux?: { localId?: string; name?: string } };
}

export interface XdBrushResource {
  type?: string;
  color?: XdColor;
  gradient?: XdGradient;
  meta?: { ux?: { localId?: string; name?: string } };
}

/**
 * Top-level content of `resources/graphic/graphicContent.agx` (parsed from JSON).
 * Despite the `.agx` extension, the file content is JSON.
 */
export interface XdGraphicContent {
  /** Root artboards and other top-level elements */
  children?: XdNode[];
  /** Symbol master definitions (keyed by symbol UUID) */
  symbols?: XdNode[];
  resources?: {
    /** Named color swatches */
    colors?: XdColorResource[];
    /** Character styles */
    characterStyles?: XdCharacterStyleResource[];
    /** Gradient + color brushes */
    brushes?: XdBrushResource[];
    /** Embedded images (ref → metadata) */
    images?: Record<string, unknown>;
  };
}

/**
 * Content of `manifest.json` at the root of the XD archive.
 */
export interface XdManifest {
  id?: string;
  name?: string;
  version?: string;
  children?: Array<{
    id: string;
    name?: string;
    path?: string;
    type?: string;
  }>;
  components?: XdNode[];
}

// ─── Interactions ─────────────────────────────────────────────────────────────

export interface XdInteraction {
  srcNodeId?: string;
  trigger?: { type?: string };
  action?: {
    type?: string;
    destination?: string;
    scroll?: { type?: string };
  };
}

export interface XdInteractionsJson {
  interactions?: XdInteraction[];
}

// ─── Guards ───────────────────────────────────────────────────────────────────

export function isXdGraphicContent(data: unknown): data is XdGraphicContent {
  if (typeof data !== "object" || data === null) return false;
  const d = data as Record<string, unknown>;
  return Array.isArray(d["children"]) || Array.isArray(d["symbols"]) || typeof d["resources"] === "object";
}

export function isXdManifest(data: unknown): data is XdManifest {
  if (typeof data !== "object" || data === null) return false;
  const d = data as Record<string, unknown>;
  return typeof d["id"] === "string" || typeof d["name"] === "string" || Array.isArray(d["children"]);
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/** Convert an XD RGBA color object to "#rrggbb" or "#rrggbbaa". */
export function xdColorToHex(c: XdColor): string {
  const toHex = (n: number) =>
    Math.round(Math.max(0, Math.min(255, n * 255))).toString(16).padStart(2, "0");
  const hex = "#" + toHex(c.r) + toHex(c.g) + toHex(c.b);
  const a = c.a ?? 1;
  return a < 1 ? hex + toHex(a) : hex;
}

/** Parse an XD transform (3×2 affine) into a 6-element Logos Transform. */
export function xdTransformToLogos(
  t: XdTransform
): [number, number, number, number, number, number] {
  return [t.a, t.b, t.c, t.d, t.tx, t.ty];
}

/** Map XD font style string to a CSS font-weight number. */
export function xdFontStyleToWeight(style: string): number {
  const WEIGHT_MAP: [RegExp, number][] = [
    [/thin|hairline/i, 100],
    [/extralight|ultralight/i, 200],
    [/light/i, 300],
    [/medium/i, 500],
    [/semibold|demibold|demi/i, 600],
    [/extrabold|ultrabold/i, 800],
    [/bold/i, 700],
    [/black|heavy/i, 900],
  ];
  for (const [re, w] of WEIGHT_MAP) {
    if (re.test(style)) return w;
  }
  return 400; // Regular
}
