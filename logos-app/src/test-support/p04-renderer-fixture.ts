import type { Page } from "../stores/documentStore";
import type { Shape } from "../types/shapes";

export const P04_RENDERER_ADAPTER_VERSION = "legacy-fixture-to-react-rect/v1" as const;
const NIL_UUID = "00000000-0000-0000-0000-000000000000";

export interface P04RendererProjection {
  adapterVersion: typeof P04_RENDERER_ADAPTER_VERSION;
  documentId: string;
  pages: Record<string, Page>;
  pageOrder: string[];
  currentPageId: string;
  shapes: Record<string, Shape>;
  unknownFieldsById: Record<string, Record<string, unknown>>;
}

export class UnsupportedP04RendererShapeError extends Error {
  readonly shapeType: string;

  constructor(shapeType: string) {
    super(`P04 renderer fixture adapter does not support ${shapeType}`);
    this.name = "UnsupportedP04RendererShapeError";
    this.shapeType = shapeType;
  }
}

/** Test-only projection for the committed one-rectangle fixture; never production state. */
export function projectP04RendererFixture(input: unknown): P04RendererProjection {
  const data = input as Record<string, unknown>;
  const documentId = data.id as string;
  const pageOrder = data.pages as string[];
  const pageIndex = data["pages-index"] as Record<string, Record<string, unknown>>;
  const pageId = pageOrder?.[0];
  const pageData = pageIndex?.[pageId];
  const objects = pageData?.objects as Record<string, Record<string, unknown>>;
  const root = objects?.[NIL_UUID];
  if (!documentId || !pageId || root?.id !== NIL_UUID || !Array.isArray(root.shapes)) {
    throw new Error("P04 fixture is missing its document/page/nil-root contract");
  }

  const rootShapeIds = root.shapes as string[];
  const shapes: Record<string, Shape> = {};
  const unknownFieldsById: Record<string, Record<string, unknown>> = {};
  for (const shapeId of rootShapeIds) {
    const legacy = objects[shapeId];
    if (!legacy || legacy.id !== shapeId || legacy["parent-id"] !== NIL_UUID) {
      throw new Error(`P04 fixture root membership is invalid for ${shapeId}`);
    }
    const sourceType = String(legacy.type).replace(/^~:/, "");
    if (sourceType !== "rect") throw new UnsupportedP04RendererShapeError(sourceType);
    const rect = legacy.selrect as Record<string, number>;
    const fills = legacy.fills as Array<Record<string, unknown>>;
    const fill = fills?.[0];
    if (!rect || !fill || typeof fill["fill-color"] !== "string") {
      throw new Error(`P04 rectangle ${shapeId} is missing geometry or solid fill`);
    }
    shapes[shapeId] = {
      id: shapeId,
      type: "rect",
      name: String(legacy.name),
      bounds: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
      transform: [1, 0, 0, 1, 0, 0],
      rotation: Number(legacy.rotation ?? 0),
      fills: [{ type: "solid", color: String(fill["fill-color"]), opacity: Number(fill["fill-opacity"] ?? 1) }],
      opacity: Number(legacy.opacity ?? 1),
      hidden: legacy.hidden === true,
      locked: legacy.locked === true,
      parentId: null,
      children: [],
    };
    const known = new Set([
      "id", "type", "name", "parent-id", "frame-id", "shapes", "x", "y", "width", "height",
      "selrect", "transform", "transform-inverse", "rotation", "opacity", "hidden", "locked", "fills", "strokes",
    ]);
    unknownFieldsById[shapeId] = Object.fromEntries(Object.entries(legacy).filter(([key]) => !known.has(key)));
  }

  return {
    adapterVersion: P04_RENDERER_ADAPTER_VERSION,
    documentId,
    pages: { [pageId]: { id: pageId, name: String(pageData.name), rootShapeIds: [...rootShapeIds] } },
    pageOrder: [...pageOrder],
    currentPageId: pageId,
    shapes,
    unknownFieldsById,
  };
}
