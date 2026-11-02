/**
 * migration/xd/xd-importer.ts
 *
 * Phase IM4 — Adobe XD file importer.
 *
 * Adobe XD files are OPC (Open Packaging Convention) ZIP archives.
 * Relevant entries:
 *   manifest.json                          — document metadata
 *   resources/graphic/graphicContent.agx  — JSON despite the .agx extension,
 *                                            contains all artboards and node tree
 */

import { unzipSync, strFromU8 } from "fflate";
import { isXdGraphicContent, isXdManifest } from "./xd-format";
import type { XdGraphicContent, XdManifest } from "./xd-format";
import { convertXdTokens } from "./xd-token-converter";
import type { XdTokenConversionResult } from "./xd-token-converter";
import { convertXdContent } from "./xd-shape-converter";
import type { XdShapeConversionResult } from "./xd-shape-converter";

// ─── Public types ─────────────────────────────────────────────────────────────

export interface XdImportResult {
  ok: true;
  documentName: string;
  tokenConversion: XdTokenConversionResult;
  shapeConversion: XdShapeConversionResult;
}

export interface XdImportError {
  ok: false;
  errorMessage: string;
}

// ─── Public API ───────────────────────────────────────────────────────────────

/**
 * Parse an `.xd` file (File or Uint8Array) and convert it into Logos
 * tokens and shape records.
 */
export async function importXdFile(
  input: File | Uint8Array
): Promise<XdImportResult | XdImportError> {
  try {
    const bytes =
      input instanceof Uint8Array
        ? input
        : new Uint8Array(await input.arrayBuffer());

    // ── Unzip ─────────────────────────────────────────────────────────────
    let files: ReturnType<typeof unzipSync>;
    try {
      files = unzipSync(bytes, {
        // Skip bitmap assets — we only need JSON metadata
        filter: (f) => !f.name.startsWith("resources/") || f.name.endsWith(".agx"),
      });
    } catch (err) {
      return { ok: false, errorMessage: `ZIP extraction failed: ${String(err)}` };
    }

    // ── Manifest ──────────────────────────────────────────────────────────
    const manifestBytes = files["manifest.json"] ?? files["META-INF/manifest.json"];
    let manifest: XdManifest | null = null;
    if (manifestBytes) {
      try {
        const parsed: unknown = JSON.parse(strFromU8(manifestBytes));
        if (isXdManifest(parsed)) manifest = parsed;
      } catch {
        // Non-fatal: document name fallback via file.name
      }
    }

    const documentName =
      manifest?.name ??
      (input instanceof File ? input.name.replace(/\.xd$/i, "") : "Untitled XD Document");

    // ── Graphic content ───────────────────────────────────────────────────
    const graphicContentBytes =
      files["resources/graphic/graphicContent.agx"] ??
      findByExtension(files, ".agx");

    if (!graphicContentBytes) {
      return { ok: false, errorMessage: "Could not locate graphicContent.agx in the XD archive." };
    }

    let graphicContent: XdGraphicContent;
    try {
      const parsed: unknown = JSON.parse(strFromU8(graphicContentBytes));
      if (!isXdGraphicContent(parsed)) {
        return { ok: false, errorMessage: "graphicContent.agx does not match the expected XD schema." };
      }
      graphicContent = parsed;
    } catch (err) {
      return { ok: false, errorMessage: `Failed to parse graphicContent.agx: ${String(err)}` };
    }

    // ── Convert ───────────────────────────────────────────────────────────
    const tokenConversion  = convertXdTokens(graphicContent, documentName);
    const shapeConversion  = convertXdContent(graphicContent);

    return {
      ok: true,
      documentName,
      tokenConversion,
      shapeConversion,
    };
  } catch (err) {
    return { ok: false, errorMessage: `Unexpected error importing XD file: ${String(err)}` };
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Find the first file ending with a given extension when the canonical path is absent. */
function findByExtension(
  files: ReturnType<typeof unzipSync>,
  ext: string
): Uint8Array | undefined {
  for (const [path, data] of Object.entries(files)) {
    if (path.endsWith(ext)) return data;
  }
  return undefined;
}
