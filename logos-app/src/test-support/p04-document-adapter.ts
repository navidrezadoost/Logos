import type { Page } from "../stores/documentStore";
import type { Shape, Transform } from "../types/shapes";

export const P04_ADAPTER_VERSION = "legacy-filedata-to-react/v1" as const;
export const NIL_UUID = "00000000-0000-0000-0000-000000000000";

type JsonObject = Record<string, unknown>;

export interface P04ReactProjection {
  adapterVersion: typeof P04_ADAPTER_VERSION;
  documentId: string;
  pages: Record<string, Page>;
  pageOrder: string[];
  currentPageId: string;
  shapes: Record<string, Shape>;
  /** Test-only sidecar; not a production persistence model. */
  unknownFieldsById: Record<string, JsonObject>;
}

export type P04AdapterErrorCode =
  | "INVALID_DOCUMENT"
  | "INVALID_HIERARCHY"
  | "UNSUPPORTED_SHAPE"
  | "UNSUPPORTED_FILL"
  | "INVALID_GEOMETRY";

export class P04AdapterError extends Error {
  readonly code: P04AdapterErrorCode;

  constructor(code: P04AdapterErrorCode, message: string) {
    super(message);
    this.name = "P04AdapterError";
    this.code = code;
  }
}

const KNOWN_SHAPE_FIELDS = new Set([
  "id", "type", "name", "parent-id", "frame-id", "shapes", "x", "y",
  "width", "height", "selrect", "transform", "transform-inverse", "rotation",
  "opacity", "hidden", "locked", "fills", "strokes", "blend-mode",
  "constraints-h", "constraints-v", "points", "r1", "r2", "r3", "r4",
]);

function object(value: unknown, label: string): JsonObject {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new P04AdapterError("INVALID_DOCUMENT", `${label} must be an object`);
  }
  return value as JsonObject;
}

function numberField(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new P04AdapterError("INVALID_GEOMETRY", `${label} must be a finite number`);
  }
  return value;
}

function stringField(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new P04AdapterError("INVALID_DOCUMENT", `${label} must be a nonempty string`);
  }
  return value;
}

function stringList(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== "string")) {
    throw new P04AdapterError("INVALID_HIERARCHY", `${label} must be an ordered string array`);
  }
  return [...value];
}

function normalizedType(value: unknown, label: string): string {
  const raw = stringField(value, label);
  return raw.startsWith("~:") ? raw.slice(2) : raw;
}

function transform(value: unknown): Transform {
  const matrix = object(value, "shape.transform");
  return [
    numberField(matrix.a, "transform.a"),
    numberField(matrix.b, "transform.b"),
    numberField(matrix.c, "transform.c"),
    numberField(matrix.d, "transform.d"),
    numberField(matrix.e, "transform.e"),
    numberField(matrix.f, "transform.f"),
  ];
}

function unknownFields(shape: JsonObject): JsonObject {
  return Object.fromEntries(Object.entries(shape).filter(([key]) => !KNOWN_SHAPE_FIELDS.has(key)));
}

/**
 * P04's deliberately narrow test adapter. It accepts the committed legacy-shaped
 * rectangle vector, maps nil-root membership to React page roots, and rejects
 * unsupported kinds instead of manufacturing a rectangle.
 */
export function projectLegacyFileData(input: unknown): P04ReactProjection {
  const data = object(input, "file.data");
  const documentId = stringField(data.id, "file.data.id");
  const pageOrder = stringList(data.pages, "file.data.pages");
  const pageIndex = object(data["pages-index"], "file.data.pages-index");
  if (pageOrder.length === 0) {
    throw new P04AdapterError("INVALID_HIERARCHY", "document has no ordered pages");
  }

  const pages: Record<string, Page> = {};
  const shapes: Record<string, Shape> = {};
  const unknownFieldsById: Record<string, JsonObject> = {};

  for (const pageId of pageOrder) {
    const pageData = object(pageIndex[pageId], `pages-index.${pageId}`);
    if (pageData.id !== pageId) {
      throw new P04AdapterError("INVALID_HIERARCHY", `page index/id mismatch for ${pageId}`);
    }
    const objects = object(pageData.objects, `pages-index.${pageId}.objects`);
    const root = object(objects[NIL_UUID], `pages-index.${pageId}.objects.nil-root`);
    if (root.id !== NIL_UUID || normalizedType(root.type, "nil-root.type") !== "frame") {
      throw new P04AdapterError("INVALID_HIERARCHY", "expected a nil-UUID root frame");
    }
    const rootShapeIds = stringList(root.shapes, "nil-root.shapes");
    unknownFieldsById[NIL_UUID] = unknownFields(root);

    for (const shapeId of rootShapeIds) {
      const legacy = object(objects[shapeId], `objects.${shapeId}`);
      if (legacy.id !== shapeId) {
        throw new P04AdapterError("INVALID_HIERARCHY", `object key/id mismatch for ${shapeId}`);
      }
      const type = normalizedType(legacy.type, `${shapeId}.type`);
      if (type !== "rect") {
        throw new P04AdapterError("UNSUPPORTED_SHAPE", `P04 adapter does not support ${type}`);
      }
      const parentId = stringField(legacy["parent-id"], `${shapeId}.parent-id`);
      if (parentId !== NIL_UUID) {
        throw new P04AdapterError("INVALID_HIERARCHY", `fixture rectangle ${shapeId} is not a nil-root child`);
      }
      const children = stringList(legacy.shapes ?? [], `${shapeId}.shapes`);
      if (children.length !== 0) {
        throw new P04AdapterError("UNSUPPORTED_SHAPE", "P04 rectangle fixture must be a leaf shape");
      }

      const rect = object(legacy.selrect, `${shapeId}.selrect`);
      const x = numberField(rect.x, `${shapeId}.selrect.x`);
      const y = numberField(rect.y, `${shapeId}.selrect.y`);
      const w = numberField(rect.width, `${shapeId}.selrect.width`);
      const h = numberField(rect.height, `${shapeId}.selrect.height`);
      if (x !== numberField(legacy.x, `${shapeId}.x`) || y !== numberField(legacy.y, `${shapeId}.y`) ||
          w !== numberField(legacy.width, `${shapeId}.width`) || h !== numberField(legacy.height, `${shapeId}.height`)) {
        throw new P04AdapterError("INVALID_GEOMETRY", `redundant geometry fields disagree for ${shapeId}`);
      }

      const fills = legacy.fills;
      if (!Array.isArray(fills) || fills.length !== 1) {
        throw new P04AdapterError("UNSUPPORTED_FILL", `expected one solid fill for ${shapeId}`);
      }
      const legacyFill = object(fills[0], `${shapeId}.fills[0]`);
      const color = stringField(legacyFill["fill-color"], `${shapeId}.fills[0].fill-color`);
      const opacity = numberField(legacyFill["fill-opacity"], `${shapeId}.fills[0].fill-opacity`);

      shapes[shapeId] = {
        id: shapeId,
        type: "rect",
        name: stringField(legacy.name, `${shapeId}.name`),
        bounds: { x, y, w, h },
        transform: transform(legacy.transform),
        rotation: typeof legacy.rotation === "number" ? legacy.rotation : 0,
        fills: [{ type: "solid", color, opacity }],
        opacity: typeof legacy.opacity === "number" ? legacy.opacity : 1,
        hidden: legacy.hidden === true,
        locked: legacy.locked === true,
        parentId: null,
        children,
      };
      unknownFieldsById[shapeId] = unknownFields(legacy);
    }

    pages[pageId] = {
      id: pageId,
      name: stringField(pageData.name, `pages-index.${pageId}.name`),
      rootShapeIds,
    };
  }

  return {
    adapterVersion: P04_ADAPTER_VERSION,
    documentId,
    pages,
    pageOrder,
    currentPageId: pageOrder[0],
    shapes,
    unknownFieldsById,
  };
}

export interface P04GeometryChange {
  type: "mod-obj";
  id: string;
  "page-id": string;
  operations: Array<{ attr: string; val: unknown }>;
}

/** Encodes only the supported geometry edit using the existing Go mod-obj wire shape. */
export function encodeGeometryChange(
  projection: P04ReactProjection,
  shapeId: string,
  bounds: Shape["bounds"],
): P04GeometryChange {
  const shape = projection.shapes[shapeId];
  if (!shape || shape.type !== "rect") {
    throw new P04AdapterError("UNSUPPORTED_SHAPE", `cannot encode geometry for ${shapeId}`);
  }
  const { x, y, w, h } = bounds;
  const selrect = { x, y, width: w, height: h, x1: x, y1: y, x2: x + w, y2: y + h };
  return {
    type: "mod-obj",
    id: shapeId,
    "page-id": projection.currentPageId,
    operations: [
      { attr: "x", val: x },
      { attr: "y", val: y },
      { attr: "width", val: w },
      { attr: "height", val: h },
      { attr: "selrect", val: selrect },
    ],
  };
}
