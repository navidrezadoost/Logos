/**
 * migration/xd/xd-shape-converter.ts
 *
 * Phase IM4 — Converts Adobe XD nodes into flat Logos Shape records.
 *
 * XD node type → Logos ShapeType:
 *   artboard           → "frame"
 *   group / RepeatGrid → "group"
 *   shape/rect         → "rect"
 *   shape/ellipse      → "circle"
 *   shape/path / shape/compound / shape/line / shape/polygon → "path"
 *   BooleanGroup       → "path"   (boolean compound, C1 extension)
 *   text               → "text"
 *   symbolInstance     → "instance"
 *   (symbol master)    → "component"
 *
 * Coordinate system:
 *   XD stores node positions in the `transform` matrix (tx, ty).
 *   Bounds dimensions come from `shape.width`/`shape.height` (for rects/artboards)
 *   or are derived from the transform for groups.
 *
 * Style mapping:
 *   fill.type "solid"    → SolidFill
 *   fill.type "gradient" → GradientFill (linear / radial)
 */

import type {
  XdGraphicContent,
  XdNode,
  XdArtboard,
  XdGroup,
  XdShape,
  XdText,
  XdSymbolInstance,
  XdBooleanGroup,
  XdFill,
  XdTransform,
  XdStyle,
  XdCharacterAttributes,
} from "./xd-format";

import { xdColorToHex, xdFontStyleToWeight, xdTransformToLogos } from "./xd-format";

import type {
  Shape,
  ShapeType,
  Fill,
  SolidFill,
  GradientFill,
  Transform,
  InstanceMeta,
} from "../../types/shapes";

import { IDENTITY_TRANSFORM } from "../../types/shapes";

// ─── Result ───────────────────────────────────────────────────────────────────

export interface XdShapeConversionResult {
  shapes: Shape[];
  /** Top-level roots per artboard (XD doesn't have explicit pages). */
  artboardRoots: { artboardId: string; artboardName: string; rootShapeIds: string[] }[];
  /** Maps XD symbolId → Logos component Shape id. */
  symbolIdMap: Map<string, string>;
  warnings: string[];
}

// ─── Public API ──────────────────────────────────────────────────────────────

export function convertXdContent(content: XdGraphicContent): XdShapeConversionResult {
  const ctx: ConvertCtx = {
    shapes: [],
    idMap: new Map(),
    symbolIdMap: new Map(),
    warnings: [],
  };

  const artboardRoots: XdShapeConversionResult["artboardRoots"] = [];

  // ── Symbol masters first — build symbolIdMap before converting artboards ──
  for (const sym of content.symbols ?? []) {
    const shape = convertNode(sym, null, ctx);
    if (shape) {
      const symbolId = (sym as { meta?: { ux?: { symbolId?: string } } }).meta?.ux?.symbolId ?? sym.id;
      ctx.symbolIdMap.set(symbolId, shape.id);
      // Mark as component so the shape type is correct
      shape.type = "component";
      shape.componentMeta = { properties: {} };
    }
  }

  // ── Top-level children (artboards + free-floating groups) ─────────────────
  for (const child of content.children ?? []) {
    const shape = convertNode(child, null, ctx);
    if (!shape) continue;

    if (child.type === "artboard") {
      artboardRoots.push({
        artboardId: child.id,
        artboardName: child.name ?? child.id,
        rootShapeIds: [shape.id],
      });
    }
  }

  // If no artboards found, treat all top-level shapes as one implicit artboard
  if (artboardRoots.length === 0 && ctx.shapes.length > 0) {
    artboardRoots.push({
      artboardId: "xd-root",
      artboardName: "Document",
      rootShapeIds: ctx.shapes.filter((s) => s.parentId === null).map((s) => s.id),
    });
  }

  return {
    shapes: ctx.shapes,
    artboardRoots,
    symbolIdMap: ctx.symbolIdMap,
    warnings: ctx.warnings,
  };
}

// ─── Internal ─────────────────────────────────────────────────────────────────

interface ConvertCtx {
  shapes: Shape[];
  idMap: Map<string, string>;
  symbolIdMap: Map<string, string>;
  warnings: string[];
}

function remapId(xdId: string, ctx: ConvertCtx): string {
  let id = ctx.idMap.get(xdId);
  if (!id) {
    id = crypto.randomUUID();
    ctx.idMap.set(xdId, id);
  }
  return id;
}

function convertNode(node: XdNode, parentId: string | null, ctx: ConvertCtx): Shape | null {
  const type = xdTypeToLogos(node);
  if (!type) {
    if (node.type !== "slice" && node.type !== "component") {
      ctx.warnings.push(`Skipping unsupported node type: ${node.type} (${node.name ?? node.id})`);
    }
    return null;
  }

  const id = remapId(node.id, ctx);

  // ── Children ──────────────────────────────────────────────────────────────
  const childIds: string[] = [];
  const withChildren = node as { children?: XdNode[] };
  for (const child of withChildren.children ?? []) {
    const childShape = convertNode(child, id, ctx);
    if (childShape) childIds.push(childShape.id);
  }

  // ── Bounds & transform ────────────────────────────────────────────────────
  const { bounds, transform } = extractBoundsAndTransform(node, childIds, ctx);
  const style = getStyle(node);

  // ── Fills ─────────────────────────────────────────────────────────────────
  const fills = convertFill(style?.fill ?? null);

  // ── Shape ─────────────────────────────────────────────────────────────────
  const shape: Shape = {
    id,
    type,
    name: node.name ?? node.id,
    bounds,
    transform,
    rotation: extractRotation(node.transform),
    fills,
    opacity: node.opacity ?? style?.opacity ?? 1,
    hidden: !(node.visible ?? true),
    locked: node.locked ?? false,
    parentId,
    children: childIds,
  };

  // ── Text ──────────────────────────────────────────────────────────────────
  if (type === "text") {
    applyText(shape, node as XdText, fills);
  }

  // ── Symbol instance ───────────────────────────────────────────────────────
  if (type === "instance") {
    applyInstance(shape, node as XdSymbolInstance, ctx);
  }

  ctx.shapes.push(shape);
  return shape;
}

// ─── Type mapping ─────────────────────────────────────────────────────────────

function xdTypeToLogos(node: XdNode): ShapeType | null {
  switch (node.type) {
    case "artboard":
      return "frame";

    case "group":
      return "group";

    case "BooleanGroup":
      return "path";

    case "shape": {
      const s = (node as XdShape).shape;
      if (!s) return "path";
      switch (s.type) {
        case "rect":      return "rect";
        case "ellipse":   return "circle";
        case "compound":
        case "path":
        case "line":
        case "polygon":
        default:          return "path";
      }
    }

    case "text":
      return "text";

    case "symbolInstance":
      return "instance";

    case "slice":
    default:
      return null;
  }
}

// ─── Bounds & transform ───────────────────────────────────────────────────────

function extractBoundsAndTransform(
  node: XdNode,
  _childIds: string[],
  _ctx: ConvertCtx
): { bounds: Shape["bounds"]; transform: Transform } {
  const t = node.transform;
  const tx = t?.tx ?? 0;
  const ty = t?.ty ?? 0;
  let w = 0, h = 0;

  if (node.type === "artboard") {
    const ab = node as XdArtboard;
    w = ab.width;
    h = ab.height;
  } else if (node.type === "shape") {
    const sh = (node as XdShape).shape;
    w = sh?.width ?? 0;
    h = sh?.height ?? 0;
    // For ellipses, rx/ry define the size
    if (sh?.type === "ellipse" && sh.rx && sh.ry) {
      w = sh.rx * 2;
      h = sh.ry * 2;
    }
  }
  // Groups/text/instances get zero bounds; the compositor uses child bounds.

  const transform: Transform = t ? xdTransformToLogos(t) : IDENTITY_TRANSFORM;

  return { bounds: { x: tx, y: ty, w, h }, transform };
}

/**
 * Extract rotation in degrees from the affine matrix.
 * For [a,b,c,d,tx,ty], rotation = atan2(b, a) in degrees.
 */
function extractRotation(t: XdTransform | undefined): number {
  if (!t) return 0;
  return (Math.atan2(t.b, t.a) * 180) / Math.PI;
}

// ─── Text mapping ─────────────────────────────────────────────────────────────

function applyText(shape: Shape, node: XdText, fills: Fill[]): void {
  // Collect raw text from all paragraph lines
  const rawLines: string[] = [];
  let firstAttrs: XdCharacterAttributes | undefined;

  for (const para of node.text?.paragraphs ?? []) {
    for (const line of para.lines ?? []) {
      const lineText = line.map((run) => run.content ?? "").join("");
      rawLines.push(lineText);
      if (!firstAttrs && line[0]?.characterAttributes) {
        firstAttrs = line[0].characterAttributes;
      }
    }
  }

  shape.text = rawLines.join("\n") || node.text?.rawText || "";

  // Font from character attributes or node style
  const styleFont = getStyle(node)?.font;
  const family = firstAttrs?.fontFamily ?? styleFont?.family ?? "Inter";
  const styleName = firstAttrs?.fontStyle ?? styleFont?.style ?? "Regular";

  shape.fontFamily = family;
  shape.fontWeight = xdFontStyleToWeight(styleName);
  shape.fontSize   = firstAttrs?.fontSize ?? styleFont?.size ?? 14;

  // Text color — from fill on the char attrs or from the first solid shape fill
  const charFill = firstAttrs?.fill;
  if (charFill?.type === "solid" && charFill.color) {
    try { shape.textColor = xdColorToHex(charFill.color); } catch { /* ignore */ }
  }
  if (!shape.textColor) {
    const solid = fills.find((f) => f.type === "solid") as SolidFill | undefined;
    shape.textColor = solid?.color ?? "#000000";
  }

  // Alignment from first paragraph
  const align = node.text?.paragraphs?.[0] as { align?: string } | undefined;
  if (align?.align) {
    shape.textAlign = mapXdAlign(String(align.align));
  }

  // Decoration
  if (firstAttrs?.underline) shape.textDecoration = "underline";
  else if (firstAttrs?.strikethrough) shape.textDecoration = "line-through";

  // Line spacing
  if (firstAttrs?.lineSpacing && firstAttrs.lineSpacing > 0) {
    shape.lineHeight = firstAttrs.lineSpacing;
  }

  // Letter spacing (XD stores as em * 1000, convert to px)
  if (typeof firstAttrs?.letterSpacing === "number") {
    shape.letterSpacing = (firstAttrs.letterSpacing / 1000) * shape.fontSize;
  }
}

function mapXdAlign(align: string): Shape["textAlign"] {
  switch (align.toLowerCase()) {
    case "left":    return "left";
    case "center":  return "center";
    case "right":   return "right";
    case "justify": return "justify";
    default:        return "left";
  }
}

// ─── Instance mapping ─────────────────────────────────────────────────────────

function applyInstance(shape: Shape, node: XdSymbolInstance, ctx: ConvertCtx): void {
  const symbolId = node.symbolId ?? node.meta?.ux?.symbolId ?? "";
  const componentId = ctx.symbolIdMap.get(symbolId) ?? symbolId;

  const instanceMeta: InstanceMeta = {
    componentId,
    variantProperties: {},
    overrides: {},
  };
  shape.instanceMeta = instanceMeta;
}

// ─── Fill / Gradient ─────────────────────────────────────────────────────────

function convertFill(fill: XdFill | null | undefined): Fill[] {
  if (!fill || fill.type === "none") {
    return [{ type: "solid", color: "#e8eaee", opacity: 0.3 }];
  }

  if (fill.type === "solid" && fill.color) {
    try {
      return [{ type: "solid", color: xdColorToHex(fill.color), opacity: fill.color.a ?? 1 } satisfies SolidFill];
    } catch {
      return [{ type: "solid", color: "#e8eaee", opacity: 0.3 }];
    }
  }

  if (fill.type === "gradient" && fill.gradient) {
    const g = fill.gradient;
    if (!g.stops || g.stops.length < 2) {
      return [{ type: "solid", color: "#e8eaee", opacity: 0.3 }];
    }

    const stops = g.stops.map((s) => ({
      color: xdColorToHex(s.color),
      position: s.offset,
      opacity: s.color.a ?? 1,
    }));

    const gf: GradientFill = {
      type: "gradient",
      opacity: 1,
      atlasSlot: -1,
      gradient:
        g.type === "radial"
          ? {
              type: "radial",
              startX: g.cx ?? 0.5, startY: g.cy ?? 0.5,
              endX: (g.cx ?? 0.5) + (g.r ?? 0.5), endY: g.cy ?? 0.5,
              width: g.r ?? 0.5,
              stops,
            }
          : {
              type: "linear",
              startX: g.x1 ?? 0, startY: g.y1 ?? 0,
              endX:   g.x2 ?? 1, endY:   g.y2 ?? 0,
              width: 1,
              stops,
            },
    };
    return [gf];
  }

  return [{ type: "solid", color: "#e8eaee", opacity: 0.3 }];
}

// ─── Style accessor ───────────────────────────────────────────────────────────

function getStyle(node: XdNode): XdStyle | undefined {
  return (node as { style?: XdStyle }).style;
}
