/**
 * migration/sketch/sketch-token-converter.ts
 *
 * Converts Sketch shared styles and color swatches to Logos TokenSets + Themes.
 *
 * Sketch has three token-like sources:
 *   1. Shared layer styles  — fill colors, border colors, shadows
 *   2. Shared text styles   — font, size, weight, line-height, color
 *   3. Swatches / assets    — named color variables (Sketch >= 69)
 *
 * Each source becomes a separate LogosTokenSet.  A single "Sketch Default"
 * theme is produced so the import is consistent with the Figma token model.
 *
 * Naming convention:
 *   Layer styles:  "Layer Styles/<style name>/fill", "/border", etc.
 *   Text styles:   "Text Styles/<style name>/font-family", "/font-size", etc.
 *   Swatches:      "Swatches/<swatch name>"
 */

import type {
  SketchDocumentJson,
  SketchSharedStyle,
  SketchColor,
} from "./sketch-format";

import { sketchColorToHex } from "./sketch-format";

import type {
  LogosTokenSet,
  LogosTokenTheme,
  LogosToken,
} from "../figma/figma-token-converter";

// ─── Public API ──────────────────────────────────────────────────────────────

export interface SketchTokenConversionResult {
  sets: LogosTokenSet[];
  themes: LogosTokenTheme[];
  warnings: string[];
}

export function convertSketchTokens(doc: SketchDocumentJson): SketchTokenConversionResult {
  const warnings: string[] = [];
  const sets: LogosTokenSet[] = [];

  // ── 1. Color swatches (most reliable token source) ───────────────────────
  const swatchTokens = extractSwatchTokens(doc, warnings);
  if (swatchTokens.length > 0) {
    sets.push({ id: "sketch-swatches", name: "Swatches", description: "Color variables from Sketch", tokens: swatchTokens });
  }

  // ── 2. Shared layer styles ────────────────────────────────────────────────
  const layerStyleTokens = extractLayerStyleTokens(
    doc.layerStyles?.objects ?? [],
    warnings
  );
  if (layerStyleTokens.length > 0) {
    sets.push({ id: "sketch-layer-styles", name: "Layer Styles", description: "Colors extracted from Sketch shared layer styles", tokens: layerStyleTokens });
  }

  // ── 3. Shared text styles ─────────────────────────────────────────────────
  const textStyleTokens = extractTextStyleTokens(
    doc.layerTextStyles?.objects ?? [],
    warnings
  );
  if (textStyleTokens.length > 0) {
    sets.push({ id: "sketch-text-styles", name: "Text Styles", description: "Typography tokens extracted from Sketch shared text styles", tokens: textStyleTokens });
  }

  // ── Single default theme ──────────────────────────────────────────────────
  const themes: LogosTokenTheme[] =
    sets.length > 0
      ? [
          {
            id: "sketch-default",
            name: "Default",
            group: "Sketch",
            description: "Default theme from Sketch import",
            overrides: {},
          },
        ]
      : [];

  return { sets, themes, warnings };
}

// ─── Swatch extraction ────────────────────────────────────────────────────────

function extractSwatchTokens(
  doc: SketchDocumentJson,
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  // sharedSwatches (Sketch Color Variables, >= 69)
  for (const swatch of doc.sharedSwatches?.objects ?? []) {
    try {
      tokens.push({
        id: swatch.do_objectID,
        name: `Swatches/${swatch.name}`,
        type: "color",
        value: sketchColorToHex(swatch.value),
        description: "Color variable from Sketch",
      });
    } catch {
      warnings.push(`Could not parse swatch: ${swatch.name}`);
    }
  }

  // assets.colorAssets (legacy color palette)
  for (const asset of doc.assets?.colorAssets ?? []) {
    if (!asset.name || !asset.color) continue;
    try {
      tokens.push({
        id: crypto.randomUUID(),
        name: `Swatches/${asset.name}`,
        type: "color",
        value: sketchColorToHex(asset.color),
        description: "Color asset from Sketch palette",
      });
    } catch {
      warnings.push(`Could not parse color asset: ${asset.name}`);
    }
  }

  // assets.colors (older format)
  for (const asset of doc.assets?.colors ?? []) {
    if (!asset.name || !asset.color) continue;
    try {
      tokens.push({
        id: crypto.randomUUID(),
        name: `Swatches/${asset.name}`,
        type: "color",
        value: sketchColorToHex(asset.color),
        description: "Color asset from Sketch library",
      });
    } catch {
      warnings.push(`Could not parse color: ${asset.name}`);
    }
  }

  return tokens;
}

// ─── Layer style extraction ───────────────────────────────────────────────────
//
// From each shared layer style we extract:
//   - fill color (first enabled solid fill)
//   - fill opacity
//   - border color (first enabled border)

function extractLayerStyleTokens(
  objects: SketchSharedStyle[],
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  for (const style of objects) {
    const slug = style.name;
    const st = style.value;
    if (!st) continue;

    // First enabled solid fill → color token
    const solidFill = (st.fills ?? []).find(
      (f) => f.isEnabled && f.fillType === 0 && f.color
    );
    if (solidFill) {
      try {
        tokens.push(...colorTokens(style.do_objectID, slug, "fill", solidFill.color));
      } catch {
        warnings.push(`Layer style "${slug}": could not parse fill color`);
      }
    }

    // First enabled border → color token
    const border = (st.borders ?? []).find((b) => b.isEnabled && b.color);
    if (border) {
      try {
        tokens.push(...colorTokens(style.do_objectID + "-border", slug, "border", border.color));
      } catch {
        warnings.push(`Layer style "${slug}": could not parse border color`);
      }
    }
  }

  return tokens;
}

function colorTokens(
  baseId: string,
  styleName: string,
  slot: string,
  color: SketchColor
): LogosToken[] {
  const hex = sketchColorToHex(color);
  return [
    {
      id: baseId + "-color",
      name: `Layer Styles/${styleName}/${slot}`,
      type: "color",
      value: hex,
      description: `From shared layer style "${styleName}"`,
    },
    ...(color.alpha < 1
      ? [
          {
            id: baseId + "-opacity",
            name: `Layer Styles/${styleName}/${slot}-opacity`,
            type: "number" as const,
            value: String(color.alpha),
            description: `Opacity for "${styleName}" ${slot}`,
          },
        ]
      : []),
  ];
}

// ─── Text style extraction ────────────────────────────────────────────────────
//
// From each shared text style we extract:
//   - font-family
//   - font-size
//   - font-weight (inferred from PostScript name component, e.g. "-Bold" → 700)
//   - text-color
//   - line-height (if explicitly set)

function extractTextStyleTokens(
  objects: SketchSharedStyle[],
  warnings: string[]
): LogosToken[] {
  const tokens: LogosToken[] = [];

  for (const style of objects) {
    const slug = style.name;
    const textStyle = style.value?.textStyle;
    if (!textStyle) continue;

    const attrs = textStyle.encodedAttributes;
    if (!attrs) continue;

    const fontAttr = attrs.MSAttributedStringFontAttribute?.attributes;
    const colorAttr = attrs.MSAttributedStringColorAttribute;

    if (fontAttr) {
      const { family, weight } = parseFontName(fontAttr.name);

      tokens.push({
        id: style.do_objectID + "-family",
        name: `Text Styles/${slug}/font-family`,
        type: "string",
        value: family,
        description: `Font family from text style "${slug}"`,
      });

      tokens.push({
        id: style.do_objectID + "-size",
        name: `Text Styles/${slug}/font-size`,
        type: "number",
        value: String(fontAttr.size),
        description: `Font size from text style "${slug}"`,
      });

      if (weight !== 400) {
        tokens.push({
          id: style.do_objectID + "-weight",
          name: `Text Styles/${slug}/font-weight`,
          type: "number",
          value: String(weight),
          description: `Font weight from text style "${slug}"`,
        });
      }
    }

    if (colorAttr) {
      try {
        tokens.push({
          id: style.do_objectID + "-color",
          name: `Text Styles/${slug}/color`,
          type: "color",
          value: sketchColorToHex(colorAttr),
          description: `Text color from text style "${slug}"`,
        });
      } catch {
        warnings.push(`Text style "${slug}": could not parse color`);
      }
    }

    // Line height
    if (attrs.lineHeight && attrs.lineHeight > 0) {
      tokens.push({
        id: style.do_objectID + "-lh",
        name: `Text Styles/${slug}/line-height`,
        type: "number",
        value: String(attrs.lineHeight),
        description: `Line height from text style "${slug}"`,
      });
    }
  }

  return tokens;
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Parse a PostScript font name like "Inter-Bold" → { family: "Inter", weight: 700 }.
 *
 * Common PostScript weight suffixes → CSS font-weight mapping:
 *   Thin / Hairline    → 100
 *   ExtraLight / Ultra → 200
 *   Light              → 300
 *   Regular / Book     → 400
 *   Medium             → 500
 *   SemiBold / Demi    → 600
 *   Bold               → 700
 *   ExtraBold / Ultra  → 800
 *   Black / Heavy      → 900
 */
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
    [/regular|book|normal/i, 400],
  ];

  const parts = psName.split("-");
  const family = parts[0].replace(/([A-Z])/g, " $1").trim(); // "InterVar" → "Inter Var"
  const suffix = parts.slice(1).join(" ");

  for (const [re, w] of WEIGHT_MAP) {
    if (re.test(suffix)) return { family, weight: w };
  }
  return { family, weight: 400 };
}
