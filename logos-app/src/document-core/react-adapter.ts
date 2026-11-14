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

export interface AdapterTimings {
  mapping: number;
  geometryConversion: number;
  serialization: number;
  wasmValidationBoundary: number;
  rustResponseParse: number;
  resultMaterialization: number;
}

type TimedConversion = { mapping: number; geometryConversion: number };

function conversionTimings(start: number, geometryConversion: number): TimedConversion {
  return { mapping: Math.max(0, performance.now() - start - geometryConversion), geometryConversion };
}

export type ReactAdapterResult =
  | {
      ok: true;
      adapterVersion: typeof G1_B3_ADAPTER_VERSION;
      candidate: Record<string, unknown>;
      roundTrip: ReactDocumentSnapshot;
      diagnostics: [];
      timingsMs: AdapterTimings;
      wasmMemoryBytes: number | null;
    }
  | {
      ok: false;
      adapterVersion: typeof G1_B3_ADAPTER_VERSION;
      issues: AdapterIssue[];
      diagnostics: RustDiagnostic[];
      timingsMs: AdapterTimings;
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

function deriveFrameIds(shapes: Record<string, Shape>): Map<string, string | null> {
  const membership = new Map<string, string | null>();
  for (const startId of Object.keys(shapes)) {
    if (membership.has(startId)) continue;
    const path: string[] = [];
    const seen = new Set<string>();
    let cursor: string | null = startId;
    while (cursor && !membership.has(cursor) && !seen.has(cursor)) {
      const current: Shape | undefined = shapes[cursor];
      if (!current) break; // Rust owns missing-parent diagnostics.
      seen.add(cursor);
      path.push(cursor);
      cursor = current.parentId;
    }
    // Resolve nearest enclosing Frame from parent links, caching every visited
    // node so a deep document is processed once rather than walking all
    // ancestors independently for each shape.
    for (const id of path.reverse()) {
      const current: Shape = shapes[id];
      const parentId = current.parentId;
      const parent = parentId ? shapes[parentId] : undefined;
      membership.set(id, parent?.type === "frame" ? parent.id : parentId ? membership.get(parentId) ?? null : null);
    }
  }
  return membership;
}

function hasSupportedParentSpace(
  shape: Shape,
  shapes: Record<string, Shape>,
  cache: Map<string, boolean>,
): boolean {
  const path: string[] = [];
  const seen = new Set<string>();
  let parentId: string | null = shape.id;
  let supported = true;
  while (parentId) {
    const known = cache.get(parentId);
    if (known !== undefined) {
      supported = known;
      break;
    }
    if (seen.has(parentId)) break; // Rust owns cycle diagnostics.
    seen.add(parentId);
    const current: Shape | undefined = shapes[parentId];
    if (!current) break; // Rust owns missing-parent diagnostics.
    // React currently stores child bounds in page coordinates. Pure parent
    // translations can be normalized exactly; rotation/scale/skew require
    // source transform and pivot semantics that remain unverified.
    const hasUnverifiedTransform = current.rotation !== 0 || current.transform.some((value, index) =>
      value !== [1, 0, 0, 1, 0, 0][index]);
    if (current.id !== shape.id && hasUnverifiedTransform) {
      supported = false;
      break;
    }
    path.push(current.id);
    parentId = current.parentId;
  }
  for (const id of path) cache.set(id, supported);
  cache.set(shape.id, supported);
  return supported;
}

function parentLocalBounds(shape: Shape, shapes: Record<string, Shape>): Shape["bounds"] {
  if (!shape.parentId) return { ...shape.bounds };
  const parent = shapes[shape.parentId];
  if (!parent) return { ...shape.bounds }; // Keep missing-parent diagnostics in Rust.
  return {
    ...shape.bounds,
    x: shape.bounds.x - parent.bounds.x,
    y: shape.bounds.y - parent.bounds.y,
  };
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
  | ({ ok: true; candidate: Record<string, unknown> } & TimedConversion)
  | ({ ok: false; issues: AdapterIssue[] } & TimedConversion) {
  const mappingStart = performance.now();
  let geometryConversion = 0;
  const issues: AdapterIssue[] = [];
  if (!isRecord(input)) return { ok: false, issues: [issue("INVALID_SNAPSHOT", "/")], ...conversionTimings(mappingStart, geometryConversion) };
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
    return { ok: false, issues: [...issues, issue("INVALID_SNAPSHOT", "/pages")], ...conversionTimings(mappingStart, geometryConversion) };
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
  const parentSpaceCache = new Map<string, boolean>();
  const frameIdByShape = deriveFrameIds(shapes);
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
    if (shape.parentId && !hasSupportedParentSpace(shape, shapes, parentSpaceCache)) {
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
        selrect: (() => {
          const geometryStart = performance.now();
          const local = parentLocalBounds(shape, shapes);
          geometryConversion += performance.now() - geometryStart;
          return { x: local.x, y: local.y, width: local.w, height: local.h };
        })(),
        transform: matrixObject(transform as Transform),
        "transform-inverse": matrixObject(transformInverse),
        rotation: shape.rotation,
        "parent-id": shape.parentId,
        "frame-id": frameIdByShape.get(shape.id) ?? null,
        shapes: [...shape.children],
        hidden: shape.hidden,
        opacity: shape.opacity,
        "masked-group": false,
        fills: fills.filter((fill) => fill.ok).map((fill) => fill.value),
        "react-locked": shape.locked,
      });
    }
  }
  if (issues.length > 0 || pageRecord === null) return {
    ok: false,
    issues: issues.length ? issues : [issue("INVALID_SNAPSHOT", "/pages")],
    ...conversionTimings(mappingStart, geometryConversion),
  };
  return {
    ok: true,
    ...conversionTimings(mappingStart, geometryConversion),
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
  const recordsById = new Map(records.map((record) => [record.id as string, record]));
  const worldOrigins = new Map<string, { x: number; y: number }>();
  const resolveWorldOrigin = (id: string): { x: number; y: number } => {
    const cached = worldOrigins.get(id);
    if (cached) return cached;
    const path: string[] = [];
    const seen = new Set<string>();
    let cursor: string | null = id;
    while (cursor && !worldOrigins.has(cursor)) {
      if (seen.has(cursor)) throw new Error(`Cycle while projecting validated shape ${cursor}`);
      seen.add(cursor);
      const record = recordsById.get(cursor);
      if (!record) throw new Error(`Missing validated shape ${cursor}`);
      path.push(cursor);
      cursor = (record["parent-id"] as string | null | undefined) ?? null;
    }
    let origin = cursor ? worldOrigins.get(cursor)! : { x: 0, y: 0 };
    for (const pathId of path.reverse()) {
      const record = recordsById.get(pathId)!;
      const selrect = record.selrect as Record<string, number>;
      origin = { x: origin.x + selrect.x, y: origin.y + selrect.y };
      worldOrigins.set(pathId, origin);
    }
    return worldOrigins.get(id)!;
  };
  const shapes: Record<string, Shape> = {};
  for (const record of records) {
    const id = record.id as string;
    const rustFills = (record.fills as Array<Record<string, unknown>> | undefined) ?? [];
    const locked = record["react-locked"];
    if (typeof locked !== "boolean") throw new Error(`Missing locked extension for ${id}`);
    const matrix = record.transform as Record<string, number>;
    const selrect = record.selrect as Record<string, number>;
    const origin = resolveWorldOrigin(id);
    shapes[id] = {
      id,
      type: record.type as Shape["type"],
      name: record.name as string,
      bounds: { x: origin.x, y: origin.y, w: selrect.width, h: selrect.height },
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
  const converted = toRustCandidate(input);
  const emptyTimings = (serialization = 0, wasmValidationBoundary = 0,
    rustResponseParse = 0, resultMaterialization = 0): AdapterTimings => ({
      mapping: converted.mapping,
      geometryConversion: converted.geometryConversion,
      serialization,
      wasmValidationBoundary,
      rustResponseParse,
      resultMaterialization,
    });
  if (!converted.ok) return {
    ok: false, adapterVersion: G1_B3_ADAPTER_VERSION, issues: converted.ok ? [issue("INVALID_SNAPSHOT", "/")] : converted.issues,
    diagnostics: [], timingsMs: emptyTimings(), wasmMemoryBytes: wasmMemoryBytes(),
  };
  const serializationStart = performance.now();
  const json = JSON.stringify(converted.candidate);
  const serialization = performance.now() - serializationStart;
  if (json === undefined) return {
    ok: false, adapterVersion: G1_B3_ADAPTER_VERSION, issues: [issue("INVALID_SNAPSHOT", "/")],
    diagnostics: [], timingsMs: emptyTimings(serialization), wasmMemoryBytes: wasmMemoryBytes(),
  };
  const validationStart = performance.now();
  let response: string;
  try {
    response = rust.validateDocumentJson(json);
  } catch {
    return { ok: false, adapterVersion: G1_B3_ADAPTER_VERSION,
      issues: [issue("INVALID_RUST_RESPONSE", "/")], diagnostics: [],
      timingsMs: emptyTimings(serialization, performance.now() - validationStart), wasmMemoryBytes: wasmMemoryBytes() };
  }
  const wasmValidationBoundary = performance.now() - validationStart;
  const parseStart = performance.now();
  let result: RustValidationResult;
  try {
    result = JSON.parse(response) as RustValidationResult;
  } catch {
    return { ok: false, adapterVersion: G1_B3_ADAPTER_VERSION,
      issues: [issue("INVALID_RUST_RESPONSE", "/")], diagnostics: [],
      timingsMs: emptyTimings(serialization, wasmValidationBoundary, performance.now() - parseStart), wasmMemoryBytes: wasmMemoryBytes() };
  }
  const rustResponseParse = performance.now() - parseStart;
  if (!result || typeof result.ok !== "boolean" || !Array.isArray(result.diagnostics)) {
    return { ok: false, adapterVersion: G1_B3_ADAPTER_VERSION,
      issues: [issue("INVALID_RUST_RESPONSE", "/")], diagnostics: [],
      timingsMs: emptyTimings(serialization, wasmValidationBoundary, rustResponseParse), wasmMemoryBytes: wasmMemoryBytes() };
  }
  if (!result.ok || !result.document) return {
    ok: false, adapterVersion: G1_B3_ADAPTER_VERSION, issues: [], diagnostics: result.diagnostics,
    timingsMs: emptyTimings(serialization, wasmValidationBoundary, rustResponseParse), wasmMemoryBytes: wasmMemoryBytes(),
  };
  const materializationStart = performance.now();
  const roundTrip = fromRustDocument(result.document);
  const resultMaterialization = performance.now() - materializationStart;
  return {
    ok: true, adapterVersion: G1_B3_ADAPTER_VERSION, candidate: result.document,
    roundTrip, diagnostics: [],
    timingsMs: emptyTimings(serialization, wasmValidationBoundary, rustResponseParse, resultMaterialization),
    wasmMemoryBytes: wasmMemoryBytes(),
  };
}
