/**
 * figma-plugin/code.ts
 *
 * Logos "Export for Logos" Figma Plugin — Backend (runs in Figma sandbox).
 *
 * Exports two things in a single JSON file:
 *   1. Variables (tokens): all collections, modes, and aliases.
 *   2. Node tree: all pages → frames → shapes, text, components, instances.
 *
 * The user downloads the `.logos-figma.json` file and imports it into Logos.
 * No API key, no network, fully offline — uses only the official figma.* API.
 *
 * Build: tsc --outDir dist code.ts
 */

figma.showUI(__html__, { width: 360, height: 380, title: "Export for Logos" });

figma.ui.onmessage = async (msg: { type: string; scope?: string }) => {
  if (msg.type === "export") {
    try {
      const scope = msg.scope ?? "page"; // "page" | "all"
      const data = await buildExport(scope);
      figma.ui.postMessage({ type: "download", data });
    } catch (err) {
      figma.ui.postMessage({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }
  if (msg.type === "close") figma.closePlugin();
};

// ─── Top-level builder ────────────────────────────────────────────────────────

async function buildExport(scope: string): Promise<LogosFigmaExport> {
  // ── Tokens ──────────────────────────────────────────────────────────────
  const rawVars = figma.variables.getLocalVariables();
  const rawCollections = figma.variables.getLocalVariableCollections();
  const collectionMap = new Map(rawCollections.map((c) => [c.id, c]));

  const collections: ExportCollection[] = rawCollections.map((c) => ({
    id: c.id,
    name: c.name,
    modes: c.modes.map((m) => ({ id: m.modeId, name: m.name })),
    defaultModeId: c.defaultModeId,
  }));

  const variables: ExportVariable[] = rawVars.map((v) => {
    const collection = collectionMap.get(v.variableCollectionId);
    const valuesByMode: Record<string, ExportVariableValue> = {};
    for (const [modeId, raw] of Object.entries(v.valuesByMode)) {
      valuesByMode[modeId] = encodeValue(raw, v.resolvedType);
    }
    return {
      id: v.id,
      name: v.name,
      collectionId: v.variableCollectionId,
      collectionName: collection?.name ?? "",
      type: v.resolvedType as ExportVariableType,
      valuesByMode,
      scopes: v.scopes as string[],
      hiddenFromPublishing: v.hiddenFromPublishing,
      description: v.description,
    };
  });

  // ── Nodes ────────────────────────────────────────────────────────────────
  const pagesToExport =
    scope === "all" ? figma.root.children : [figma.currentPage];

  const pages: ExportPage[] = pagesToExport.map((page) => ({
    id: page.id,
    name: page.name,
    children: page.children.map((node) => serializeNode(node)),
  }));

  return {
    version: 1,
    schemaVersion: 2,       // v2 = tokens + nodes
    source: "figma-plugin",
    exportedAt: new Date().toISOString(),
    documentName: figma.root.name,
    collections,
    variables,
    pages,
  };
}

// ─── Node serializer ─────────────────────────────────────────────────────────

function serializeNode(node: SceneNode): ExportNode {
  const base: ExportNodeBase = {
    id: node.id,
    name: node.name,
    type: node.type,
    visible: "visible" in node ? (node as SceneNode & { visible: boolean }).visible : true,
    locked:  "locked"  in node ? (node as SceneNode & { locked: boolean  }).locked  : false,
    x: "x" in node ? (node as { x: number }).x : 0,
    y: "y" in node ? (node as { y: number }).y : 0,
    width:  "width"  in node ? (node as { width:  number }).width  : 0,
    height: "height" in node ? (node as { height: number }).height : 0,
    rotation: "rotation" in node ? (node as { rotation: number }).rotation : 0,
    opacity: "opacity" in node ? (node as { opacity: number }).opacity : 1,
    fills: extractFills(node),
    strokes: extractStrokes(node),
    effects: extractEffects(node),
    constraints: extractConstraints(node),
    layout: extractLayout(node),
    blendMode: "blendMode" in node ? String((node as { blendMode: unknown }).blendMode) : "NORMAL",
    children: [],
  };

  // Text
  if (node.type === "TEXT") {
    const t = node as TextNode;
    (base as ExportTextNode).text = t.characters;
    const fs = t.getRangeFontSize(0, 1);
    const fw = t.getRangeFontWeight ? t.getRangeFontWeight(0, 1) : undefined;
    const fn = t.getRangeFontName(0, 1);
    (base as ExportTextNode).fontSize = typeof fs === "number" ? fs : 14;
    (base as ExportTextNode).fontWeight = typeof fw === "number" ? fw : 400;
    (base as ExportTextNode).fontFamily = typeof fn === "object" && fn !== figma.mixed ? (fn as FontName).family : "Inter";
    (base as ExportTextNode).textAlign = t.textAlignHorizontal;
    (base as ExportTextNode).lineHeight = t.lineHeight;
    (base as ExportTextNode).letterSpacing = t.letterSpacing;
    // textDecoration — getRangeTextDecoration returns mixed or a constant
    const td = t.getRangeTextDecoration ? t.getRangeTextDecoration(0, 1) : undefined;
    (base as ExportTextNode).textDecoration =
      td !== figma.mixed ? String(td ?? "NONE") : "NONE";
  }

  // Component property definitions
  if (node.type === "COMPONENT" || node.type === "COMPONENT_SET") {
    const c = node as ComponentNode | ComponentSetNode;
    if (c.componentPropertyDefinitions) {
      (base as ExportComponentNode).propertyDefinitions = Object.fromEntries(
        Object.entries(c.componentPropertyDefinitions).map(([k, def]) => [
          k,
          { type: def.type, defaultValue: String(def.defaultValue), variantOptions: (def as { variantOptions?: string[] }).variantOptions },
        ])
      );
    }
  }

  // Instance bindings
  if (node.type === "INSTANCE") {
    const inst = node as InstanceNode;
    (base as ExportInstanceNode).mainComponentId = inst.mainComponent?.id ?? "";
    (base as ExportInstanceNode).componentProperties = Object.fromEntries(
      Object.entries(inst.componentProperties ?? {}).map(([k, v]) => [k, String(v.value)])
    );
  }

  // Vector network — export full topology for complex VECTOR nodes
  if (node.type === "VECTOR") {
    const vn = (node as VectorNode).vectorNetwork;
    if (vn && (vn.vertices.length > 0 || vn.segments.length > 0)) {
      (base as ExportNodeBase & { vectorNetwork: unknown }).vectorNetwork = {
        vertices: vn.vertices.map((v) => ({
          x: v.x,
          y: v.y,
          strokeCap: v.strokeCap,
          strokeJoin: (v as { strokeJoin?: string }).strokeJoin,
          cornerRadius: v.cornerRadius,
          handleMirrorType: v.handleMirrorType,
        })),
        segments: vn.segments.map((s) => ({
          start: s.start,
          end: s.end,
          tangentStart: s.tangentStart,
          tangentEnd: s.tangentEnd,
        })),
        regions: vn.regions?.map((r) => ({
          windingRule: r.windingRule,
          loops: r.loops,
        })),
      };
    }
  }

  // Recurse into children
  if ("children" in node) {
    base.children = (node as ChildrenMixin).children.map((ch) => serializeNode(ch));
  }

  return base as ExportNode;
}

// ─── Paint / Effect extractors ────────────────────────────────────────────────

function extractFills(node: SceneNode): ExportPaint[] {
  if (!("fills" in node) || node.fills === figma.mixed || !Array.isArray(node.fills)) return [];
  return (node.fills as Paint[]).map(encodePaint);
}

function extractStrokes(node: SceneNode): ExportPaint[] {
  if (!("strokes" in node) || !Array.isArray((node as GeometryMixin).strokes)) return [];
  return ((node as GeometryMixin).strokes as Paint[]).map(encodePaint);
}

function extractEffects(node: SceneNode): ExportEffect[] {
  if (!("effects" in node) || !Array.isArray((node as BlendMixin).effects)) return [];
  return ((node as BlendMixin).effects as Effect[]).map((e) => ({
    type: e.type,
    visible: e.visible,
    radius: "radius" in e ? (e as { radius: number }).radius : 0,
    color: "color" in e ? rgbaToHex((e as { color: RGBA }).color) : undefined,
    offset: "offset" in e ? (e as { offset: Vector }).offset : undefined,
    spread: "spread" in e ? (e as { spread: number }).spread : undefined,
  }));
}

function extractConstraints(node: SceneNode): ExportConstraints | undefined {
  if (!("constraints" in node)) return undefined;
  const c = (node as ConstraintMixin).constraints;
  return { horizontal: c.horizontal, vertical: c.vertical };
}

function extractLayout(node: SceneNode): ExportLayout | undefined {
  if (!("layoutMode" in node)) return undefined;
  const f = node as FrameNode;
  if (f.layoutMode === "NONE") return undefined;
  return {
    mode: f.layoutMode,
    primaryAxisSizingMode: f.primaryAxisSizingMode,
    counterAxisSizingMode: f.counterAxisSizingMode,
    paddingTop: f.paddingTop,
    paddingRight: f.paddingRight,
    paddingBottom: f.paddingBottom,
    paddingLeft: f.paddingLeft,
    gap: f.itemSpacing,
    counterAxisSpacing: f.counterAxisSpacing ?? 0,
    layoutWrap: (f as { layoutWrap?: string }).layoutWrap ?? "NO_WRAP",
    primaryAxisAlignItems: f.primaryAxisAlignItems,
    counterAxisAlignItems: f.counterAxisAlignItems,
  };
}

function encodePaint(p: Paint): ExportPaint {
  if (p.type === "SOLID") {
    return { type: "SOLID", color: rgbaToHex({ ...p.color, a: p.opacity ?? 1 }), opacity: p.opacity ?? 1, visible: p.visible };
  }
  if (p.type === "GRADIENT_LINEAR" || p.type === "GRADIENT_RADIAL" || p.type === "GRADIENT_ANGULAR") {
    const gp = p as GradientPaint;
    return {
      type: p.type,
      opacity: p.opacity ?? 1,
      visible: p.visible,
      stops: gp.gradientStops.map((s) => ({ color: rgbaToHex(s.color), position: s.position })),
      transform: gp.gradientTransform,
    };
  }
  return { type: p.type, opacity: 1, visible: p.visible };
}

function rgbaToHex({ r, g, b, a }: RGBA): string {
  const toHex = (n: number) => Math.round(n * 255).toString(16).padStart(2, "0");
  return "#" + toHex(r) + toHex(g) + toHex(b) + (a < 1 ? toHex(a) : "");
}

// ─── Variable value encoder (unchanged from v1) ────────────────────────────

function encodeValue(raw: VariableValue, type: string): ExportVariableValue {
  if (typeof raw === "object" && raw !== null && (raw as VariableAlias).type === "VARIABLE_ALIAS") {
    return { alias: (raw as VariableAlias).id };
  }
  if (type === "COLOR" && typeof raw === "object" && "r" in raw) {
    const { r, g, b, a } = raw as RGBA;
    return { color: rgbaToHex({ r, g, b, a }) };
  }
  if (type === "FLOAT" && typeof raw === "number") return { number: raw };
  if (type === "STRING" && typeof raw === "string") return { string: raw };
  if (type === "BOOLEAN" && typeof raw === "boolean") return { boolean: raw };
  return { raw: String(raw) };
}

// ─── Exported types ───────────────────────────────────────────────────────────

type ExportVariableType = "COLOR" | "FLOAT" | "STRING" | "BOOLEAN";
interface ExportVariableValue { alias?: string; color?: string; number?: number; string?: string; boolean?: boolean; raw?: string; }
interface ExportMode { id: string; name: string; }
interface ExportCollection { id: string; name: string; modes: ExportMode[]; defaultModeId: string; }
interface ExportVariable { id: string; name: string; collectionId: string; collectionName: string; type: ExportVariableType; valuesByMode: Record<string, ExportVariableValue>; scopes: string[]; hiddenFromPublishing: boolean; description: string; }

interface ExportPaint {
  type: string;
  color?: string;
  opacity?: number;
  visible?: boolean;
  stops?: { color: string; position: number }[];
  transform?: number[][];
}
interface ExportEffect {
  type: string;
  visible: boolean;
  radius: number;
  color?: string;
  offset?: { x: number; y: number };
  spread?: number;
}
interface ExportConstraints { horizontal: string; vertical: string; }
interface ExportLayout {
  mode: string;
  primaryAxisSizingMode: string;
  counterAxisSizingMode: string;
  paddingTop: number; paddingRight: number; paddingBottom: number; paddingLeft: number;
  gap: number;
  counterAxisSpacing: number;
  layoutWrap: string;
  primaryAxisAlignItems: string;
  counterAxisAlignItems: string;
}

interface ExportNodeBase {
  id: string;
  name: string;
  type: string;
  visible: boolean;
  locked: boolean;
  x: number; y: number; width: number; height: number;
  rotation: number;
  opacity: number;
  fills: ExportPaint[];
  strokes: ExportPaint[];
  effects: ExportEffect[];
  constraints?: ExportConstraints;
  layout?: ExportLayout;
  blendMode: string;
  children: ExportNode[];
}

interface ExportTextNode extends ExportNodeBase {
  text?: string;
  fontSize?: number;
  fontWeight?: number;
  fontFamily?: string;
  textAlign?: string;
  textDecoration?: string;
  lineHeight?: LineHeight;
  letterSpacing?: LetterSpacing;
}

interface ExportComponentNode extends ExportNodeBase {
  propertyDefinitions?: Record<string, { type: string; defaultValue: string; variantOptions?: string[] }>;
}

interface ExportInstanceNode extends ExportNodeBase {
  mainComponentId?: string;
  componentProperties?: Record<string, string>;
}

type ExportNode = ExportNodeBase & Partial<ExportTextNode> & Partial<ExportComponentNode> & Partial<ExportInstanceNode>;

interface ExportPage { id: string; name: string; children: ExportNode[]; }

interface LogosFigmaExport {
  version: 1;
  schemaVersion?: number;
  source: "figma-plugin";
  exportedAt: string;
  documentName: string;
  collections: ExportCollection[];
  variables: ExportVariable[];
  pages?: ExportPage[];
}


figma.ui.onmessage = async (msg: { type: string }) => {
  if (msg.type === "export") {
    try {
      const data = buildExport();
      figma.ui.postMessage({ type: "download", data });
    } catch (err) {
      figma.ui.postMessage({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  if (msg.type === "close") {
    figma.closePlugin();
  }
};

// ─── Builder ─────────────────────────────────────────────────────────────────

function buildExport(): LogosFigmaExport {
  const rawVars = figma.variables.getLocalVariables();
  const rawCollections = figma.variables.getLocalVariableCollections();

  // Index collections for O(1) lookup
  const collectionMap = new Map(rawCollections.map((c) => [c.id, c]));

  const collections: ExportCollection[] = rawCollections.map((c) => ({
    id: c.id,
    name: c.name,
    modes: c.modes.map((m) => ({ id: m.modeId, name: m.name })),
    defaultModeId: c.defaultModeId,
  }));

  const variables: ExportVariable[] = rawVars.map((v) => {
    const collection = collectionMap.get(v.variableCollectionId);

    const valuesByMode: Record<string, ExportVariableValue> = {};
    for (const [modeId, raw] of Object.entries(v.valuesByMode)) {
      valuesByMode[modeId] = encodeValue(raw, v.resolvedType);
    }

    return {
      id: v.id,
      name: v.name,
      collectionId: v.variableCollectionId,
      collectionName: collection?.name ?? "",
      type: v.resolvedType as ExportVariableType,
      valuesByMode,
      scopes: v.scopes as string[],
      hiddenFromPublishing: v.hiddenFromPublishing,
      description: v.description,
    };
  });

  return {
    version: 1,
    source: "figma-plugin",
    exportedAt: new Date().toISOString(),
    documentName: figma.root.name,
    collections,
    variables,
  };
}

function encodeValue(
  raw: VariableValue,
  type: string
): ExportVariableValue {
  // Alias reference
  if (
    typeof raw === "object" &&
    raw !== null &&
    (raw as VariableAlias).type === "VARIABLE_ALIAS"
  ) {
    return { alias: (raw as VariableAlias).id };
  }

  // Color: Figma uses { r, g, b, a } where channels are 0–1
  if (type === "COLOR" && typeof raw === "object" && "r" in raw) {
    const { r, g, b, a } = raw as RGBA;
    const hex =
      "#" +
      [r, g, b]
        .map((ch) =>
          Math.round(ch * 255)
            .toString(16)
            .padStart(2, "0")
        )
        .join("") +
      (a < 1
        ? Math.round(a * 255)
            .toString(16)
            .padStart(2, "0")
        : "");
    return { color: hex };
  }

  // Number / Float
  if (type === "FLOAT" && typeof raw === "number") {
    return { number: raw };
  }

  // String
  if (type === "STRING" && typeof raw === "string") {
    return { string: raw };
  }

  // Boolean
  if (type === "BOOLEAN" && typeof raw === "boolean") {
    return { boolean: raw };
  }

  // Fallback: stringify whatever Figma gave us
  return { raw: String(raw) };
}

// ─── Exported types (must match figma-plugin-format.ts on the import side) ──

type ExportVariableType = "COLOR" | "FLOAT" | "STRING" | "BOOLEAN";

interface ExportVariableValue {
  alias?: string;       // Figma variable ID being aliased
  color?: string;       // hex string "#rrggbb" or "#rrggbbaa"
  number?: number;
  string?: string;
  boolean?: boolean;
  raw?: string;         // fallback
}

interface ExportMode {
  id: string;
  name: string;
}

interface ExportCollection {
  id: string;
  name: string;
  modes: ExportMode[];
  defaultModeId: string;
}

interface ExportVariable {
  id: string;
  name: string;
  collectionId: string;
  collectionName: string;
  type: ExportVariableType;
  valuesByMode: Record<string, ExportVariableValue>;
  scopes: string[];
  hiddenFromPublishing: boolean;
  description: string;
}

interface LogosFigmaExport {
  version: 1;
  source: "figma-plugin";
  exportedAt: string;
  documentName: string;
  collections: ExportCollection[];
  variables: ExportVariable[];
}
