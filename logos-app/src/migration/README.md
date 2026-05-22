# Universal Migration Engine

Logos can open design files from every major design tool. Drop a `.sketch`, `.xd`, or the JSON exported from our Figma plugin into the Import dialog: tokens, frames, components, text, and vector shapes all appear in Logos with zero data loss.

---

## Table of Contents

- [Architecture overview](#architecture-overview)
- [File layout](#file-layout)
- [Token model](#token-model)
- [Shape model](#shape-model)
- [Importer API reference](#importer-api-reference)
  - [Figma](#figma)
  - [Sketch](#sketch)
  - [Adobe XD](#adobe-xd)
- [Type–node mapping tables](#type--node-mapping-tables)
- [Fill / gradient mapping](#fill--gradient-mapping)
- [Typography mapping](#typography-mapping)
- [Adding a new importer](#adding-a-new-importer)
- [Known limitations](#known-limitations)

---

## Architecture Overview

Every importer follows an identical four-layer architecture:

```
*-format.ts          TypeScript types + runtime guards + low-level helpers
*-token-converter.ts Pure function: raw document → LogosTokenSet[] + LogosTokenTheme[]
*-shape-converter.ts Pure function: raw document → Shape[] (flat, depth-first)
*-importer.ts        Orchestrator: reads & unzips the file, calls the two converters,
                     returns a discriminated union result { ok: true, ... } | { ok: false, ... }
```

All four layers are **side-effect free**. They receive data and return data. Nothing touches a Zustand store, the DOM, or the file system. The dialog (`ImportMigrationDialog.tsx`) owns all side effects.

### Data flow

```
User selects file
       │
       ▼
 importXxxFile(file)          ← orchestrator (async, handles ZIP/JSON extraction)
       │
       ├──▶ convertXxxTokens(content)   ── pure ──▶  XxxTokenConversionResult
       │                                              { sets[], themes[], warnings[] }
       │
       └──▶ convertXxxContent(content)  ── pure ──▶  XxxShapeConversionResult
                                                      { shapes[], roots[], symbolIdMap, warnings[] }
       │
       ▼
ImportMigrationDialog
  tokenStore.loadImport(sets, themes)
  documentStore.insertShapes(shapes)    ← future / caller's responsibility
```

---

## File Layout

```
src/migration/
├── README.md                   ← this file
├── figma/
│   ├── figma-plugin-format.ts  TypeScript types for the Logos Figma plugin JSON
│   ├── figma-token-converter.ts Figma variables → LogosTokenSet + LogosTokenTheme
│   ├── figma-shape-converter.ts FigmaExportNode tree → flat Shape[]
│   └── figma-importer.ts       Reads .logos-figma.json, calls both converters
├── sketch/
│   ├── sketch-format.ts        TypeScript types for Sketch JSON layers
│   ├── sketch-token-converter.ts Shared styles + swatches → tokens
│   ├── sketch-shape-converter.ts Sketch layers → flat Shape[]
│   └── sketch-importer.ts      Unzips .sketch, calls both converters
└── xd/
    ├── xd-format.ts            TypeScript types for Adobe XD graphicContent.agx
    ├── xd-token-converter.ts   Color resources + char styles → tokens
    ├── xd-shape-converter.ts   XD node tree → flat Shape[]
    └── xd-importer.ts          Unzips .xd (OPC), calls both converters
```

---

## Token Model

All three importers emit the same token types, defined in `figma/figma-token-converter.ts` and shared across the module:

```ts
interface LogosToken {
  id: string;
  name: string;       // slash-separated namespace: "brand/blue/500"
  type: LogosTokenType;
  value: string;      // resolved value or "{alias.path}" reference
  description: string;
}

interface LogosTokenSet {
  id: string;
  name: string;
  description: string;
  tokens: LogosToken[];
}

interface LogosTokenTheme {
  id: string;
  name: string;
  // maps token name → value override for this mode/theme
  values: Record<string, string>;
}

type LogosTokenType =
  | "color" | "number" | "string" | "boolean"
  | "spacing" | "dimensions" | "opacity";
```

### Token sets produced per importer

| Importer | Sets produced |
|----------|--------------|
| **Figma** | One set per Variable Collection; one theme per Collection Mode |
| **Sketch** | `"Swatches"`, `"Layer Styles"`, `"Text Styles"` |
| **Adobe XD** | `"Colors"`, `"Brushes"`, `"Character Styles"` |

---

## Shape Model

Shape converters emit flat arrays of `Shape` records (defined in `src/types/shapes.ts`). Every shape carries:

| Field | Type | Meaning |
|-------|------|---------|
| `id` | `string` (UUID v4) | Logos-internal ID (remapped from source-platform IDs) |
| `type` | `ShapeType` | `"frame"`, `"group"`, `"rect"`, `"circle"`, `"path"`, `"text"`, `"instance"`, `"component"`, `"vector-network"` |
| `name` | `string` | Layer name from the source file |
| `bounds` | `{ x, y, w, h }` | Position and dimensions in canvas space |
| `transform` | `Transform` (6-tuple) | Affine matrix `[a, b, c, d, tx, ty]` |
| `rotation` | `number` | Degrees, CCW positive, derived from the matrix |
| `fills` | `Fill[]` | Solid or gradient fills (see [Fill mapping](#fill--gradient-mapping)) |
| `opacity` | `number` | `0–1` |
| `hidden` | `boolean` | Layer visibility |
| `locked` | `boolean` | Layer lock state |
| `parentId` | `string \| null` | Parent shape ID in the flat array |
| `children` | `string[]` | Child shape IDs (depth-first order) |

Optional fields present only when relevant:

| Field | Present when |
|-------|-------------|
| `text`, `fontFamily`, `fontWeight`, `fontSize`, `textColor`, `textAlign`, `textDecoration`, `lineHeight`, `letterSpacing` | `type === "text"` |
| `layout`, `layoutFlexDir`, `layoutWrapType`, `layoutJustifyContent`, `layoutAlignItems`, `layoutGap`, `layoutPadding` | Node has auto-layout (Figma) or Smart Layout (Sketch) |
| `componentMeta` | `type === "component"` |
| `instanceMeta` | `type === "instance"` |

---

## Importer API Reference

### Figma

**Input:** A `.logos-figma.json` file exported from the Logos Figma plugin.

```ts
import { importFigmaTokenFile } from "./figma/figma-importer";

const result = await importFigmaTokenFile(file);

if (!result.ok) {
  console.error(result.error);  // human-readable string
  return;
}

result.documentName       // string: Figma document name
result.conversion         // ConversionResult: { sets, themes, warnings }
result.shapeConversion    // ShapeConversionResult | undefined (v2 exports only)
                          // { shapes, pageRoots, warnings }
```

Figma also supports a REST API path:

```ts
import { importFigmaViaApi } from "./figma/figma-importer";

const result = await importFigmaViaApi(fileKey, personalAccessToken);
// Same result shape as importFigmaTokenFile, shapeConversion always absent
```

**Schema versions:**
- `schemaVersion: 1` (or absent) — tokens only, no shape tree
- `schemaVersion: 2` — tokens + full node tree with auto-layout, typography, vector networks

---

### Sketch

**Input:** A `.sketch` file (standard Sketch save; no special export step required).

```ts
import { importSketchFile } from "./sketch/sketch-importer";

const result = await importSketchFile(file);

if (!result.ok) {
  console.error(result.error);
  return;
}

result.documentName        // string: Sketch document name
result.tokenConversion     // SketchTokenConversionResult: { sets, themes, warnings }
result.shapeConversion     // SketchShapeConversionResult:
                           //   { shapes, pageRoots, symbolIdMap, warnings }
```

**ZIP layout read by the importer:**

| Entry | Contents |
|-------|---------|
| `document.json` | Shared styles, swatches, page list |
| `pages/<uuid>.json` | Layer tree for each page |
| `meta.json` | Document metadata (name, version) |

---

### Adobe XD

**Input:** A `.xd` file (Adobe XD's native save format; no special export step required). Also accepts a pre-read `Uint8Array` for programmatic use.

```ts
import { importXdFile } from "./xd/xd-importer";

const result = await importXdFile(file);          // File
const result = await importXdFile(uint8array);    // Uint8Array

if (!result.ok) {
  console.error(result.errorMessage);
  return;
}

result.documentName        // string
result.tokenConversion     // XdTokenConversionResult: { sets, themes, warnings }
result.shapeConversion     // XdShapeConversionResult:
                           //   { shapes, artboardRoots, symbolIdMap, warnings }
```

**XD (OPC) archive layout read by the importer:**

| Entry | Contents |
|-------|---------|
| `manifest.json` | Document name, component list |
| `resources/graphic/graphicContent.agx` | Full node tree (JSON despite `.agx` extension) |

---

## Type–Node Mapping Tables

### Figma → Logos

| Figma node type | Logos `ShapeType` | Notes |
|----------------|-------------------|-------|
| `FRAME`, `SECTION` | `"frame"` | Auto-layout preserved |
| `GROUP` | `"group"` | |
| `RECTANGLE` | `"rect"` | |
| `ELLIPSE` | `"circle"` | |
| `TEXT` | `"text"` | Full typography |
| `VECTOR` (simple) | `"path"` | |
| `VECTOR` (≥1 segment + regions) | `"vector-network"` | VN anchors, segments, regions |
| `LINE`, `POLYGON`, `STAR` | `"path"` | |
| `BOOLEAN_OPERATION` | `"path"` | Pre-computed result |
| `COMPONENT`, `COMPONENT_SET` | `"component"` | |
| `INSTANCE` | `"instance"` | |

### Sketch → Logos

| Sketch layer class | Logos `ShapeType` | Notes |
|-------------------|-------------------|-------|
| `artboard`, `page` | `"frame"` | |
| `group` | `"group"` | Smart Layout preserved |
| `rectangle` | `"rect"` | |
| `oval` | `"circle"` | |
| `text` | `"text"` | |
| `shapePath`, `shapeGroup` | `"path"` | |
| `symbolMaster` | `"component"` | Pre-scanned into `symbolIdMap` |
| `symbolInstance` | `"instance"` | Resolved via `symbolIdMap` |
| `slice`, `bitmap` | skipped | |

### Adobe XD → Logos

| XD node type | XD shape subtype | Logos `ShapeType` | Notes |
|-------------|-----------------|-------------------|-------|
| `artboard` | — | `"frame"` | Acts as page root |
| `group`, `RepeatGrid` | — | `"group"` | RepeatGrid loses grid metadata |
| `BooleanGroup` | — | `"path"` | Boolean ops not preserved |
| `shape` | `rect` | `"rect"` | |
| `shape` | `ellipse` | `"circle"` | Sized from `rx`/`ry` |
| `shape` | `path`, `compound`, `line`, `polygon` | `"path"` | |
| `text` | — | `"text"` | Multi-paragraph aware |
| `symbolInstance` | — | `"instance"` | Resolved via `symbolIdMap` |
| `slice` | — | skipped | |

---

## Fill / Gradient Mapping

All three converters produce the same `Fill` union type:

```ts
// Solid
{ type: "solid", color: "#rrggbbaa", opacity: number }

// Linear gradient
{
  type: "gradient", opacity: number, atlasSlot: -1,
  gradient: {
    type: "linear",
    startX: number, startY: number,   // 0–1 relative to bounding box
    endX: number, endY: number,
    width: 1,
    stops: { color: "#rrggbb", position: number, opacity: number }[]
  }
}

// Radial gradient
{
  type: "gradient", opacity: number, atlasSlot: -1,
  gradient: {
    type: "radial",
    startX: number, startY: number,
    endX: number, endY: number,
    width: number,
    stops: [...]
  }
}
```

**Transparent placeholder fill:** Nodes with no visible fills receive `{ type: "solid", color: "#e8eaee", opacity: 0.3 }` so they remain selectable in the canvas.

---

## Typography Mapping

All three importers map to the same fields on `Shape`:

| Logos field | Figma source | Sketch source | XD source |
|-------------|-------------|---------------|-----------|
| `text` | `node.characters` | Concatenated `NSString` attributes | `paragraphs[].lines[][]` runs |
| `fontFamily` | `fontName.family` | `NSFontInfoAttribute` | `characterAttributes.fontFamily` |
| `fontWeight` | `fontName.style` → weight table | `parseFontName("Inter-SemiBold")` | `fontStyle` → weight table |
| `fontSize` | `fontSize` | `NSFontAttributeName` | `characterAttributes.fontSize` |
| `textColor` | fill paint with `type: "SOLID"` | `NSColor` | `characterAttributes.fill` |
| `textAlign` | `textAlignHorizontal` | `textAlignment` (0–4) | `paragraph.align` |
| `textDecoration` | `textDecoration` range property | `strikethrough`/`underline` attr | `characterAttributes.strikethrough/underline` |
| `lineHeight` | `lineHeight` (PIXELS / PERCENT) | `paragraphSpacing` | `characterAttributes.lineSpacing` |
| `letterSpacing` | `letterSpacing` with unit conversion | `kerning` | `letterSpacing / 1000 × fontSize` |

**Font style → weight mapping** (shared across all three):

| Style string | Weight |
|-------------|--------|
| `Thin`, `ExtraLight`, `Hairline` | 100 |
| `Light` | 300 |
| `Regular`, `Normal` | 400 |
| `Medium` | 500 |
| `SemiBold`, `DemiBold` | 600 |
| `Bold` | 700 |
| `ExtraBold`, `UltraBold` | 800 |
| `Black`, `Heavy` | 900 |
| *(anything else)* | 400 |

---

## Adding a New Importer

Follow the four-file pattern. Replace `Xxx` with your platform name:

### 1. `xxx-format.ts`

Define TypeScript types for the source format and export runtime guards:

```ts
export interface XxxDocument { /* ... */ }

export function isXxxDocument(v: unknown): v is XxxDocument {
  return (
    typeof v === "object" && v !== null &&
    "version" in v                           // minimum viable guard
  );
}

// Low-level helpers used by both converters
export function xxxColorToHex(c: XxxColor): string { /* ... */ }
```

### 2. `xxx-token-converter.ts`

```ts
import type { LogosTokenSet, LogosTokenTheme } from "../figma/figma-token-converter";

export interface XxxTokenConversionResult {
  sets: LogosTokenSet[];
  themes: LogosTokenTheme[];
  warnings: string[];
}

export function convertXxxTokens(
  doc: XxxDocument,
  documentName: string
): XxxTokenConversionResult { /* ... */ }
```

### 3. `xxx-shape-converter.ts`

```ts
import type { Shape } from "../../types/shapes";
import { IDENTITY_TRANSFORM } from "../../types/shapes";

export interface XxxShapeConversionResult {
  shapes: Shape[];
  pageRoots: { pageId: string; pageName: string; rootShapeIds: string[] }[];
  symbolIdMap: Map<string, string>;
  warnings: string[];
}

export function convertXxxContent(doc: XxxDocument): XxxShapeConversionResult {
  const ctx: ConvertCtx = {
    shapes: [],
    idMap: new Map(),           // source ID → crypto.randomUUID()
    symbolIdMap: new Map(),
    warnings: [],
  };
  // 1. Pre-scan component masters → symbolIdMap
  // 2. Walk artboards/pages depth-first via convertNode(node, parentId, ctx)
  // 3. Return results
}
```

Key conventions:
- Always call `crypto.randomUUID()` via `ctx.idMap` — never use source-platform IDs as Logos IDs.
- Push shapes **after** recursing into children so child IDs are known when building `childIds`.
- Use the transparent placeholder fill for nodes with no visible fill.

### 4. `xxx-importer.ts`

```ts
export interface XxxImportResult {
  ok: true;
  documentName: string;
  tokenConversion: XxxTokenConversionResult;
  shapeConversion: XxxShapeConversionResult;
}
export interface XxxImportError { ok: false; errorMessage: string; }

export async function importXxxFile(
  input: File | Uint8Array
): Promise<XxxImportResult | XxxImportError> {
  // 1. Read bytes
  // 2. Unzip (if applicable) with fflate.unzipSync
  // 3. Parse and validate with isXxxDocument()
  // 4. Call convertXxxTokens() and convertXxxContent()
  // 5. Return result
}
```

### 5. Wire the dialog

In `ImportMigrationDialog.tsx`:
1. Add `"xxx"` to the `ImportSource` union.
2. Add `{ id: "xxx", label: "Platform Name" }` to `SOURCES`.
3. Add a `xxxFileInputRef` ref and a `handleXxxFilePick` callback.
4. Add an `<XxxPanel>` render branch in the body.

---

## Known Limitations

These limitations are consistent across all three importers and share the same resolution path.

| # | Limitation | Affected importers | Severity | Follow-up |
|---|-----------|-------------------|----------|-----------|
| L1 | **Boolean operations not preserved.** `BOOLEAN_OPERATION` (Figma), `shapeGroup` (Sketch), and `BooleanGroup` (XD) all import as flattened paths. The boolean op type (union / subtract / intersect / exclude) is discarded. | All | Low | Map to Logos vector network boolean ops once VN editor supports them. |
| L2 | **Embedded images skipped.** Bitmap fills and embedded image layers are not imported. | Sketch, XD | Low | Extract images to the Logos media store as a follow-up pipeline step. |
| L3 | **Repeat Grid loses grid metadata.** XD's Repeat Grid imports as a flat group of repeated shapes. The repeat count and spacing are not preserved. | XD only | Low | Logos has no direct equivalent; shapes are preserved. |
| L4 | **Figma REST API imports tokens only.** The shape tree is unavailable via the public API without a full plugin export. | Figma API | Low | Full tree export requires the plugin (schema v2). |
| L5 | **Component variant properties not remapped.** Instance override values are stored verbatim from the source format. They are not resolved against the Logos component schema. | All | Low | Full override resolution requires the variant system to be finalized. |
