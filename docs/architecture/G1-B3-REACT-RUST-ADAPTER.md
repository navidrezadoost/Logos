# G1-B3 — Read-only React to Rust document adapter

## Status

This is a test-only, versioned adapter candidate. It does not establish Rust as production document authority and is not connected to editor mutation, persistence, save, or import paths. The React store remains authoritative for current editor behavior. The Rust `logos-types::document` module performs semantic validation.

## Repository-grounded source contract

The current web model is `logos-app/src/stores/documentStore.ts` plus `logos-app/src/types/shapes.ts`:

| React source | Candidate Rust document | Policy |
| --- | --- | --- |
| `documentId` | `Document.id` | Explicit caller-supplied UUID. It is not present in Zustand. Missing/malformed identity rejects. |
| `pageOrder` (exactly one entry) | `pages` vector order | One-page slice only. Additional pages reject. |
| `Page.id`, `name`, `rootShapeIds` | `Page.id`, `name`, `root-shape-ids` | Page ID is preserved; it must already be a UUID. No generated/remapped ID. |
| `Shape.id`, `name`, `type` | Shape record | UUID and name preserved. Only `rect`, `frame`, `group` supported. |
| `bounds{x,y,w,h}` | `selrect{x,y,width,height}` | React documents these as canvas-local; Rust candidate documents them as parent-local. Direct copy is accepted only when every ancestor has zero x/y origin and identity transform, making the spaces coincide. Other nested placements reject as `UNVERIFIED_COORDINATE_SPACE`. |
| six-number `transform` | Rust affine `Matrix` | Coefficients preserved; inverse is derived algebraically and then checked by Rust. Singular/non-finite matrices reject. |
| `rotation` | `rotation` in degrees | Preserved separately from the matrix, matching the source field. |
| `parentId`, ordered `children` | `parent-id`, ordered `shapes` | Preserved. Rust checks backlinks, graph validity, root membership, ordering, and container policy. |
| implicit frame ancestry | `frame-id` | Derived by walking `parentId` to the nearest enclosing React `frame`. For a frame, this means its outer frame. Cycles/missing parents are left for Rust diagnostics. |
| `hidden`, `opacity` | same-named Rust fields | Preserved and Rust validated. |
| ordered solid fills `{type,color,opacity}` | Rust `Fill::Solid` | Preserved in order. Gradient/image/unknown fill payload rejects. |
| `locked` | typed `react-locked` extension | Boolean is preserved as an extension because this candidate Rust Shape has no locked field. It does not affect Rust renderer semantics. |

The actual Zustand snapshot does not provide a document UUID; page IDs are currently generated with `nanoid()` in `documentStore.ts`, while the candidate Rust envelope requires UUIDs. Consequently, the adapter requires an explicit document UUID and rejects live nanoid page IDs. Stable IDs from a fixture are used only by isolated conformance tests. Production identity migration or a stable ID mapping is outside G1-B3 and must be decided before editor integration.

The source `Shape` type describes `bounds` as canvas-local, while the Rust candidate says `selrect` is parent-local. G1-B3 does not invent an arbitrary coordinate conversion. It accepts nested fixtures only when ancestor origins and transforms are identity values; translated or transformed parent trees fail closed. Ordinary nested editor snapshots may therefore still reject even when their shape kinds are otherwise supported. This is an explicit compatibility blocker, not complete nested-coordinate support.

## Strict rejection matrix

| Source content | Result |
| --- | --- |
| Text, typography, and text-only properties | `UNSUPPORTED_SHAPE` or `UNSUPPORTED_PROPERTY` |
| Vector networks, paths, circles/ellipses, SVG, booleans | `UNSUPPORTED_SHAPE` |
| Components and instances, component metadata | `UNSUPPORTED_SHAPE` or `UNSUPPORTED_PROPERTY` |
| Gradients, images, complex appearance, strokes, shadows, blur | `UNSUPPORTED_FILL` or `UNSUPPORTED_PROPERTY` |
| Auto Layout and layout attributes | `UNSUPPORTED_PROPERTY` |
| Prototyping or token binding fields | `UNSUPPORTED_PROPERTY` |
| Unknown top-level, page, shape, or fill properties | `UNSUPPORTED_PROPERTY` |
| Additional pages, unsupported page IDs, malformed IDs | `INVALID_SNAPSHOT`, `UNSUPPORTED_PAGE_ID`, or `IDENTITY_REQUIRED` |
| Invalid bounds/matrices | `INVALID_GEOMETRY` or `INVALID_TRANSFORM`; Rust also checks the normalized graph and inverse matrix |
| Nested bounds under a translated/transformed ancestor | `UNVERIFIED_COORDINATE_SPACE`; no silent coordinate reinterpretation |
| Invalid backlinks, roots, ordering, cycles, cross-page/missing references | Rust stable `ValidationDiagnosticCode` values; no TypeScript graph validator |

No failed conversion returns a partially converted document. Candidate extensions are limited to the `react-locked` boolean, whose round trip is asserted by the browser fixture. Any unknown extension is rejected rather than copied through an untyped sidecar.

## WASM and lifecycle boundary

`logos-types::wasm_api::validate_document_json` is a narrow JSON-in/JSON-out function over `decode_and_validate_json`. It retains no document state. The TypeScript adapter only checks field shape, maps the supported wire fields, derives the inverse matrix and frame ancestry, and converts the validated Rust result back to React-compatible shape records. It does not implement document graph semantics.

The Chromium smoke runs valid, invalid-backlink, unsupported-field, coordinate-space, document-identity, and nanoid-page-ID cases. It compares the full supported React snapshot after Rust round trip, reads the real Zustand store before/after without calling store actions, and traps `localStorage.setItem` and `indexedDB.open`. It also performs repeated React mount/unmount and rejected-document validation, plus size runs for 4, 100, 1,000, and 10,000 objects. Serialization and Rust validation time are recorded separately; WASM linear-memory reservation and observed JS heap size are reported independently where available. These values do not measure retained JavaScript resources and do not prove absence of leaks.

The test-only browser output includes the browser user agent, per-size timing fields, WASM linear-memory bytes, and Chrome's observed JS heap size when available. CI uploads the DOM evidence; hosted figures must be read from the exact-head artifact.

### Local adapter measurement (2026-11-14)

Executed on Headless Chrome 133.0.6943.126, Linux x86_64 (kernel 6.12.112-1-MANJARO), 1280×720 at DPR 1. Each size had one warmup and five measured runs. p50/p95 use nearest-rank samples. Serialization includes React-to-candidate mapping and `JSON.stringify`; the Rust WASM column measures the JSON ABI call including Rust decode/validate/encode and JS response parsing. These are one-host adapter measurements, not editor-render latency.

| React shape records | Serialization p50 / p95 (ms) | Rust WASM boundary p50 / p95 (ms) | Total p50 / p95 (ms) | WASM linear memory reserved after sample (bytes) | Observed JS heap after sample (bytes) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 0.00 / 0.10 | 0.20 / 0.30 | 0.30 / 0.50 | 1,179,648 | 3,531,170 |
| 100 | 1.10 / 2.00 | 2.60 / 4.40 | 3.90 / 6.50 | 1,835,008 | 4,490,812 |
| 1,000 | 4.70 / 5.00 | 16.70 / 17.50 | 22.00 / 22.80 | 7,864,320 | 21,357,146 |
| 10,000 | 32.90 / 40.80 | 147.50 / 152.20 | 182.00 / 194.70 | 61,407,232 | 155,002,513 |

Repeated five React mount/unmount validation cycles showed JS heap observations of 3,158,262 bytes before and 3,266,050 bytes after, without forced garbage collection. This is not retained-memory or leak evidence. The larger fixtures are only validated shape records; the harness does not render them or treat them as observed canvas output. The exact-head hosted workflow must repeat and upload its own evidence before this local run is treated as CI qualification.

The adapter does not feed the rendering projection or mutate the store, so production rendering is outside its call path. This smoke does not claim pixel-level renderer parity. That remains an integration gate.

## First controlled integration recommendation

After the Chief Architect resolves stable document/page identity ownership and canvas-local versus parent-local coordinate conversion, integrate as a developer-only read-only diagnostics command on a single supported one-page document. Run validation on an immutable snapshot, show Rust diagnostics, and keep the action unreachable from save/edit paths. Gate progression on a named production-source fixture with UUID identities, exact round trip, no store or persistence writes, Rust native/WASM agreement, plus a separate renderer pixel comparison. Do not begin write-through, Go persistence, or schema migration from this adapter.

## Validation evidence

See the G1-B3 PR checks and uploaded `g1-b3-react-rust-adapter-browser-evidence` artifact for exact-head test output. Historical documents and ordinary live Zustand documents remain unsupported wherever their data falls outside the table above. This is a supported-subset proof only, not complete document compatibility.
