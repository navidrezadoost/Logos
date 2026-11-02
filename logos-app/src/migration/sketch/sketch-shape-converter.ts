/**
 * migration/sketch/sketch-shape-converter.ts
 *
 * Converts a Sketch page layer tree into flat Logos Shape records.
 *
 * Layer class → Logos ShapeType:
 *   artboard       → "frame"  (with Smart Layout if present)
 *   group          → "group"  (with Smart Layout if present)
 *   rectangle      → "rect"
 *   oval           → "circle"
 *   text           → "text"
 *   shapePath      → "path"
 *   shapeGroup     → "path"   (boolean compound path)
 *   star / polygon → "path"
 *   symbolMaster   → "component"
 *   symbolInstance → "instance"
 *   slice / bitmap → skipped
 *
 * Coordinate system:
 *   Sketch frame x/y are relative to the parent layer's origin — same as Logos.
 *
 * Fill mapping:
 *   fillType 0 (solid)    → SolidFill
 *   fillType 1 (gradient) → GradientFill (linear / radial)
 *   others                → skipped
 */

import type {
  SketchLayer,
  SketchPageJson,
  SketchStyle,
  SketchFill,
  SketchGradient,
  SketchColor,
  SketchSmartLayout,
} from "./sketch-format";

import { sketchColorToHex, parseSketchPoint } from "./sketch-format";

import type {
  Shape,
  ShapeType,
  Fill,
  SolidFill,
  GradientFill,
  Transform,
  ComponentMeta,
  InstanceMeta,
  LayoutType,
  FlexDirection,
} from "../../types/shapes";

import { IDENTITY_TRANSFORM } from "../../types/shapes";

// ─── Result ───────────────────────────────────────────────────────────────────

export interface SketchShapeConversionResult {
  shapes: Shape[];
  pageRoots: { pageId: string; pageName: string; rootShapeIds: string[] }[];
  /** Maps Sketch symbolID → Logos component Shape id (populated during conversion). */
  symbolIdMap: Map<string, string>;
  warnings: string[];
}

// ─── Public API ──────────────────────────────────────────────────────────────

export function convertSketchPages(pages: SketchPageJson[]): SketchShapeConversionResult {
  const ctx: ConvertCtx = {
    shapes: [],
    idMap: new Map(),
    symbolIdMap: new Map(),
    warnings: [],
  };

  const pageRoots: SketchShapeConversionResult["pageRoots"] = [];

  for (const page of pages) {
    const rootIds: string[] = [];
    for (const layer of page.layers) {
      const shape = convertLayer(layer, null, ctx);
      if (shape) rootIds.push(shape.id);
    }
    pageRoots.push({
      pageId: remapId(page.do_objectID, ctx),
      pageName: page.name,
      rootShapeIds: rootIds,
    });
  }

  return {
    shapes: ctx.shapes,
    pageRoots,
    symbolIdMap: ctx.symbolIdMap,
    warnings: ctx.warnings,
  };
}

// ─── Internal ─────────────────────────────────────────────────────────────────

interface ConvertCtx {
  shapes: Shape[];
  idMap: Map<string, string>;
  /** Sketch symbolID → Logos Shape id (component master) */
  symbolIdMap: Map<string, string>;
  warnings: string[];
}

function remapId(sketchId: string, ctx: ConvertCtx): string {
  let id = ctx.idMap.get(sketchId);
  if (!id) {
    id = crypto.randomUUID();
    ctx.idMap.set(sketchId, id);
  }
  return id;
}

function convertLayer(
  layer: SketchLayer,
  parentId: string | null,
  ctx: ConvertCtx
): Shape | null {
  const type = sketchClassToLogos(layer._class);
  if (!type) {
    // Skip slices, bitmaps, unknown
    if (layer._class !== "slice" && layer._class !== "bitmap") {
      ctx.warnings.push(`Skipping unsupported layer class: ${layer._class} (${layer.name})`);
    }
    return null;
  }

  const id = remapId(layer.do_objectID, ctx);

  // ── Children (depth-first) ────────────────────────────────────────────────
  const childIds: string[] = [];
  const groupLayer = layer as { layers?: SketchLayer[] };
  for (const child of groupLayer.layers ?? []) {
    const childShape = convertLayer(child, id, ctx);
    if (childShape) childIds.push(childShape.id);
  }

  // ── Fills ─────────────────────────────────────────────────────────────────
  const fills = convertStyleFills(layer.style, ctx);

  // ── Frame / bounds ────────────────────────────────────────────────────────
  const f = layer.frame;

  // ── Shape ─────────────────────────────────────────────────────────────────
  const shape: Shape = {
    id,
    type,
    name: layer.name,
    bounds: { x: f.x, y: f.y, w: f.width, h: f.height },
    transform: buildTransform(layer),
    rotation: layer.rotation ?? 0,
    fills,
    opacity: layer.style?.contextSettings?.opacity ?? 1,
    hidden: !(layer.isVisible ?? true),
    locked: layer.isLocked ?? false,
    parentId,
    children: childIds,
  };

  // ── Layout (Smart Layout on artboards/groups) ─────────────────────────────
  const smartLayout = (layer as { groupLayout?: SketchSmartLayout; layout?: SketchSmartLayout }).groupLayout
    ?? (layer as { layout?: SketchSmartLayout }).layout;
  if (smartLayout && shouldApplyLayout(smartLayout)) {
    applySmartLayout(shape, smartLayout);
  }

  // ── Text ──────────────────────────────────────────────────────────────────
  if (type === "text") {
    applyText(shape, layer, fills);
  }

  // ── Symbol master (component) ─────────────────────────────────────────────
  if (type === "component") {
    const sym = layer as { symbolID?: string };
    if (sym.symbolID) ctx.symbolIdMap.set(sym.symbolID, id);
    shape.componentMeta = { properties: {} };
  }

  // ── Symbol instance ───────────────────────────────────────────────────────
  if (type === "instance") {
    applyInstance(shape, layer, ctx);
  }

  ctx.shapes.push(shape);
  return shape;
}

// ─── Layer class → Logos type ─────────────────────────────────────────────────

function sketchClassToLogos(cls: string): ShapeType | null {
  switch (cls) {
    case "artboard":
    case "page":       return "frame";
    case "group":      return "group";
    case "rectangle":  return "rect";
    case "oval":       return "circle";
    case "text":       return "text";
    case "shapePath":
    case "shapeGroup":
    case "star":
    case "polygon":
    case "triangle":   return "path";
    case "symbolMaster": return "component";
    case "symbolInstance": return "instance";
    case "slice":
    case "bitmap":
    default:           return null;
  }
}

// ─── Smart Layout → Logos flex ────────────────────────────────────────────────
//
// Sketch Smart Layout axis values:
//   0 = horizontal (left to right)
//   1 = right to left
//   2 = vertical (top to bottom)
//   3 = bottom to top
//   4 = horizontal (centered) — treat as row
//   5 = vertical (centered)   — treat as column

function shouldApplyLayout(layout: SketchSmartLayout): boolean {
  return typeof layout.axis === "number" || typeof (layout as { layoutType?: unknown }).layoutType === "number";
}

function applySmartLayout(shape: Shape, layout: SketchSmartLayout): void {
  const axis = layout.axis
    ?? (layout as { layoutType?: number }).layoutType
    ?? 0;

  shape.layout = "flex" satisfies LayoutType;

  const isVertical = axis === 2 || axis === 3 || axis === 5;
  shape.layoutFlexDir = (isVertical ? "column" : "row") satisfies FlexDirection;

  // Sketch Smart Layout doesn't expose gap/padding at the group level (those come
  // from child constraints). Leave layoutGap/layoutPadding absent — the Logos
  // flex engine uses zero defaults.
  shape.layoutWrapType = "nowrap";
  shape.layoutJustifyContent = "start";
  shape.layoutAlignItems = "start";
}

// ─── Text conversion ─────────────────────────────────────────────────────────

function applyText(shape: Shape, layer: SketchLayer, fills: Fill[]): void {
  const textLayer = layer as {
    attributedString?: { string?: string; attributes?: Array<{ attributes?: Record<string, unknown> }> };
    textStyle?: { encodedAttributes?: Record<string, unknown> };
    style?: { textStyle?: { encodedAttributes?: Record<string, unknown> } };
  };

  shape.text = textLayer.attributedString?.string ?? "";

  // Get attributes from the first run, or from the text style
  const firstRun = textLayer.attributedString?.attributes?.[0]?.attributes ?? {};
  const styleAttrs = (textLayer.textStyle ?? textLayer.style?.textStyle)?.encodedAttributes ?? {};
  const attrs = Object.keys(firstRun).length > 0 ? firstRun : styleAttrs;

  // Font
  const fontAttr = (attrs["MSAttributedStringFontAttribute"] as {
    attributes?: { name?: string; size?: number };
  } | undefined)?.attributes;

  if (fontAttr?.name) {
    const { family, weight } = parseFontName(fontAttr.name);
    shape.fontFamily = family;
    shape.fontWeight = weight;
  } else {
    shape.fontFamily = "Inter";
    shape.fontWeight = 400;
  }
  shape.fontSize = fontAttr?.size ?? 14;

  // Text color — from fill attribute or first solid fill
  const colorAttr = attrs["MSAttributedStringColorAttribute"] as SketchColor | undefined;
  if (colorAttr?.red !== undefined) {
    shape.textColor = sketchColorToHex(colorAttr);
  } else {
    const solid = fills.find((f) => f.type === "solid") as SolidFill | undefined;
    shape.textColor = solid?.color ?? "#000000";
  }

  // Text alignment from NSParagraphStyle — Sketch stores this as a binary archive;
  // try to read the raw int if it was serialized as a plain number instead.
  const alignAttr = attrs["NSParagraphStyle"] as { alignment?: number } | undefined;
  if (typeof alignAttr?.alignment === "number") {
    shape.textAlign = mapSketchAlign(alignAttr.alignment);
  }

  // Decoration
  const underline = attrs["NSUnderline"];
  const strike = attrs["NSStrikethrough"];
  if (underline && Number(underline) > 0) shape.textDecoration = "underline";
  else if (strike && Number(strike) > 0)  shape.textDecoration = "line-through";

  // Line height & letter spacing
  const lineHeight = (attrs["lineHeight"] ?? attrs["MSAttributedStringLineHeightMultiplierAttribute"]) as number | undefined;
  if (typeof lineHeight === "number" && lineHeight > 0) {
    shape.lineHeight = lineHeight;
  }
  const kerning = attrs["kerning"] as number | undefined;
  if (typeof kerning === "number") {
    shape.letterSpacing = kerning;
  }
}

function mapSketchAlign(align: number): Shape["textAlign"] {
  switch (align) {
    case 0: return "left";
    case 1: return "right";
    case 2: return "center";
    case 3: return "justify";
    default: return "left";
  }
}

// ─── Instance conversion ──────────────────────────────────────────────────────

function applyInstance(shape: Shape, layer: SketchLayer, ctx: ConvertCtx): void {
  const inst = layer as {
    symbolID?: string;
    overrideValues?: Array<{ overrideName?: string; value?: unknown }>;
  };

  const symbolId = inst.symbolID ?? "";
  const componentId = ctx.symbolIdMap.get(symbolId) ?? symbolId;

  const overrides: Record<string, unknown> = {};
  for (const ov of inst.overrideValues ?? []) {
    if (ov.overrideName) overrides[ov.overrideName] = ov.value;
  }

  const instanceMeta: InstanceMeta = {
    componentId,
    variantProperties: {},
    overrides,
  };
  shape.instanceMeta = instanceMeta;
}

// ─── Fill / Style conversion ──────────────────────────────────────────────────

function convertStyleFills(style: SketchStyle | undefined, ctx: ConvertCtx): Fill[] {
  if (!style) return [{ type: "solid", color: "#e8eaee", opacity: 0.3 }];

  const result: Fill[] = [];

  for (const fill of style.fills ?? []) {
    if (!fill.isEnabled) continue;

    if (fill.fillType === 0 && fill.color) {
      // Solid
      const opacity = fill.contextSettings?.opacity ?? fill.opacity ?? 1;
      result.push({
        type: "solid",
        color: sketchColorToHex(fill.color),
        opacity,
      } satisfies SolidFill);
    } else if (fill.fillType === 1 && fill.gradient) {
      // Gradient
      const gf = convertGradient(fill, ctx);
      if (gf) result.push(gf);
    }
  }

  if (result.length === 0) {
    result.push({ type: "solid", color: "#e8eaee", opacity: 0.3 });
  }

  return result;
}

function convertGradient(fill: SketchFill, _ctx: ConvertCtx): GradientFill | null {
  const g: SketchGradient = fill.gradient;
  if (!g || !g.stops || g.stops.length < 2) return null;

  const stops = g.stops.map((s) => ({
    color: sketchColorToHex(s.color),
    position: s.position,
    opacity: s.color.alpha,
  }));

  const from = parseSketchPoint(g.from);
  const to   = parseSketchPoint(g.to);

  const gf: GradientFill = {
    type: "gradient",
    opacity: fill.contextSettings?.opacity ?? fill.opacity ?? 1,
    atlasSlot: -1,
    gradient:
      g.gradientType === 0  // linear
        ? {
            type: "linear",
            startX: from.x, startY: from.y,
            endX: to.x,     endY: to.y,
            width: 1,
            stops,
          }
        : {
            // radial (type=1) or angular (type=2)
            type: "radial",
            startX: from.x, startY: from.y,
            endX: to.x,     endY: to.y,
            width: 0.5,
            stops,
          },
  };

  return gf;
}

// ─── Transform ────────────────────────────────────────────────────────────────

function buildTransform(layer: SketchLayer): Transform {
  const deg = layer.rotation ?? 0;
  const flipH = layer.isFlippedHorizontal ?? false;
  const flipV = layer.isFlippedVertical ?? false;

  if (deg === 0 && !flipH && !flipV) return IDENTITY_TRANSFORM;

  // Sketch: rotation is counter-clockwise, Logos: clockwise positive — negate angle.
  const r = (-deg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);

  let a = cos, b = sin, c = -sin, d = cos;

  if (flipH) { a = -a; c = -c; }
  if (flipV) { b = -b; d = -d; }

  return [a, b, c, d, 0, 0];
}

// ─── Misc helpers ─────────────────────────────────────────────────────────────

function parseFontName(psName: string): { family: string; weight: number } {
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
  const parts = psName.split("-");
  const family = parts[0].replace(/([A-Z])/g, " $1").trim();
  const suffix = parts.slice(1).join(" ");
  for (const [re, w] of WEIGHT_MAP) {
    if (re.test(suffix)) return { family, weight: w };
  }
  return { family, weight: 400 };
}

// Re-export ComponentMeta/InstanceMeta for the orchestrator
export type { Shape };
