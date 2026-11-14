import type { Page } from "../stores/documentStore";
import type { Shape, Transform } from "../types/shapes";

export const G1_B3_ADAPTER_VERSION = "react-document-to-rust/v1" as const;

export interface ReactDocumentSnapshot {
  /** Supplied by the caller; the current React store has no document id. */
  documentId: string;
  pages: Record<string, Page>;
  pageOrder: string[];
  shapes: Record<string, Shape>;
}

export interface RustDiagnostic {
  code: string;
  path: string;
  objectId?: string;
  relatedId?: string;
}

export interface RustValidationResult {
  ok: boolean;
  document?: Record<string, unknown>;
  diagnostics: RustDiagnostic[];
}

export interface RustDocumentValidator {
  validateDocumentJson(json: string): string;
}

export type AdapterIssueCode =
  | "INVALID_SNAPSHOT"
  | "IDENTITY_REQUIRED"
  | "UNSUPPORTED_PAGE_ID"
  | "UNSUPPORTED_SHAPE"
  | "UNSUPPORTED_PROPERTY"
  | "UNSUPPORTED_FILL"
  | "INVALID_GEOMETRY"
  | "INVALID_TRANSFORM"
  | "UNVERIFIED_COORDINATE_SPACE"
  | "INVALID_RUST_RESPONSE";

export interface AdapterIssue {
  code: AdapterIssueCode;
  path: string;
}

export type ReactAdapterResult =
  | {
      ok: true;
      adapterVersion: typeof G1_B3_ADAPTER_VERSION;
      candidate: Record<string, unknown>;
      roundTrip: ReactDocumentSnapshot;
      diagnostics: [];
      timingsMs: { serialize: number; rustValidation: number };
      wasmMemoryBytes: number | null;
    }
  | {
      ok: false;
      adapterVersion: typeof G1_B3_ADAPTER_VERSION;
      issues: AdapterIssue[];
      diagnostics: RustDiagnostic[];
      timingsMs: { serialize: number; rustValidation: number };
      wasmMemoryBytes: number | null;
    };

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const SHAPE_KEYS = new Set([
  "id", "type", "name", "bounds", "transform", "rotation", "fills", "opacity",
  "hidden", "locked", "parentId", "children",
  // Known React properties outside this milestone. Presence with a value is
  // rejected below; listing them keeps diagnostics specific.
  "fontVariationSettings", "vnAnchors", "vnSegments", "vnRegions", "componentMeta",
  "instanceMeta", "text", "fontFamily", "fontWeight", "fontSize", "textColor",
  "textOpacity", "lineHeight", "letterSpacing", "textAlign", "textDecoration",
  "layout", "layoutFlexDir", "layoutWrapType", "layoutJustifyContent",
  "layoutAlignItems", "layoutGap", "layoutPadding",
]);

function issue(code: AdapterIssueCode, path: string): AdapterIssue {
  return { code, path };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function inverse(matrix: Transform): Transform | null {
  const [a, b, c, d, e, f] = matrix;
  const determinant = a * d - b * c;
  if (!Number.isFinite(determinant) || Math.abs(determinant) < 1e-12) return null;
  const values: Transform = [d / determinant, -b / determinant, -c / determinant, a / determinant,
    (c * f - d * e) / determinant, (b * e - a * f) / determinant];
  return values.map((value) => Object.is(value, -0) ? 0 : value) as Transform;
}

function matrixObject(values: Transform): Record<string, number> {
  const [a, b, c, d, e, f] = values;
  return { a, b, c, d, e, f };
}

function isDefaultUnsupported(key: string, value: unknown): boolean {
  // Optional unsupported fields absent/undefined are harmless; an explicit
  // value, including null or an empty collection, remains observable and is
  // rejected rather than silently discarded.
  return SHAPE_KEYS.has(key) && key !== "id" && key !== "type" && key !== "name" &&
    key !== "bounds" && key !== "transform" && key !== "rotation" && key !== "fills" &&
    key !== "opacity" && key !== "hidden" && key !== "locked" &&
    key !== "parentId" && key !== "children" && value !== undefined;
}

function deriveFrameId(shape: Shape, shapes: Record<string, Shape>): string | null {
  const seen = new Set<string>();
  let parentId = shape.parentId;
  while (parentId) {
    if (seen.has(parentId)) return null; // Rust reports the graph cycle.
    seen.add(parentId);
    const parent = shapes[parentId];
    if (!parent) return null; // Rust reports the missing parent.
    if (parent.type === "frame") return parent.id;
    parentId = parent.parentId;
  }
  return null;
}

function hasEquivalentParentSpace(shape: Shape, shapes: Record<string, Shape>): boolean {
  const seen = new Set<string>();
  let parentId = shape.parentId;
  while (parentId) {
    if (seen.has(parentId)) return true; // Rust owns cycle diagnostics.
    seen.add(parentId);
    const parent = shapes[parentId];
    if (!parent) return true; // Rust owns missing-parent diagnostics.
    if (parent.bounds.x !== 0 || parent.bounds.y !== 0 ||
        parent.transform.some((value, index) => value !== [1, 0, 0, 1, 0, 0][index])) return false;
    parentId = parent.parentId;
  }
  return true;
}

type FillConversion = { ok: true; value: Record<string, unknown> } | { ok: false; issue: AdapterIssue };

function fillToRust(fill: unknown, path: string): FillConversion {
  if (!isRecord(fill) || fill.type !== "solid" ||
      Object.keys(fill).some((key) => !["type", "color", "opacity"].includes(key)) ||
      typeof fill.color !== "string" || typeof fill.opacity !== "number" || !Number.isFinite(fill.opacity)) {
    return { ok: false, issue: issue("UNSUPPORTED_FILL", path) };
  }
  return { ok: true, value: { "fill-type": "solid", "fill-color": fill.color, "fill-opacity": fill.opacity } };
}

/** Convert a one-page Zustand-shaped snapshot to the candidate Rust envelope. */
export function toRustCandidate(input: unknown):
  | { ok: true; candidate: Record<string, unknown> }
  | { ok: false; issues: AdapterIssue[] } {
  const issues: AdapterIssue[] = [];
  if (!isRecord(input)) return { ok: false, issues: [issue("INVALID_SNAPSHOT", "/")] };
  for (const key of Object.keys(input)) {
    if (!["documentId", "pages", "pageOrder", "shapes"].includes(key)) {
      issues.push(issue("UNSUPPORTED_PROPERTY", `/${key}`));
    }
  }
  const documentId = input.documentId;
  if (typeof documentId !== "string" || !UUID.test(documentId)) {
    issues.push(issue("IDENTITY_REQUIRED", "/documentId"));
  }
  if (!Array.isArray(input.pageOrder) || input.pageOrder.length !== 1 ||
      typeof input.pageOrder[0] !== "string" || !isRecord(input.pages) || !isRecord(input.shapes)) {
    return { ok: false, issues: [...issues, issue("INVALID_SNAPSHOT", "/pages")] };
  }
  const pageId = input.pageOrder[0];
  if (Object.keys(input.pages).length !== 1 || Object.keys(input.pages)[0] !== pageId) {
    issues.push(issue("INVALID_SNAPSHOT", "/pages"));
  }
  const pages = input.pages as Record<string, unknown>;
  const page = pages[pageId];
  let pageRecord: { id: string; name: string; rootShapeIds: string[] } | null = null;
  if (!isRecord(page) || page.id !== pageId || typeof page.name !== "string" ||
      !Array.isArray(page.rootShapeIds) || page.rootShapeIds.some((id) => typeof id !== "string")) {
    issues.push(issue("INVALID_SNAPSHOT", `/pages/${pageId}`));
  } else {
    pageRecord = { id: pageId, name: page.name, rootShapeIds: page.rootShapeIds as string[] };
    if (!UUID.test(pageId)) issues.push(issue("UNSUPPORTED_PAGE_ID", `/pages/${pageId}/id`));
  }
  if (isRecord(page)) {
    for (const key of Object.keys(page)) {
      if (!["id", "name", "rootShapeIds"].includes(key)) {
        issues.push(issue("UNSUPPORTED_PROPERTY", `/pages/${pageId}/${key}`));
      }
    }
  }
  const shapes = input.shapes as Record<string, Shape>;
  const shapeRecords: Record<string, unknown>[] = [];
  for (const [key, raw] of Object.entries(shapes)) {
    const path = `/shapes/${key}`;
    if (!isRecord(raw) || raw.id !== key) {
      issues.push(issue("INVALID_SNAPSHOT", path));
      continue;
    }
    const shape = raw as unknown as Shape;
    if (!UUID.test(shape.id)) issues.push(issue("IDENTITY_REQUIRED", `${path}/id`));
    for (const [field, value] of Object.entries(raw)) {
      if (!SHAPE_KEYS.has(field)) issues.push(issue("UNSUPPORTED_PROPERTY", `${path}/${field}`));
      else if (isDefaultUnsupported(field, value)) issues.push(issue("UNSUPPORTED_PROPERTY", `${path}/${field}`));
    }
    const type = shape.type === "rect" ? "rect" : shape.type === "frame" ? "frame" :
      shape.type === "group" ? "group" : null;
    if (!type) issues.push(issue("UNSUPPORTED_SHAPE", `${path}/type`));
    const bounds = shape.bounds;
    if (!bounds || ![bounds.x, bounds.y, bounds.w, bounds.h].every(Number.isFinite) ||
        bounds.w < 0 || bounds.h < 0) {
      issues.push(issue("INVALID_GEOMETRY", `${path}/bounds`));
    }
    const transform = shape.transform;
    if (!Array.isArray(transform) || transform.length !== 6 || !transform.every(Number.isFinite)) {
      issues.push(issue("INVALID_TRANSFORM", `${path}/transform`));
    }
    const transformInverse = Array.isArray(transform) ? inverse(transform as Transform) : null;
    if (!transformInverse) issues.push(issue("INVALID_TRANSFORM", `${path}/transform`));
    if (!Array.isArray(shape.fills)) issues.push(issue("UNSUPPORTED_FILL", `${path}/fills`));
    const fills = Array.isArray(shape.fills)
      ? shape.fills.map((fill, index) => fillToRust(fill, `${path}/fills/${index}`)) : [];
    for (const fill of fills) if (!fill.ok) issues.push(fill.issue);
    if (!Array.isArray(shape.children) || shape.children.some((id) => typeof id !== "string")) {
      issues.push(issue("INVALID_SNAPSHOT", `${path}/children`));
    }
    if (!shape.parentId && shape.parentId !== null) issues.push(issue("INVALID_SNAPSHOT", `${path}/parentId`));
    if (shape.parentId && !hasEquivalentParentSpace(shape, shapes)) {
      issues.push(issue("UNVERIFIED_COORDINATE_SPACE", `${path}/bounds`));
    }
    if (typeof shape.name !== "string" || typeof shape.hidden !== "boolean" ||
        typeof shape.locked !== "boolean" || !Number.isFinite(shape.rotation) ||
        !Number.isFinite(shape.opacity)) issues.push(issue("INVALID_SNAPSHOT", path));
    if (type && transformInverse && bounds && Array.isArray(shape.children)) {
      const extensions = shape.locked === false ? { "react-locked": false } : { "react-locked": true };
      shapeRecords.push({
        "page-id": pageId,
        id: shape.id,
        type,
        name: shape.name,
        selrect: { x: bounds.x, y: bounds.y, width: bounds.w, height: bounds.h },
        transform: matrixObject(transform as Transform),
        "transform-inverse": matrixObject(transformInverse),
        rotation: shape.rotation,
        "parent-id": shape.parentId,
        "frame-id": deriveFrameId(shape, shapes),
        shapes: [...shape.children],
        hidden: shape.hidden,
        opacity: shape.opacity,
        "masked-group": false,
        fills: fills.filter((fill) => fill.ok).map((fill) => fill.value),
        "react-locked": shape.locked,
      });
    }
  }
  if (issues.length > 0 || pageRecord === null) return { ok: false, issues: issues.length ? issues : [issue("INVALID_SNAPSHOT", "/pages")] };
  return {
    ok: true,
    candidate: {
      id: documentId,
      "schema-version": 1,
      pages: [{ id: pageRecord.id, name: pageRecord.name, "root-shape-ids": [...pageRecord.rootShapeIds] }],
      shapes: shapeRecords,
    },
  };
}

function fromRustDocument(document: Record<string, unknown>): ReactDocumentSnapshot {
  const pages = document.pages as Array<Record<string, unknown>>;
  const records = document.shapes as Array<Record<string, unknown>>;
  const page = pages[0];
  const pageId = page.id as string;
  const shapes: Record<string, Shape> = {};
  for (const record of records) {
    const id = record.id as string;
    const rustFills = (record.fills as Array<Record<string, unknown>> | undefined) ?? [];
    const locked = record["react-locked"];
    if (typeof locked !== "boolean") throw new Error(`Missing locked extension for ${id}`);
    const matrix = record.transform as Record<string, number>;
    const selrect = record.selrect as Record<string, number>;
    shapes[id] = {
      id,
      type: record.type as Shape["type"],
      name: record.name as string,
      bounds: { x: selrect.x, y: selrect.y, w: selrect.width, h: selrect.height },
      transform: [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f],
      rotation: record.rotation as number,
      fills: rustFills.map((fill) => ({
        type: "solid" as const,
        color: fill["fill-color"] as string,
        opacity: fill["fill-opacity"] as number,
      })),
      opacity: record.opacity as number,
      hidden: record.hidden as boolean,
      locked,
      parentId: (record["parent-id"] as string | null | undefined) ?? null,
      children: [...(record.shapes as string[] ?? [])],
    };
  }
  return {
    documentId: document.id as string,
    pages: { [pageId]: { id: pageId, name: page.name as string,
      rootShapeIds: [...page["root-shape-ids"] as string[]] } },
    pageOrder: [pageId],
    shapes,
  };
}

/** One-shot read-only conversion, Rust validation, and React projection. */
export function validateReactSnapshot(
  input: ReactDocumentSnapshot,
  rust: RustDocumentValidator,
  wasmMemoryBytes: () => number | null = () => null,
): ReactAdapterResult {
  const serializationStart = performance.now();
  const converted = toRustCandidate(input);
  const json = converted.ok ? JSON.stringify(converted.candidate) : null;
  const serialize = performance.now() - serializationStart;
  if (!converted.ok || json === null) return {
    ok: false, adapterVersion: G1_B3_ADAPTER_VERSION, issues: converted.ok ? [issue("INVALID_SNAPSHOT", "/")] : converted.issues,
    diagnostics: [], timingsMs: { serialize, rustValidation: 0 }, wasmMemoryBytes: wasmMemoryBytes(),
  };
  const validationStart = performance.now();
  let result: RustValidationResult;
  try {
    result = JSON.parse(rust.validateDocumentJson(json)) as RustValidationResult;
  } catch {
    return { ok: false, adapterVersion: G1_B3_ADAPTER_VERSION,
      issues: [issue("INVALID_RUST_RESPONSE", "/")], diagnostics: [],
      timingsMs: { serialize, rustValidation: performance.now() - validationStart }, wasmMemoryBytes: wasmMemoryBytes() };
  }
  const rustValidation = performance.now() - validationStart;
  if (!result || typeof result.ok !== "boolean" || !Array.isArray(result.diagnostics)) {
    return { ok: false, adapterVersion: G1_B3_ADAPTER_VERSION,
      issues: [issue("INVALID_RUST_RESPONSE", "/")], diagnostics: [],
      timingsMs: { serialize, rustValidation }, wasmMemoryBytes: wasmMemoryBytes() };
  }
  if (!result.ok || !result.document) return {
    ok: false, adapterVersion: G1_B3_ADAPTER_VERSION, issues: [], diagnostics: result.diagnostics,
    timingsMs: { serialize, rustValidation }, wasmMemoryBytes: wasmMemoryBytes(),
  };
  return {
    ok: true, adapterVersion: G1_B3_ADAPTER_VERSION, candidate: result.document,
    roundTrip: fromRustDocument(result.document), diagnostics: [],
    timingsMs: { serialize, rustValidation }, wasmMemoryBytes: wasmMemoryBytes(),
  };
}
