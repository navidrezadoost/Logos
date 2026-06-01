/**
 * plugins/types.ts — re-export shim
 *
 * All public plugin types are now auto-generated from Malli schemas and live in
 * `@logos/plugin-types`. This file re-exports the full surface so that internal
 * code that imports from `./types` continues to work without changes.
 *
 * To update the generated types:
 *   bin/generate-plugin-types
 */
export type {
  // Geometry
  Point,
  Rect,
  Transform,

  // Shape
  ShapeType,
  BlendMode,
  HorizontalConstraint,
  VerticalConstraint,
  BoolType,
  Shape,
  ShapePatch,

  // Fills
  HexColor,
  GradientStop,
  LinearGradient,
  RadialGradient,
  Gradient,
  SolidFill,
  GradientFill,
  ImageFill,
  Fill,

  // Stroke
  StrokeCap,
  StrokeType,
  StrokePosition,
  Stroke,

  // Shadow / blur
  ShadowType,
  Shadow,
  Blur,

  // Page
  Page,

  // Permissions + registry
  PluginPermission,
  PluginRegistryEntry,

  // Wire protocol
  PluginRequest,
  PluginResponse,
  PluginEvent,
  HostMessage,
  PluginMessage,

  // Plugin handle
  PluginHandle,

  // Plugin API
  PluginApiMethods,
  TypedCall,
  LogosPluginGlobal,
  GrowType,
} from "@logos/plugin-types";

// ---------------------------------------------------------------------------
// Internal bridge type — not part of the public package
// ---------------------------------------------------------------------------

/** Tracks an in-flight host→plugin request with its resolve/reject pair. */
export interface PendingRequest {
  resolve: (data: unknown) => void;
  reject: (err: Error) => void;
  timeoutId: ReturnType<typeof setTimeout>;
}

// ---------------------------------------------------------------------------
// Backward-compat aliases for code that used the old Logos-style names
// ---------------------------------------------------------------------------

export type { Shape as PluginShape } from "@logos/plugin-types";
export type { Fill as PluginFill } from "@logos/plugin-types";
export type { Page as PluginPage } from "@logos/plugin-types";
export type { ShapeType as PluginShapeType } from "@logos/plugin-types";
