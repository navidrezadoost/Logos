/**
 * utils/cssCodegen.ts
 *
 * P4.9 Dev Mode — CSS codegen from a Shape.
 *
 * Generates structured CSS property groups from a Shape so the
 * DevModePanel can render them with copy-to-clipboard support.
 */

import type { Shape } from "../types/shapes";

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

export interface CssProp {
  /** CSS property name, e.g. "width" */
  property: string;
  /** CSS value string, e.g. "240px" */
  value: string;
}

export interface CssGroup {
  /** Section heading, e.g. "Layout", "Fill", "Transform" */
  label: string;
  props: CssProp[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Hex color → `rgba(r, g, b, a)` string.
 * Handles #rgb, #rgba, #rrggbb, #rrggbbaa.
 */
function hexToRgba(hex: string, opacity = 1): string {
  const h = hex.replace("#", "");
  let r = 0, g = 0, b = 0, a = opacity;

  if (h.length === 3 || h.length === 4) {
    r = parseInt(h[0] + h[0], 16);
    g = parseInt(h[1] + h[1], 16);
    b = parseInt(h[2] + h[2], 16);
    if (h.length === 4) a = (parseInt(h[3] + h[3], 16) / 255) * opacity;
  } else if (h.length === 6 || h.length === 8) {
    r = parseInt(h.slice(0, 2), 16);
    g = parseInt(h.slice(2, 4), 16);
    b = parseInt(h.slice(4, 6), 16);
    if (h.length === 8) a = (parseInt(h.slice(6, 8), 16) / 255) * opacity;
  }

  const alpha = Math.round(a * 1000) / 1000;
  return alpha === 1 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

/** Round to at most 2 decimal places, remove trailing zeros. */
function px(n: number): string {
  const rounded = Math.round(n * 100) / 100;
  return `${rounded}px`;
}

/** CSS border-radius for circle / ellipse. */
function radiusForShape(shape: Shape): string | null {
  if (shape.type === "circle" || shape.type === "ellipse") return "50%";
  if (shape.type === "frame" || shape.type === "rect") return "0px";
  return null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Main codegen
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Build an ordered list of CSS groups for `shape`.
 * Returns ready-to-render groups that DevModePanel can display.
 */
export function generateCssGroups(shape: Shape): CssGroup[] {
  const groups: CssGroup[] = [];

  // ── Layout ────────────────────────────────────────────────────────────────
  const layoutProps: CssProp[] = [
    { property: "width",  value: px(shape.bounds.w) },
    { property: "height", value: px(shape.bounds.h) },
    { property: "left",   value: px(shape.bounds.x) },
    { property: "top",    value: px(shape.bounds.y) },
    { property: "position", value: "absolute" },
  ];
  const borderRadius = radiusForShape(shape);
  if (borderRadius) {
    layoutProps.push({ property: "border-radius", value: borderRadius });
  }
  groups.push({ label: "Layout", props: layoutProps });

  // ── Fill ─────────────────────────────────────────────────────────────────
  if (shape.fills.length > 0) {
    const fillProps: CssProp[] = shape.fills.map((f, i) => ({
      property: shape.fills.length === 1 ? "background-color" : `/* fill-${i + 1} */ background-color`,
      value: hexToRgba(f.color, f.opacity),
    }));
    groups.push({ label: "Fill", props: fillProps });
  } else {
    groups.push({ label: "Fill", props: [{ property: "background-color", value: "transparent" }] });
  }

  // ── Opacity ───────────────────────────────────────────────────────────────
  if (shape.opacity < 1) {
    groups.push({
      label: "Opacity",
      props: [{ property: "opacity", value: String(Math.round(shape.opacity * 1000) / 1000) }],
    });
  }

  // ── Transform ─────────────────────────────────────────────────────────────
  if (shape.rotation !== 0) {
    groups.push({
      label: "Transform",
      props: [{ property: "transform", value: `rotate(${Math.round(shape.rotation * 100) / 100}deg)` }],
    });
  }

  // ── Visibility ────────────────────────────────────────────────────────────
  if (shape.hidden) {
    groups.push({
      label: "Visibility",
      props: [{ property: "display", value: "none" }],
    });
  }

  // ── Children layout (frames / groups as flex containers) ──────────────────
  if ((shape.type === "frame" || shape.type === "group") && shape.children.length > 0) {
    groups.push({
      label: "Children",
      props: [
        { property: "/* child count */", value: String(shape.children.length) },
        { property: "overflow",          value: "hidden" },
      ],
    });
  }

  // ── Variable font axes ────────────────────────────────────────────────────
  if (shape.fontVariationSettings && Object.keys(shape.fontVariationSettings).length > 0) {
    const axes = Object.entries(shape.fontVariationSettings)
      .map(([tag, val]) => `"${tag}" ${val}`)
      .join(", ");
    groups.push({
      label: "Typography",
      props: [{ property: "font-variation-settings", value: axes }],
    });
  }

  return groups;
}

/**
 * Flatten all groups into a single CSS rule block string.
 * Example output:
 *   .shape-name {
 *     width: 200px;
 *     height: 100px;
 *     ...
 *   }
 */
export function generateCssBlock(shape: Shape): string {
  const slug = shape.name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "") || "shape";

  const groups = generateCssGroups(shape);
  const lines = groups
    .flatMap((g) => g.props)
    .filter((p) => !p.property.startsWith("/*"))
    .map((p) => `  ${p.property}: ${p.value};`);

  return `.${slug} {\n${lines.join("\n")}\n}`;
}
