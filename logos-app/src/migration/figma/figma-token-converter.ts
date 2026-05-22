/**
 * migration/figma/figma-token-converter.ts
 *
 * Phase IM1 — Converts a LogosFigmaExport (from the Logos Figma plugin) into
 * the Logos token runtime model: TokenSet records grouped by collection, and
 * TokenTheme records for each Figma mode.
 *
 * Mapping:
 *   Figma Collection  →  one LogosTokenSet  (groups tokens by namespace)
 *   Figma Mode        →  one LogosTokenTheme (within that set)
 *   Figma Variable    →  one LogosToken  (inside the appropriate set)
 *   Figma alias value →  {referenced.token.name} reference string
 *   Figma color value →  "#rrggbb" / "#rrggbbaa"
 *   Figma float value →  numeric string  "16"
 *   Figma string/bool →  plain string
 */

import type {
  LogosFigmaExport,
  FigmaVariable,
  FigmaVariableValue,
} from "./figma-plugin-format";

// ─── Logos token model (minimal runtime types) ───────────────────────────────
// These mirror the ClojureScript Token / TokenSet / TokenTheme records
// from common/types/tokens_lib.cljc, using plain TypeScript objects.
// When we persist to the file format, these serialize to the same JSON.

export type LogosTokenType =
  | "color"
  | "number"
  | "string"
  | "boolean"
  | "spacing"
  | "dimensions"
  | "opacity";

export interface LogosToken {
  id: string;
  name: string;          // slash-separated path: "brand/blue/500"
  type: LogosTokenType;
  /** Resolved value or "{alias.path}" reference string */
  value: string;
  description: string;
}

/**
 * Corresponds to a Figma collection.
 * Contains tokens for a specific mode activation.
 */
export interface LogosTokenSet {
  id: string;
  name: string;
  description: string;
  /** Full token list (initial/default mode values) */
  tokens: LogosToken[];
}

/**
 * A mode override: maps token names to their values in this theme/mode.
 * When a theme is active, these values overrule the base TokenSet values.
 */
export interface LogosTokenTheme {
  id: string;
  name: string;
  /** Parent collection name */
  group: string;
  description: string;
  /** token name → value override */
  overrides: Record<string, string>;
}

// ─── Conversion result ───────────────────────────────────────────────────────

export interface ConversionResult {
  /** One TokenSet per Figma collection */
  sets: LogosTokenSet[];
  /** One TokenTheme per (collection × mode) pair, excluding the default mode */
  themes: LogosTokenTheme[];
  /** Any warnings generated during conversion (non-fatal) */
  warnings: string[];
}

// ─── Public API ──────────────────────────────────────────────────────────────

/**
 * Convert a Logos Figma plugin export into Logos token sets and themes.
 *
 * @param figmaJson  Parsed JSON from a `.logos-figma.json` file.
 * @returns          Sets, themes, and any non-fatal warnings.
 */
export function convertFigmaExport(figmaJson: LogosFigmaExport): ConversionResult {
  const warnings: string[] = [];

  // Build an id→name lookup so we can resolve alias references by name
  const varIdToName = new Map<string, string>(
    figmaJson.variables.map((v) => [v.id, figmaPathToLogosPath(v.name)])
  );

  const sets: LogosTokenSet[] = [];
  const themes: LogosTokenTheme[] = [];

  for (const collection of figmaJson.collections) {
    const collectionVars = figmaJson.variables.filter(
      (v) => v.collectionId === collection.id
    );

    // Default mode: use its values as the canonical TokenSet
    const defaultMode = collection.modes.find(
      (m) => m.id === collection.defaultModeId
    ) ?? collection.modes[0];

    if (!defaultMode) {
      warnings.push(`Collection "${collection.name}" has no modes — skipped.`);
      continue;
    }

    const tokens: LogosToken[] = [];

    for (const variable of collectionVars) {
      const defaultValue = variable.valuesByMode[defaultMode.id];
      if (defaultValue === undefined) {
        warnings.push(
          `Variable "${variable.name}" has no value in default mode — skipped.`
        );
        continue;
      }

      const token = buildToken(variable, defaultValue, varIdToName);
      if (token) {
        tokens.push(token);
      } else {
        warnings.push(
          `Variable "${variable.name}" produced an empty value — skipped.`
        );
      }
    }

    sets.push({
      id: crypto.randomUUID(),
      name: collection.name,
      description: `Imported from Figma collection "${collection.name}"`,
      tokens,
    });

    // Non-default modes become themes with per-token value overrides
    for (const mode of collection.modes) {
      if (mode.id === defaultMode.id) continue;

      const overrides: Record<string, string> = {};

      for (const variable of collectionVars) {
        const modeValue = variable.valuesByMode[mode.id];
        if (!modeValue) continue;

        const tokenName = figmaPathToLogosPath(variable.name);
        const value = encodeValue(modeValue, variable.type, varIdToName);
        if (value !== null) {
          overrides[tokenName] = value;
        }
      }

      themes.push({
        id: crypto.randomUUID(),
        name: mode.name,
        group: collection.name,
        description: `Mode "${mode.name}" from collection "${collection.name}"`,
        overrides,
      });
    }
  }

  return { sets, themes, warnings };
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Convert a Figma variable name to a Logos token path.
 * Figma uses "/" as separator; Logos uses "." internally but "/" in display.
 * We preserve "/" so the hierarchy is readable in the token panel.
 *
 * "Brand/Primary/500" → "Brand/Primary/500"
 */
function figmaPathToLogosPath(name: string): string {
  return name.trim();
}

/** Map Figma variable type to the closest Logos token type. */
function figmaTypeToLogosType(
  figmaType: FigmaVariable["type"]
): LogosTokenType {
  switch (figmaType) {
    case "COLOR":   return "color";
    case "FLOAT":   return "number";
    case "STRING":  return "string";
    case "BOOLEAN": return "boolean";
    default:        return "string";
  }
}

/**
 * Encode a FigmaVariableValue into the Logos token value string.
 *
 * Colors → "#rrggbb" or "#rrggbbaa"
 * Numbers → "16"
 * Strings → "Inter"
 * Booleans → "true" / "false"
 * Aliases → "{referenced.token.name}"
 */
function encodeValue(
  val: FigmaVariableValue,
  type: FigmaVariable["type"],
  varIdToName: Map<string, string>
): string | null {
  if (val.alias !== undefined) {
    const referencedName = varIdToName.get(val.alias);
    if (!referencedName) return null;
    return `{${referencedName}}`;
  }

  if (type === "COLOR" && val.color !== undefined) {
    return val.color;
  }

  if (type === "FLOAT" && val.number !== undefined) {
    // Keep up to 4 decimal places; remove trailing zeros
    return String(parseFloat(val.number.toFixed(4)));
  }

  if (type === "STRING" && val.string !== undefined) {
    return val.string;
  }

  if (type === "BOOLEAN" && val.boolean !== undefined) {
    return val.boolean ? "true" : "false";
  }

  if (val.raw !== undefined) {
    return val.raw;
  }

  return null;
}

function buildToken(
  variable: FigmaVariable,
  defaultValue: FigmaVariableValue,
  varIdToName: Map<string, string>
): LogosToken | null {
  const value = encodeValue(defaultValue, variable.type, varIdToName);
  if (value === null) return null;

  return {
    id: crypto.randomUUID(),
    name: figmaPathToLogosPath(variable.name),
    type: figmaTypeToLogosType(variable.type),
    value,
    description: variable.description ?? "",
  };
}
