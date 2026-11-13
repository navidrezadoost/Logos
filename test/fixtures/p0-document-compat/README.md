# P0 Document Compatibility Fixture

`source-document.json`, `react-projection.json`, `rust-domain-shape.json`, and `renderer-payload-expectation.json` are synthetic vectors describing the same rectangle at four boundaries. They are not a canonical document format, a production file, or proof that the complete editor currently converts between them. The separately listed historical fixture is an unmodified legacy test response, with the limits below.

| File | Intended boundary | Executed check in this branch |
|---|---|---|
| `source-document.json` | Synthetic legacy/Malli-shaped file data at the Go `file.data` boundary, including a raw string enum from an older Go row. | Go `filedata.NormalizeFileData` + JSON encode/decode test asserts keyword normalization, page/tree order, and the unknown extension survives. |
| `historical-flex-layouts.json` | Verbatim legacy render-wasm Playwright mock response containing Transit `fdata/objects-map` data and serialized flex-layout shapes. | Go test verifies the source digest, flex-direction evidence, and that this Go path leaves the unsupported envelope opaque through JSON encode/decode. It does **not** decode or render the shapes. |
| `react-projection.json` | Expected projection into the current React/Zustand `Page` + `Shape` shape. | **Not consumed by application code.** There is no active loader/converter from `get-file` into `WorkspacePage`; type and behavior mapping remain blocked. |
| `rust-domain-shape.json` | Expected Rust `logos-types::Shape` serialization for the same shape. | Rust `logos-types` serde test parses and checks the known geometry/identity/fill. This is a single-shape unit contract, not a Go→Rust adapter test. |
| `renderer-payload-expectation.json` | Current React scene adapter’s 104-byte base-props and 4+160-byte fill ABI fields for this shape. | **Descriptive only.** Existing renderer CI tests a standalone blue pixel; no test feeds this exact React projection through `scene.ts` and compares the emitted buffer. |

## Deliberate loss probes

- The source adds `unknown-extension`; the Go map/JSON path must preserve it. The current React `Shape` and Rust `Shape` have no general unknown-field bag (Rust `plugin_data` only accepts string values), so a React/Rust round trip is not allowed to silently claim preservation.
- Legacy `parent-id` uses the nil-UUID page root; React page membership uses `rootShapeIds` and a nullable `parentId`; Rust `parent_id` is an optional UUID. The boundary mapping must specify whether root membership maps to a nil UUID or `None` and preserve order.
- Legacy solid fills use `fill-color`/`fill-opacity` (and Transit keyword/type conventions); React uses `{type:"solid",color,opacity}`; Rust serde uses a `fill-type` tagged enum. Current Go normalization does not establish the React/Rust conversion.
- React `bounds.w/h` and renderer ABI `[x1,y1,x2,y2]` differ from Rust `selrect {x,y,width,height}` and the legacy map’s redundant `x/y/width/height/selrect` fields. Fixture numbers are chosen to expose off-by-origin/extent mistakes.
- `componentStore`, `tokenStore`, and `prototypeStore` are not all included in `offline/persist.ts`’s document snapshot. This rectangle fixture does not cover those features; they need separate identity/alias/interaction fixtures before a complete-document claim.

## Historical sample provenance and limits

`historical-flex-layouts.json` is copied byte-for-byte from the preserved legacy checkout at commit `665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92`, path `frontend/playwright/data/render-wasm/get-file-flex-layouts.json` (SHA-256 `8af0a6ced3bab8f4fc4b5d738d457477591ad1f3291710bd1f7e4596eed20b12`). The old `frontend/playwright/ui/render-wasm-specs/shapes.spec.js` references it in the test “Renders a file with flex layouts and different directions.” It is a source-controlled mocked API response, not an independently verified user-saved `.penpot` archive.

The sample advertises `fdata/objects-map`; its `~:data` page objects use the `~#penpot/objects-map/v2` wrapper and contain Transit-serialized shape strings, including flex directions `row`, `column`, `row-reverse`, and `column-reverse`. The current Go `NormalizeFileData` path does not recognize that Transit envelope. Its executable check asserts only that normalization is a no-op and generic Go JSON encode/decode retains the parsed payload. This is a **negative boundary contract**, not a claim of historical import, schema validity, flex/grid support, rendering fidelity, or end-to-end persistence. No fields are stripped or re-authored in the retained sample.

Run the focused checks from the repository root:

```bash
(cd backend-go && go test ./internal/filedata -run 'TestP0(CompatibilityFixture|HistoricalFlexLayoutEnvelope)' -count=1)
cargo test --manifest-path rust/Cargo.toml -p logos-types --locked p0_compatibility_fixture
```

The **cross-boundary** acceptance remains open until a supported loader/serializer is exercised through Go → React → Rust → render → save → reload and compares the same document identity and semantics.
