/**
 * migration/xd/xd-token-converter.ts
 *
 * Converts Adobe XD color resources and character styles into
 * Logos TokenSets + Themes.
 *
 * XD has two token-like sources:
 *   1. Color resources  — named swatches + gradient brushes
 *   2. Character styles — font, size, weight, line-spacing, color
 *
 * Each source maps to a separate LogosTokenSet.
 * A single "XD Default" theme is emitted for consistency with the
 * Figma/Sketch token model.
 */

import type {
  XdGraphicContent,
  XdColorResource,
  XdCharacterStyleResource,
  XdBrushResource,
  XdFill,
  XdColor,
} from "./xd-format";

import { xdColorToHex, xdFontStyleToWeight } from "./xd-format";

import type {
  LogosTokenSet,
  LogosTokenTheme,
  LogosToken,
} from "../figma/figma-token-converter";

// ─── Public API ──────────────────────────────────────────────────────────────

export interface XdTokenConversionResult {
  sets: LogosTokenSet[];
  themes: LogosTokenTheme[];
  warnings: string[];
}

export function convertXdTokens(
  content: XdGraphicContent,
  documentName: string
): XdTokenConversionResult {
  const warnings: string[] = [];
  const sets: LogosTokenSet[] = [];

  const resources = content.resources;
  if (!resources) {
    return { sets: [], themes: [], warnings: ["No resource block found in XD document."] };
  }

  // ── 1. Color resources (named swatches) ───────────────────────────────────
  const colorTokens = extractColorTokens(resources.colors ?? [], warnings);
  if (colorTokens.length > 0) {
    sets.push({
      id: "xd-colors",
      name: "Colors",
      description: `Color swatches from ${documentName}`,
      tokens: colorTokens,
    });
  }

  // ── 2. Brush resources (named gradients + solid colors from library) ──────
  const brushTokens = extractBrushTokens(resources.brushes ?? [], warnings);
  if (brushTokens.length > 0) {
    sets.push({
      id: "xd-brushes",
      name: "Brushes",
      description: `Color and gradient brushes from ${documentName}`,
      tokens: brushTokens,
    });
  }

  // ── 3. Character styles ─────────────────────────────────────────────────
  const charTokens = extractCharStyleTokens(resources.characterStyles ?? [], warnings);
  if (charTokens.length > 0) {
    sets.push({
      id: "xd-char-styles",
      name: "Character Styles",
      description: `Typography tokens from ${documentName}`,
      tokens: charTokens,
    });
  }

  const themes: LogosTokenTheme[] =
    sets.length > 0
      ? [
          {
            id: "xd-default",
            name: "Default",
            group: "XD",
            description: "Default theme from Adobe XD import",
            overrides: {},
          },
        ]
      : [];

  return { sets, themes, warnings };
}

// ─── Color resource extraction ────────────────────────────────────────────────

function extractColorTokens(
  colors: XdColorResource[],
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  for (const cr of colors) {
    const name = cr.meta?.ux?.name;
    const color = cr.value;
    if (!name || !color) continue;

    try {
      tokens.push({
        id: cr.meta?.ux?.localId ?? crypto.randomUUID(),
        name: `Colors/${name}`,
        type: "color",
        value: xdColorToHex(color),
        description: "Color swatch from Adobe XD",
      });
    } catch {
      warnings.push(`Could not parse color "${name}"`);
    }
  }

  return tokens;
}

// ─── Brush resource extraction ────────────────────────────────────────────────

function extractBrushTokens(
  brushes: XdBrushResource[],
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  for (const br of brushes) {
    const name = br.meta?.ux?.name;
    if (!name) continue;

    // Solid brush
    if (br.type === "solid" && br.color) {
      try {
        tokens.push({
          id: br.meta?.ux?.localId ?? crypto.randomUUID(),
          name: `Brushes/${name}`,
          type: "color",
          value: xdColorToHex(br.color),
          description: "Solid brush from Adobe XD",
        });
      } catch {
        warnings.push(`Could not parse brush color "${name}"`);
      }
    }
    // Gradient brush — emit each stop as a color token
    else if (br.gradient && br.gradient.stops.length >= 2) {
      br.gradient.stops.forEach((stop, i) => {
        try {
          tokens.push({
            id: (br.meta?.ux?.localId ?? crypto.randomUUID()) + `-stop${i}`,
            name: `Brushes/${name}/stop-${i}`,
            type: "color",
            value: xdColorToHex(stop.color),
            description: `Gradient brush stop ${i} from Adobe XD`,
          });
        } catch {
          warnings.push(`Could not parse gradient stop ${i} in "${name}"`);
        }
      });
    }
  }

  return tokens;
}

// ─── Character style extraction ───────────────────────────────────────────────

function extractCharStyleTokens(
  charStyles: XdCharacterStyleResource[],
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  for (const cs of charStyles) {
    const name = cs.meta?.ux?.name;
    if (!name) continue;

    const font  = cs.style?.font ?? cs.style?.textAttributes;
    const fill  = cs.style?.fill ?? cs.style?.textAttributes?.fill;
    const attrs = cs.style?.textAttributes;

    const family = (font as { family?: string } | undefined)?.family;
    const style  = (font as { style?: string }  | undefined)?.style  ?? "Regular";
    const size   = (font as { size?: number }   | undefined)?.size;

    if (family) {
      tokens.push({
        id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-family",
        name: `Character Styles/${name}/font-family`,
        type: "string",
        value: family,
        description: `Font family from XD character style "${name}"`,
      });
    }

    if (size) {
      tokens.push({
        id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-size",
        name: `Character Styles/${name}/font-size`,
        type: "number",
        value: String(size),
        description: `Font size from XD character style "${name}"`,
      });
    }

    const weight = xdFontStyleToWeight(style);
    if (weight !== 400) {
      tokens.push({
        id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-weight",
        name: `Character Styles/${name}/font-weight`,
        type: "number",
        value: String(weight),
        description: `Font weight from XD character style "${name}"`,
      });
    }

    if (fill) {
      const hex = extractFillColor(fill);
      if (hex) {
        tokens.push({
          id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-color",
          name: `Character Styles/${name}/color`,
          type: "color",
          value: hex,
          description: `Text color from XD character style "${name}"`,
        });
      }
    }

    if (attrs?.lineSpacing && attrs.lineSpacing > 0) {
      tokens.push({
        id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-lh",
        name: `Character Styles/${name}/line-height`,
        type: "number",
        value: String(attrs.lineSpacing),
        description: `Line height from XD character style "${name}"`,
      });
    }

    if (typeof attrs?.letterSpacing === "number") {
      tokens.push({
        id: (cs.meta?.ux?.localId ?? crypto.randomUUID()) + "-ls",
        name: `Character Styles/${name}/letter-spacing`,
        type: "number",
        value: String(attrs.letterSpacing),
        description: `Letter spacing from XD character style "${name}"`,
      });
    }
  }

  return tokens;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

function extractFillColor(fill: XdFill): string | null {
  if (fill.type === "solid" && fill.color) {
    try { return xdColorToHex(fill.color); } catch { return null; }
  }
  return null;
}
