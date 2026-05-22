/**
 * migration/figma/figma-shape-converter.ts
 *
 * Phase IM2 Complete — Converts FigmaExportNode trees into flat arrays of
 * Logos Shape records suitable for insertion into documentStore.
 *
 * Mapping:
 *   FRAME / SECTION          → "frame"      (with auto-layout if present)
 *   GROUP                    → "group"
 *   RECTANGLE                → "rect"
 *   ELLIPSE / CIRCLE         → "circle"
 *   TEXT                     → "text"       (with full typography)
 *   VECTOR (simple path)     → "path"
 *   VECTOR (complex network) → "vector-network"  ← C3
 *   LINE / POLYGON / STAR    → "path"
 *   BOOLEAN_OPERATION        → "path"
 *   COMPONENT / COMPONENT_SET → "component"
 *   INSTANCE                 → "instance"
 *
 * Coordinate system:
 *   Figma x/y are relative to the parent's origin for child nodes, and
 *   absolute canvas-space for top-level nodes — same as Logos convention.
 */

import type {
  FigmaExportNode,
  FigmaExportPage,
  FigmaExportPaint,
} from "./figma-plugin-format";

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
  WrapType,
  JustifyContent,
  AlignItems,
} from "../../types/shapes";

import { IDENTITY_TRANSFORM } from "../../types/shapes";

// ─── Result ───────────────────────────────────────────────────────────────────

export interface ShapeConversionResult {
  /**
   * Flat array of all converted shapes, in depth-first order.
   * Insert all of these into documentStore.shapes.
   */
  shapes: Shape[];
  /**
   * Top-level shape IDs for each page (same order as pages[]).
   * Use these as the rootShapeIds when creating Logos pages.
   */
  pageRoots: { pageId: string; pageName: string; rootShapeIds: string[] }[];
  warnings: string[];
}

// ─── Public API ──────────────────────────────────────────────────────────────

/**
 * Convert an array of Figma pages (from the plugin export) to flat Logos shapes.
 *
 * @param pages   The `pages` array from LogosFigmaExport.
 * @param idRemap Optional existing id→id remap if you need stable IDs.
 */
export function convertFigmaPages(
  pages: FigmaExportPage[],
  idRemap?: Map<string, string>
): ShapeConversionResult {
  const ctx: ConvertCtx = {
    shapes: [],
    idMap: idRemap ?? new Map<string, string>(),
    warnings: [],
  };

  const pageRoots: ShapeConversionResult["pageRoots"] = [];

  for (const page of pages) {
    const rootShapeIds: string[] = [];
    for (const node of page.children) {
      const shape = convertNode(node, null, ctx);
      if (shape) rootShapeIds.push(shape.id);
    }
    pageRoots.push({
      pageId: remapId(page.id, ctx),
      pageName: page.name,
      rootShapeIds,
    });
  }

  return { shapes: ctx.shapes, pageRoots, warnings: ctx.warnings };
}

// ─── Internal ─────────────────────────────────────────────────────────────────

interface ConvertCtx {
  shapes: Shape[];
  /** Maps Figma node ID → new Logos UUID */
  idMap: Map<string, string>;
  warnings: string[];
}

function remapId(figmaId: string, ctx: ConvertCtx): string {
  let newId = ctx.idMap.get(figmaId);
  if (!newId) {
    newId = crypto.randomUUID();
    ctx.idMap.set(figmaId, newId);
  }
  return newId;
}

function convertNode(
  node: FigmaExportNode,
  parentId: string | null,
  ctx: ConvertCtx
): Shape | null {
  // ── Determine type — vector network detection (C3) ────────────────────────
  const type = figmaTypeToLogos(node);
  if (!type) {
    ctx.warnings.push(`Skipping unsupported node type: ${node.type} (${node.name})`);
    return null;
  }

  const id = remapId(node.id, ctx);

  // ── Children first (depth-first) ─────────────────────────────────────────
  const childIds: string[] = [];
  for (const child of node.children ?? []) {
    const childShape = convertNode(child, id, ctx);
    if (childShape) childIds.push(childShape.id);
  }

  // ── Fills ─────────────────────────────────────────────────────────────────
  const fills = convertFills(node.fills ?? []);

  // ── Base shape ────────────────────────────────────────────────────────────
  const shape: Shape = {
    id,
    type,
    name: node.name,
    bounds: { x: node.x, y: node.y, w: node.width, h: node.height },
    transform: rotationToTransform(node.rotation ?? 0),
    rotation: node.rotation ?? 0,
    fills,
    opacity: node.opacity ?? 1,
    hidden: !(node.visible ?? true),
    locked: node.locked ?? false,
    parentId,
    children: childIds,
  };

  // ── Auto-layout (C1) ──────────────────────────────────────────────────────
  if (node.layout) {
    applyLayout(shape, node.layout);
  }

  // ── Text / Typography (C2) ────────────────────────────────────────────────
  if (type === "text") {
    applyText(shape, node, fills);
  }

  // ── Vector network geometry (C3) ─────────────────────────────────────────
  if (type === "vector-network" && node.vectorNetwork) {
    applyVectorNetwork(shape, node.vectorNetwork);
  }

  // ── Component metadata ────────────────────────────────────────────────────
  if ((type === "component") && node.propertyDefinitions) {
    const meta: ComponentMeta = { properties: {} };
    for (const [key, def] of Object.entries(node.propertyDefinitions)) {
      if (def.type === "VARIANT") {
        meta.properties[key] = {
          kind: "variant",
          name: key,
          values: def.variantOptions ?? [],
          defaultValue: def.defaultValue,
        };
      } else if (def.type === "BOOLEAN") {
        meta.properties[key] = { kind: "boolean", name: key, defaultValue: def.defaultValue };
      } else if (def.type === "TEXT") {
        meta.properties[key] = { kind: "text", name: key, defaultValue: def.defaultValue };
      }
    }
    shape.componentMeta = meta;
  }

  // ── Instance metadata ─────────────────────────────────────────────────────
  if (type === "instance") {
    const mainId = node.mainComponentId
      ? (ctx.idMap.get(node.mainComponentId) ?? node.mainComponentId)
      : "";
    const instanceMeta: InstanceMeta = {
      componentId: mainId,
      variantProperties: node.componentProperties ?? {},
      overrides: {},
    };
    shape.instanceMeta = instanceMeta;
  }

  ctx.shapes.push(shape);
  return shape;
}

// ─── C1: Auto-layout mapping ───────────────────────────────────────────────────
//
// Figma:                          Logos (CanonicalShape / Shape):
//   layoutMode: "HORIZONTAL"   →  layout: "flex", layoutFlexDir: "row"
//   layoutMode: "VERTICAL"     →  layout: "flex", layoutFlexDir: "column"
//   primaryAxisAlignItems       →  layoutJustifyContent
//   counterAxisAlignItems       →  layoutAlignItems
//   layoutWrap                  →  layoutWrapType
//   gap                         →  layoutGap.rowGap / columnGap
//   padding*                    →  layoutPadding { p1(top), p2(right), p3(bottom), p4(left) }

type FigmaLayout = NonNullable<FigmaExportNode["layout"]>;

function applyLayout(shape: Shape, layout: FigmaLayout): void {
  shape.layout = "flex" satisfies LayoutType;

  shape.layoutFlexDir = layout.mode === "VERTICAL"
    ? ("column" satisfies FlexDirection)
    : ("row" satisfies FlexDirection);

  // Wrap
  shape.layoutWrapType = layout.layoutWrap === "WRAP"
    ? ("wrap" satisfies WrapType)
    : ("nowrap" satisfies WrapType);

  // Main-axis alignment (primaryAxisAlignItems)
  shape.layoutJustifyContent = mapPrimaryAlign(layout.primaryAxisAlignItems);

  // Cross-axis alignment (counterAxisAlignItems)
  shape.layoutAlignItems = mapCounterAlign(layout.counterAxisAlignItems);

  // Gap — for column layouts, figma "gap" is rowGap; for row it's columnGap
  const gap = layout.gap ?? 0;
  shape.layoutGap = layout.mode === "VERTICAL"
    ? { rowGap: gap, columnGap: layout.counterAxisSpacing ?? 0 }
    : { rowGap: layout.counterAxisSpacing ?? 0, columnGap: gap };

  // Padding: p1=top, p2=right, p3=bottom, p4=left  (Logos convention)
  shape.layoutPadding = {
    p1: layout.paddingTop,
    p2: layout.paddingRight,
    p3: layout.paddingBottom,
    p4: layout.paddingLeft,
  };
}

function mapPrimaryAlign(figmaAlign: string): JustifyContent {
  switch (figmaAlign) {
    case "MIN":           return "start";
    case "CENTER":        return "center";
    case "MAX":           return "end";
    case "SPACE_BETWEEN": return "space-between";
    default:              return "start";
  }
}

function mapCounterAlign(figmaAlign: string): AlignItems {
  switch (figmaAlign) {
    case "MIN":    return "start";
    case "CENTER": return "center";
    case "MAX":    return "end";
    case "BASELINE":
    default:       return "stretch";
  }
}

// ─── C2: Text / Typography mapping ────────────────────────────────────────────
//
// Figma:                          Logos:
//   characters                 →  text
//   fontName.family            →  fontFamily
//   fontWeight                 →  fontWeight
//   fontSize                   →  fontSize
//   fills[0] (SOLID)           →  textColor
//   textAlignHorizontal        →  textAlign  ("LEFT" → "left", "JUSTIFIED" → "justify")
//   textDecoration             →  textDecoration ("UNDERLINE" → "underline", "STRIKETHROUGH" → "line-through")
//   lineHeight { unit, value } →  lineHeight (px only; PERCENT treated as line-height × fontSize)
//   letterSpacing { unit, val} →  letterSpacing (px only; PERCENT = val/100 * fontSize, approx)

function applyText(shape: Shape, node: FigmaExportNode, fills: Fill[]): void {
  shape.text = node.text ?? "";
  shape.fontFamily = node.fontFamily ?? "Inter";
  shape.fontWeight = node.fontWeight ?? 400;
  shape.fontSize = node.fontSize ?? 14;

  // Text color from first solid fill
  const firstSolid = fills.find((f) => f.type === "solid") as SolidFill | undefined;
  shape.textColor = firstSolid?.color ?? "#000000";

  // Horizontal alignment
  if (node.textAlign) {
    shape.textAlign = mapTextAlign(node.textAlign);
  }

  // Text decoration
  if (node.textDecoration) {
    shape.textDecoration = mapTextDecoration(node.textDecoration);
  }

  // Line height — Figma exports as { unit: "PIXELS" | "PERCENT" | "AUTO", value? }
  const lh = node.lineHeight as { unit?: string; value?: number } | undefined;
  if (lh && lh.unit !== "AUTO" && typeof lh.value === "number") {
    if (lh.unit === "PIXELS") {
      shape.lineHeight = lh.value;
    } else if (lh.unit === "PERCENT" && shape.fontSize) {
      shape.lineHeight = (lh.value / 100) * shape.fontSize;
    }
  }

  // Letter spacing — Figma exports as { unit: "PIXELS" | "PERCENT", value }
  const ls = node.letterSpacing as { unit?: string; value?: number } | undefined;
  if (ls && typeof ls.value === "number") {
    if (ls.unit === "PIXELS") {
      shape.letterSpacing = ls.value;
    } else if (ls.unit === "PERCENT" && shape.fontSize) {
      shape.letterSpacing = (ls.value / 100) * shape.fontSize;
    }
  }
}

function mapTextAlign(figmaAlign: string): Shape["textAlign"] {
  switch (figmaAlign.toUpperCase()) {
    case "LEFT":      return "left";
    case "CENTER":    return "center";
    case "RIGHT":     return "right";
    case "JUSTIFIED": return "justify";
    default:          return "left";
  }
}

function mapTextDecoration(figmaDeco: string): Shape["textDecoration"] {
  switch (figmaDeco.toUpperCase()) {
    case "UNDERLINE":    return "underline";
    case "STRIKETHROUGH": return "line-through";
    default:             return "none";
  }
}

// ─── C3: Vector network geometry ──────────────────────────────────────────────
//
// Figma VN vertices carry absolute coordinates (node-local space).
// Figma VN segments reference vertex indices + tangent points (absolute).
// Logos VNAnchor:  { x, y, hi?: [dx,dy], ho?: [dx,dy] } — handles RELATIVE to anchor
// Logos VNSegment: { s, e, c1?: [x,y], c2?: [x,y] }    — control pts ABSOLUTE
//
// Mapping:
//   vertices[i] → VNAnchor { x, y }  (hi/ho from adjacent tangents, simplified)
//   segments[j] → VNSegment { s: start, e: end, c1: tangentStart, c2: tangentEnd }
//   regions[k]  → VNRegion (flat loop of segment indices, first loop only)

type FigmaVN = NonNullable<FigmaExportNode["vectorNetwork"]>;

function applyVectorNetwork(shape: Shape, vn: FigmaVN): void {
  shape.vnAnchors = vn.vertices.map((v) => ({ x: v.x, y: v.y }));

  shape.vnSegments = vn.segments.map((s) => ({
    s: s.start,
    e: s.end,
    c1: s.tangentStart ? [s.tangentStart.x, s.tangentStart.y] as [number, number] : undefined,
    c2: s.tangentEnd   ? [s.tangentEnd.x,   s.tangentEnd.y  ] as [number, number] : undefined,
  }));

  if (vn.regions && vn.regions.length > 0) {
    // Each Figma region has one or more loops; Logos VNRegion = one ordered
    // list of segment indices.  Emit one VNRegion per region, using the first loop.
    shape.vnRegions = vn.regions.map((r) => r.loops[0] ?? []);
  }
}

// ─── Type mapping ─────────────────────────────────────────────────────────────

/**
 * C3: If the VECTOR node has a complex vector network (multi-vertex, multi-segment)
 * it maps to "vector-network".  Simple paths (0 or 1 segments, tree-topology)
 * map to "path" so Logos renders them without the VN engine overhead.
 */
function figmaTypeToLogos(node: FigmaExportNode): ShapeType | null {
  switch (node.type) {
    case "FRAME":
    case "SECTION":
      return "frame";
    case "GROUP":
      return "group";
    case "RECTANGLE":
      return "rect";
    case "ELLIPSE":
    case "CIRCLE":
      return "circle";
    case "TEXT":
      return "text";
    case "VECTOR": {
      const vn = node.vectorNetwork;
      // Complex network: more than one segment, or segments form a non-tree topology
      if (vn && (vn.segments.length > 1 || (vn.regions && vn.regions.length > 0))) {
        return "vector-network";
      }
      return "path";
    }
    case "LINE":
    case "POLYGON":
    case "STAR":
    case "BOOLEAN_OPERATION":
      return "path";
    case "COMPONENT":
    case "COMPONENT_SET":
      return "component";
    case "INSTANCE":
      return "instance";
    default:
      return null;
  }
}

// ─── Fill conversion ──────────────────────────────────────────────────────────

function convertFills(paints: FigmaExportPaint[]): Fill[] {
  const result: Fill[] = [];
  for (const p of paints) {
    if (!(p.visible ?? true)) continue;

    if (p.type === "SOLID" && p.color) {
      result.push({ type: "solid", color: p.color, opacity: p.opacity ?? 1 } satisfies SolidFill);
      continue;
    }

    if (
      (p.type === "GRADIENT_LINEAR" || p.type === "GRADIENT_RADIAL") &&
      Array.isArray(p.stops) &&
      p.stops.length >= 2
    ) {
      const stops = p.stops.map((s) => ({ color: s.color, position: s.position, opacity: 1 }));
      const gradientFill: GradientFill = {
        type: "gradient",
        opacity: p.opacity ?? 1,
        atlasSlot: -1,
        gradient:
          p.type === "GRADIENT_LINEAR"
            ? { type: "linear", startX: 0, startY: 0, endX: 1, endY: 0, width: 1, stops }
            : { type: "radial", startX: 0.5, startY: 0.5, endX: 0.5, endY: 0.5, width: 0.5, stops },
      };
      result.push(gradientFill);
    }
  }
  // Transparent placeholder for shapes with no visible fills (e.g. pure containers)
  if (result.length === 0) {
    result.push({ type: "solid", color: "#e8eaee", opacity: 0.3 } satisfies SolidFill);
  }
  return result;
}

// ─── Rotation → Transform matrix ─────────────────────────────────────────────

/** Convert a rotation in degrees to a 2D affine transform matrix [a,b,c,d,e,f]. */
function rotationToTransform(deg: number): Transform {
  if (deg === 0) return IDENTITY_TRANSFORM;
  const r = (deg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return [cos, sin, -sin, cos, 0, 0];
}
