/**
 * migration/sketch/sketch-importer.ts
 *
 * Phase IM3 — Orchestrator for Sketch file import.
 *
 * Flow:
 *   1. User drops a `.sketch` file into the Import dialog.
 *   2. importSketchFile(file) unzips the archive using fflate.
 *   3. Reads document.json → tokens (shared styles, swatches).
 *   4. Reads pages/*.json → shapes (layers, text, components, instances).
 *   5. Returns SketchImportResult for the dialog to consume.
 *
 * Side-effect free: all stores are updated by the caller (dialog).
 */

import { unzipSync, strFromU8 } from "fflate";

import {
  isSketchDocumentJson,
  isSketchPageJson,
  type SketchPageJson,
} from "./sketch-format";

import {
  convertSketchTokens,
  type SketchTokenConversionResult,
} from "./sketch-token-converter";

import {
  convertSketchPages,
  type SketchShapeConversionResult,
} from "./sketch-shape-converter";

// ─── Public API ──────────────────────────────────────────────────────────────

export interface SketchImportResult {
  ok: true;
  documentName: string;
  tokenConversion: SketchTokenConversionResult;
  shapeConversion: SketchShapeConversionResult;
}

export interface SketchImportError {
  ok: false;
  error: string;
}

/**
 * Import a `.sketch` file.
 *
 * @param file  File selected by the user via <input type="file">.
 * @returns     SketchImportResult on success, SketchImportError on failure.
 */
export async function importSketchFile(
  file: File
): Promise<SketchImportResult | SketchImportError> {
  // ── 1. Read & unzip ────────────────────────────────────────────────────────
  let zipEntries: ReturnType<typeof unzipSync>;
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    zipEntries = unzipSync(bytes, { filter: (f) => !f.name.startsWith("images/") });
  } catch (err) {
    return {
      ok: false,
      error: `Could not open "${file.name}" as a Sketch archive. Make sure it is a valid .sketch file. (${err})`,
    };
  }

  // ── 2. Parse document.json ─────────────────────────────────────────────────
  const docEntry = zipEntries["document.json"];
  if (!docEntry) {
    return { ok: false, error: "No document.json found inside the .sketch archive." };
  }

  let documentJson: unknown;
  try {
    documentJson = JSON.parse(strFromU8(docEntry));
  } catch {
    return { ok: false, error: "document.json is not valid JSON." };
  }

  if (!isSketchDocumentJson(documentJson)) {
    return { ok: false, error: "document.json does not look like a Sketch document." };
  }

  // Derive the document name from the file name (Sketch doesn't store it in document.json)
  const documentName = file.name.replace(/\.sketch$/i, "");

  // ── 3. Parse page JSON files ───────────────────────────────────────────────
  const pages: SketchPageJson[] = [];

  // document.json.pages[] contains refs like "pages/<uuid>"
  const pageRefs = documentJson.pages.map((ref) => ref._ref + ".json");

  for (const refPath of pageRefs) {
    const entry = zipEntries[refPath];
    if (!entry) {
      // Fallback: some exporters include pages without the explicit ref
      continue;
    }
    let pageJson: unknown;
    try {
      pageJson = JSON.parse(strFromU8(entry));
    } catch {
      continue;
    }
    if (isSketchPageJson(pageJson)) {
      pages.push(pageJson);
    }
  }

  // If refs produced nothing, try all pages/ entries directly
  if (pages.length === 0) {
    for (const [path, data] of Object.entries(zipEntries)) {
      if (!path.startsWith("pages/") || !path.endsWith(".json")) continue;
      let pageJson: unknown;
      try {
        pageJson = JSON.parse(strFromU8(data));
      } catch {
        continue;
      }
      if (isSketchPageJson(pageJson)) {
        pages.push(pageJson);
      }
    }
  }

  if (pages.length === 0) {
    return { ok: false, error: "No page data found inside the .sketch archive." };
  }

  // ── 4. Convert tokens ─────────────────────────────────────────────────────
  let tokenConversion: SketchTokenConversionResult;
  try {
    tokenConversion = convertSketchTokens(documentJson);
  } catch (err) {
    return { ok: false, error: `Token conversion failed: ${err}` };
  }

  // ── 5. Convert shapes ─────────────────────────────────────────────────────
  let shapeConversion: SketchShapeConversionResult;
  try {
    shapeConversion = convertSketchPages(pages);
  } catch (err) {
    return { ok: false, error: `Shape conversion failed: ${err}` };
  }

  return { ok: true, documentName, tokenConversion, shapeConversion };
}
