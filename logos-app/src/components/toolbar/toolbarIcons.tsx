/**
 * Maps toolbar tool ids → svger-cli React icon components.
 * Source SVGs live in src/icons/toolbar/ (Font Awesome solid, from mcp/icons/solid).
 */

import React from "react";
import type { ComponentType, SVGProps } from "react";
import type { Tool } from "../../stores/uiStore";
import type { BoolOp } from "../../worker/vector-network.types";
import {
  Arrow,
  BoolExclude,
  BoolIntersect,
  BoolSubtract,
  BoolUnion,
  ChevronDown,
  Dev,
  Ellipse,
  Frame,
  Hand,
  ImageImport,
  Line,
  Path,
  Polygon,
  Prototype,
  Rect,
  ResetView,
  Scale,
  Select,
  Selection,
  Slice,
  Star,
  Text,
} from "../icons/index";

export type ToolbarIconComponent = ComponentType<
  SVGProps<SVGSVGElement> & { size?: number | string }
>;

export const TOOLBAR_ICONS = {
  select: Select,
  hand: Hand,
  scale: Scale,
  frame: Frame,
  selection: Selection,
  slice: Slice,
  rect: Rect,
  line: Line,
  arrow: Arrow,
  ellipse: Ellipse,
  polygon: Polygon,
  star: Star,
  imageImport: ImageImport,
  text: Text,
  path: Path,
  prototype: Prototype,
  dev: Dev,
  chevronDown: ChevronDown,
  resetView: ResetView,
} as const satisfies Record<string, ToolbarIconComponent>;

export type ToolbarIconName = keyof typeof TOOLBAR_ICONS;

export const BOOL_OP_ICONS: Record<BoolOp, ToolbarIconComponent> = {
  union: BoolUnion,
  intersect: BoolIntersect,
  subtract: BoolSubtract,
  exclude: BoolExclude,
};

/** Tool ids that have a dedicated toolbar icon. */
export type ToolIconName = Extract<Tool, ToolbarIconName>;

export function isToolIconName(id: Tool): id is ToolIconName {
  return id in TOOLBAR_ICONS;
}

interface ToolbarIconProps {
  name: ToolbarIconName;
  size?: number;
  className?: string;
}

export function ToolbarIcon({
  name,
  size = 18,
  className,
}: ToolbarIconProps): React.ReactElement | null {
  const Icon = TOOLBAR_ICONS[name];
  if (!Icon) return null;
  return <Icon size={size} className={className} aria-hidden focusable={false} />;
}
