/**
 * migration/figma/figma-plugin-format.ts
 *
 * TypeScript types that describe the JSON produced by the Logos Figma plugin
 * (figma-plugin/code.ts).  The import side uses these to parse and validate
 * the file before converting.
 *
 * These types intentionally mirror the plugin-side declarations so both ends
 * stay in sync.  When the plugin format bumps its version, add a new
 * discriminated union branch here.
 */

// ─── Value types ─────────────────────────────────────────────────────────────

export type FigmaVariableType = "COLOR" | "FLOAT" | "STRING" | "BOOLEAN";

/**
 * A resolved (non-alias) or aliased value for a single variable in one mode.
 *
 * Exactly one field is set:
 *   alias   → this variable references another variable by Figma ID
 *   color   → "#rrggbb" or "#rrggbbaa"
 *   number  → numeric value (spacing, radius, font-size, etc.)
 *   string  → font family, custom string, etc.
 *   boolean → true / false (visibility, toggle)
 *   raw     → unexpected type; preserved as a string for debugging
 */
export interface FigmaVariableValue {
  alias?: string;
  color?: string;
  number?: number;
  string?: string;
  boolean?: boolean;
  raw?: string;
}

// ─── Structure ───────────────────────────────────────────────────────────────

export interface FigmaMode {
  id: string;
  name: string;
}

/**
 * Corresponds to a Figma Variable Collection.
 * In Logos: becomes a group/namespace within the token library.
 */
export interface FigmaCollection {
  id: string;
  name: string;
  modes: FigmaMode[];
  defaultModeId: string;
}

/**
 * Corresponds to a single Figma Variable.
 * In Logos: becomes a Token record inside a TokenSet.
 */
export interface FigmaVariable {
  id: string;
  name: string;
  collectionId: string;
  collectionName: string;
  type: FigmaVariableType;
  /** Keyed by Figma mode ID */
  valuesByMode: Record<string, FigmaVariableValue>;
  scopes: string[];
  hiddenFromPublishing: boolean;
  description: string;
}

// ─── Top-level export ────────────────────────────────────────────────────────

export interface LogosFigmaExport {
  version: 1;
  /** 1 = tokens only (schemaVersion absent/1), 2 = tokens + full node tree */
  schemaVersion?: number;
  source: "figma-plugin";
  exportedAt: string;
  documentName: string;
  collections: FigmaCollection[];
  variables: FigmaVariable[];
  pages?: FigmaExportPage[];
}

// ─── Guard ────────────────────────────────────────────────────────────────────

export function isLogosFigmaExport(data: unknown): data is LogosFigmaExport {
  if (typeof data !== "object" || data === null) return false;
  const d = data as Record<string, unknown>;
  return (
    d["version"] === 1 &&
    d["source"] === "figma-plugin" &&
    Array.isArray(d["collections"]) &&
    Array.isArray(d["variables"])
  );
}

// ─── Node types (schemaVersion 2+) ───────────────────────────────────────────

export interface FigmaExportPaint {
  type: string;         // "SOLID" | "GRADIENT_LINEAR" | "GRADIENT_RADIAL" | ...
  color?: string;       // hex, only for SOLID
  opacity?: number;
  visible?: boolean;
  stops?: { color: string; position: number }[];
  transform?: number[][];
}

export interface FigmaExportEffect {
  type: string;
  visible: boolean;
  radius: number;
  color?: string;
  offset?: { x: number; y: number };
  spread?: number;
}

export interface FigmaExportConstraints {
  horizontal: string;  // "LEFT" | "RIGHT" | "CENTER" | "SCALE" | "STRETCH"
  vertical: string;
}

export interface FigmaExportLayout {
  mode: string;                   // "HORIZONTAL" | "VERTICAL"
  primaryAxisSizingMode: string;
  counterAxisSizingMode: string;
  paddingTop: number;
  paddingRight: number;
  paddingBottom: number;
  paddingLeft: number;
  gap: number;
  counterAxisSpacing: number;
  layoutWrap: string;             // "NO_WRAP" | "WRAP"
  primaryAxisAlignItems: string;  // "MIN" | "CENTER" | "MAX" | "SPACE_BETWEEN"
  counterAxisAlignItems: string;
}

export interface FigmaExportNode {
  id: string;
  name: string;
  type: string;
  visible: boolean;
  locked: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  opacity: number;
  fills: FigmaExportPaint[];
  strokes: FigmaExportPaint[];
  effects: FigmaExportEffect[];
  constraints?: FigmaExportConstraints;
  layout?: FigmaExportLayout;
  blendMode: string;
  children: FigmaExportNode[];
  // Text
  text?: string;
  fontSize?: number;
  fontWeight?: number;
  fontFamily?: string;
  textAlign?: string;         // "LEFT" | "CENTER" | "RIGHT" | "JUSTIFIED"
  textDecoration?: string;    // "NONE" | "UNDERLINE" | "STRIKETHROUGH"
  lineHeight?: { unit: string; value?: number };      // { unit: "PIXELS" | "PERCENT" | "AUTO", value? }
  letterSpacing?: { unit: string; value?: number };   // { unit: "PIXELS" | "PERCENT", value }
  // Component
  propertyDefinitions?: Record<string, {
    type: string;
    defaultValue: string;
    variantOptions?: string[];
  }>;
  // Instance
  mainComponentId?: string;
  componentProperties?: Record<string, string>;
  // Vector network (VECTOR nodes with complex topology)
  vectorNetwork?: {
    vertices: Array<{
      x: number;
      y: number;
      strokeCap?: string;
      strokeJoin?: string;
      cornerRadius?: number;
      handleMirrorType?: string;
    }>;
    segments: Array<{
      start: number;
      end: number;
      tangentStart?: { x: number; y: number };
      tangentEnd?: { x: number; y: number };
    }>;
    regions?: Array<{
      windingRule: string;
      loops: number[][];
    }>;
  };
}

export interface FigmaExportPage {
  id: string;
  name: string;
  children: FigmaExportNode[];
}
