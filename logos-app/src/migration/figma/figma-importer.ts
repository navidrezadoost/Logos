/**
 * migration/figma/figma-importer.ts
 *
 * Phase IM1 — Orchestrator for Figma token import.
 *
 * Flow:
 *  1. User selects a `.logos-figma.json` file via the import dialog.
 *  2. importFigmaTokenFile() reads and validates the JSON.
 *  3. Calls the converter to produce LogosTokenSet[] + LogosTokenTheme[].
 *  4. Calls the callback (tokenStore.loadImport) to put data into the store.
 *
 * This module is deliberately side-effect free: it returns data and calls
 * the provided callback; it does not import the store directly.  This keeps
 * it testable without Zustand setup.
 */

import { isLogosFigmaExport } from "./figma-plugin-format";
import { convertFigmaExport, type ConversionResult } from "./figma-token-converter";
import { convertFigmaPages, type ShapeConversionResult } from "./figma-shape-converter";

// ─── Public API ──────────────────────────────────────────────────────────────

export interface ImportResult {
  ok: true;
  documentName: string;
  conversion: ConversionResult;
  /** Present when the export contained a full node tree (schemaVersion >= 2). */
  shapeConversion?: ShapeConversionResult;
}

export interface ImportError {
  ok: false;
  error: string;
}

/**
 * Parse a `.logos-figma.json` File object and convert it to Logos tokens.
 *
 * @param file  The File selected by the user (from <input type="file">)
 * @returns     ImportResult on success, ImportError on failure
 */
export async function importFigmaTokenFile(
  file: File
): Promise<ImportResult | ImportError> {
  // 1. Read
  let raw: string;
  try {
    raw = await file.text();
  } catch (err) {
    return { ok: false, error: `Could not read file: ${err}` };
  }

  // 2. Parse
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return { ok: false, error: "File is not valid JSON." };
  }

  // 3. Validate
  if (!isLogosFigmaExport(data)) {
    return {
      ok: false,
      error:
        "This file was not exported by the Logos Figma plugin. " +
        "Make sure you export using the 'Export for Logos' plugin.",
    };
  }

  // 4. Convert
  let conversion: ConversionResult;
  try {
    conversion = convertFigmaExport(data);
  } catch (err) {
    return { ok: false, error: `Conversion failed: ${err}` };
  }

  // 5. Shape conversion (v2 exports only)
  let shapeConversion: ShapeConversionResult | undefined;
  if (data.pages && data.pages.length > 0) {
    try {
      shapeConversion = convertFigmaPages(data.pages);
    } catch (err) {
      // Non-fatal: tokens are still imported
      console.warn("[logos-import] shape conversion failed:", err);
    }
  }

  return { ok: true, documentName: data.documentName, conversion, shapeConversion };
}

/**
 * Import from a Figma personal access token + file key via the REST API.
 * Secondary path: useful for CI/CD pipelines or power users.
 *
 * Requires the Figma REST API v1 endpoints:
 *   GET /v1/files/:key/variables/local
 *
 * @param fileKey    Figma file key (from the URL: figma.com/design/:key/...)
 * @param apiToken   Figma personal access token
 */
export async function importFigmaViaApi(
  fileKey: string,
  apiToken: string
): Promise<ImportResult | ImportError> {
  const headers = { "X-Figma-Token": apiToken };

  // Fetch file name for display
  let documentName = fileKey;
  try {
    const metaRes = await fetch(
      `https://api.figma.com/v1/files/${fileKey}?depth=0`,
      { headers }
    );
    if (metaRes.ok) {
      const meta = (await metaRes.json()) as { name?: string };
      documentName = meta.name ?? fileKey;
    }
  } catch {
    // Non-fatal: we continue without a document name
  }

  // Fetch variables
  let data: unknown;
  try {
    const res = await fetch(
      `https://api.figma.com/v1/files/${fileKey}/variables/local`,
      { headers }
    );
    if (!res.ok) {
      return {
        ok: false,
        error: `Figma API error ${res.status}: ${res.statusText}. ` +
               "Check your API token and file key.",
      };
    }
    data = await res.json();
  } catch (err) {
    return { ok: false, error: `Network error: ${err}` };
  }

  // The REST API returns a different envelope than the plugin —
  // normalize it into the plugin export format before converting.
  const normalized = normalizeFigmaApiResponse(data, documentName);
  if (!normalized) {
    return {
      ok: false,
      error: "Unexpected Figma API response shape. The API may have changed.",
    };
  }

  let conversion: ConversionResult;
  try {
    conversion = convertFigmaExport(normalized);
  } catch (err) {
    return { ok: false, error: `Conversion failed: ${err}` };
  }

  return { ok: true, documentName, conversion };
}

// ─── Normalize Figma REST API response ───────────────────────────────────────

/**
 * The Figma REST API /variables/local endpoint returns a different JSON shape
 * than the plugin export.  This function maps it into LogosFigmaExport format.
 *
 * API shape (simplified):
 * {
 *   meta: {
 *     variables: { [id]: { name, resolvedType, valuesByMode, ... } },
 *     variableCollections: { [id]: { name, modes, defaultModeId, ... } }
 *   }
 * }
 */
function normalizeFigmaApiResponse(
  data: unknown,
  documentName: string
): import("./figma-plugin-format").LogosFigmaExport | null {
  if (typeof data !== "object" || data === null) return null;
  const d = data as Record<string, unknown>;
  const meta = d["meta"] as Record<string, unknown> | undefined;
  if (!meta) return null;

  const rawVars = meta["variables"] as Record<string, unknown> | undefined;
  const rawCols = meta["variableCollections"] as Record<string, unknown> | undefined;
  if (!rawVars || !rawCols) return null;

  const collections = Object.entries(rawCols).map(([id, col]) => {
    const c = col as Record<string, unknown>;
    return {
      id,
      name: String(c["name"] ?? id),
      modes: ((c["modes"] as unknown[]) ?? []).map((m) => {
        const mode = m as Record<string, unknown>;
        return { id: String(mode["modeId"]), name: String(mode["name"]) };
      }),
      defaultModeId: String(c["defaultModeId"] ?? ""),
    };
  });

  const variables = Object.entries(rawVars).map(([id, v]) => {
    const variable = v as Record<string, unknown>;
    const rawByMode = variable["valuesByMode"] as Record<string, unknown> ?? {};

    const valuesByMode: Record<string, import("./figma-plugin-format").FigmaVariableValue> = {};
    for (const [modeId, raw] of Object.entries(rawByMode)) {
      const rv = raw as Record<string, unknown>;
      if (rv["type"] === "VARIABLE_ALIAS") {
        valuesByMode[modeId] = { alias: String(rv["id"]) };
      } else {
        const type = String(variable["resolvedType"]);
        if (type === "COLOR" && typeof rv["r"] === "number") {
          const r = rv["r"] as number;
          const g = rv["g"] as number;
          const b = rv["b"] as number;
          const a = (rv["a"] as number) ?? 1;
          const toHex = (n: number) =>
            Math.round(n * 255).toString(16).padStart(2, "0");
          const hex =
            "#" + toHex(r) + toHex(g) + toHex(b) + (a < 1 ? toHex(a) : "");
          valuesByMode[modeId] = { color: hex };
        } else if (type === "FLOAT") {
          valuesByMode[modeId] = { number: Number(raw) };
        } else if (type === "STRING") {
          valuesByMode[modeId] = { string: String(raw) };
        } else if (type === "BOOLEAN") {
          valuesByMode[modeId] = { boolean: Boolean(raw) };
        } else {
          valuesByMode[modeId] = { raw: JSON.stringify(raw) };
        }
      }
    }

    return {
      id,
      name: String(variable["name"] ?? id),
      collectionId: String(variable["variableCollectionId"] ?? ""),
      collectionName: "",
      type: String(variable["resolvedType"]) as import("./figma-plugin-format").FigmaVariableType,
      valuesByMode,
      scopes: (variable["scopes"] as string[]) ?? [],
      hiddenFromPublishing: Boolean(variable["hiddenFromPublishing"]),
      description: String(variable["description"] ?? ""),
    };
  });

  // Backfill collectionName
  const colNameMap = new Map(collections.map((c) => [c.id, c.name]));
  variables.forEach((v) => {
    v.collectionName = colNameMap.get(v.collectionId) ?? v.collectionId;
  });

  return {
    version: 1,
    source: "figma-plugin",
    exportedAt: new Date().toISOString(),
    documentName,
    collections,
    variables,
  };
}
