/**
 * rust-generated/compat.ts
 *
 * Malli-wire-format–compatible type surface that bridges the gap between
 * the Rust-native types (camelCase fields, different enum members) and the
 * legacy TypeScript types previously generated from Clojure/Malli schemas.
 *
 * Rules:
 *  • Types whose structure is IDENTICAL to the Rust ones are simply re-exported
 *    from the sibling modules (Point, Bounds, Transform, StrokePosition).
 *  • Types that are RENAMED get an alias export.
 *  • Types ABSENT from logos-types (layout, blend modes, …) are declared here
 *    verbatim, ported from the superseded generated/shapes.d.ts.
 *
 * Do not edit by hand.  When a Rust type supersedes a compat definition, remove
 * the compat entry and update shapes.ts accordingly.
 */

import type { StrokePosition } from "./stroke";
import type { ConstraintH, ConstraintV } from "./shape";
import type { ShadowStyle } from "./shadow";

// ── Re-exports: exact structural matches ─────────────────────────────────────
export type { Point, Bounds, Transform } from "./geometry";
export type { StrokePosition }           from "./stroke";

// ── Re-exports: renamed types ────────────────────────────────────────────────
export type { ConstraintH as HorizontalConstraint } from "./shape";
export type { ConstraintV as VerticalConstraint   } from "./shape";
export type { ShadowStyle as ShadowType } from "./shadow";

// ─────────────────────────────────────────────────────────────────────────────
// Scalars
// ─────────────────────────────────────────────────────────────────────────────

export type HexColor = string;

// ─────────────────────────────────────────────────────────────────────────────
// BlendMode
// ─────────────────────────────────────────────────────────────────────────────

export type BlendMode =
  | "color"       | "color-burn"  | "color-dodge" | "darken"
  | "difference"  | "exclusion"   | "hard-light"  | "hue"
  | "lighten"     | "luminosity"  | "multiply"    | "normal"
  | "overlay"     | "saturation"  | "screen"      | "soft-light";

// ─────────────────────────────────────────────────────────────────────────────
// Gradient — Malli wire format
// ─────────────────────────────────────────────────────────────────────────────

export interface GradientStop {
  position: number;
  opacity:  number;
  color:    HexColor;
}

export interface LinearGradient {
  type:   "linear";
  startX: number;
  startY: number;
  endX:   number;
  endY:   number;
  width:  number;
  stops:  GradientStop[];
}

export interface RadialGradient {
  type:   "radial";
  startX: number;
  startY: number;
  endX:   number;
  endY:   number;
  width:  number;
  stops:  GradientStop[];
}

export type Gradient = LinearGradient | RadialGradient;

// ─────────────────────────────────────────────────────────────────────────────
// Fill (image only — SolidFill / GradientFill declared in shapes.ts)
// ─────────────────────────────────────────────────────────────────────────────

export interface ImageFill {
  type:    "image";
  imageId: string;
  opacity: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Stroke — Malli wire format
// ─────────────────────────────────────────────────────────────────────────────

export type StrokeType = "solid" | "dotted" | "dashed" | "mixed";

export type StrokeCap =
  | "circle-marker" | "diamond-marker" | "line-arrow"
  | "round" | "square" | "square-marker" | "triangle-arrow";

export interface Stroke {
  color?:    HexColor;
  opacity?:  number;
  width?:    number;
  type?:     StrokeType;
  position?: StrokePosition;
  capStart?: StrokeCap;
  capEnd?:   StrokeCap;
}

// ─────────────────────────────────────────────────────────────────────────────
// Shadow — Malli wire format
// ─────────────────────────────────────────────────────────────────────────────

export interface Shadow {
  type:    ShadowStyle;
  x:       number;
  y:       number;
  blur:    number;
  spread:  number;
  color:   HexColor;
  opacity: number;
  hidden:  boolean;
}

// ─────────────────────────────────────────────────────────────────────────────
// Blur — Malli wire format
// ─────────────────────────────────────────────────────────────────────────────

export interface Blur {
  type:   "layer-blur";
  value:  number;
  hidden: boolean;
}

// ─────────────────────────────────────────────────────────────────────────────
// BoolType — Malli uses "exclude"; Rust uses "exclusion".
// ─────────────────────────────────────────────────────────────────────────────

export type BoolType = "difference" | "exclude" | "intersection" | "union";

// ─────────────────────────────────────────────────────────────────────────────
// GrowType
// ─────────────────────────────────────────────────────────────────────────────

export type GrowType = "auto-height" | "auto-width" | "fixed";

// ─────────────────────────────────────────────────────────────────────────────
// Layout (flex / grid) — not yet in logos-types
// ─────────────────────────────────────────────────────────────────────────────

export type LayoutType    = "flex" | "grid";
export type FlexDirection = "column" | "column-reverse" | "row" | "row-reverse";
export type GridDirection = "column" | "row";
export type WrapType      = "nowrap" | "wrap";

export type JustifyContent =
  | "center" | "end" | "space-around" | "space-between"
  | "space-evenly" | "start" | "stretch";

export type AlignContent =
  | "center" | "end" | "space-around" | "space-between"
  | "space-evenly" | "start" | "stretch";

export type AlignItems  = "center" | "end" | "start" | "stretch";
export type JustifyItems = "center" | "end" | "start" | "stretch";

export type GridTrackType = "auto" | "fixed" | "flex" | "percent";
export type GridPosition  = "area" | "auto" | "manual";
export type GridCellAlignSelf   = "auto" | "center" | "end" | "start" | "stretch";
export type GridCellJustifySelf = "auto" | "center" | "end" | "start" | "stretch";

export interface GridTrack {
  type:   GridTrackType;
  value?: number | null;
}

export interface GridCell {
  id:           string;
  areaName?:    string;
  row:          number;
  rowSpan:      number;
  column:       number;
  columnSpan:   number;
  position?:    GridPosition;
  alignSelf?:   GridCellAlignSelf;
  justifySelf?: GridCellJustifySelf;
  shapes:       string[];
}

export interface LayoutAttrs {
  layout?:               LayoutType;
  layoutFlexDir?:        FlexDirection;
  layoutGap?:            { rowGap?: number; columnGap?: number };
  layoutWrapType?:       WrapType;
  layoutPadding?:        { p1?: number; p2?: number; p3?: number; p4?: number };
  layoutJustifyContent?: JustifyContent;
  layoutAlignContent?:   AlignContent;
  layoutAlignItems?:     AlignItems;
  layoutJustifyItems?:   JustifyItems;
  layoutGridDir?:        GridDirection;
  layoutGridRows?:       GridTrack[];
  layoutGridColumns?:    GridTrack[];
  layoutGridCells?:      Record<string, GridCell>;
}

// ─────────────────────────────────────────────────────────────────────────────
// CanonicalShape — Malli wire format for the CRDT change protocol.
// The Rust Shape in shape.ts is structurally different (snake_case → camelCase,
// different field set).  This compat version stays until the Go backend and
// CRDT layer are migrated.
// ─────────────────────────────────────────────────────────────────────────────

/** Shape type as carried in the Malli/transit wire protocol. */
export type CanonicalShapeType =
  | "bool" | "circle" | "frame" | "group" | "image"
  | "path" | "rect"   | "svg-raw" | "text";

type CompatFill =
  | { type: "solid";    color: HexColor;   opacity: number }
  | { type: "gradient"; gradient: Gradient; opacity: number }
  | ImageFill;

/** Complete shape record as persisted in the database (Malli/transit wire format). */
export interface CanonicalShape {
  readonly id:     string;
  type:            CanonicalShapeType;
  name:            string;
  x:               number;
  y:               number;
  width:           number;
  height:          number;
  rotation:        number;
  opacity:         number;
  hidden:          boolean;
  locked:          boolean;
  blendMode?:      BlendMode;
  fills:           CompatFill[];
  strokes:         Stroke[];
  shadows:         Shadow[];
  blur?:           Blur;
  constraintsH?:   ConstraintH;
  constraintsV?:   ConstraintV;
  transform:       import("./geometry").Transform;
  parentId:        string | null;
  frameId:         string | null;
  children:        string[];
  layout?:         LayoutType;
  layoutFlexDir?:  FlexDirection;
  layoutWrapType?: WrapType;
  layoutJustifyContent?: JustifyContent;
  layoutAlignContent?:   AlignContent;
  layoutAlignItems?:     AlignItems;
}
