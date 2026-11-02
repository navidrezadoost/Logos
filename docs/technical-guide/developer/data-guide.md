---
title: 3.08. Data Guide
desc: "Logos Data Guide: shape data model, TypeScript types, migrations, and file format."
---

# Data Guide

The data model is one of the most important parts of Logos. This guide explains how shapes
are represented, how data integrity is maintained, and what to check when modifying the model.

---

## Core Concepts

### Attribute optionality

Shape and page attributes are **optional by default**. The absence of an attribute implies
the default behavior; the presence activates a feature.

```typescript
// No fill = shape is not filled. Do not use null.
const shape: Shape = {
  id: "uuid",
  type: "rect",
  x: 0, y: 0, w: 100, h: 50,
  // fills: []   ← omitted, not null
};

// With a fill
const filled: Shape = {
  ...shape,
  fills: [{ type: "solid", color: { r: 1, g: 0, b: 0, a: 1 } }],
};
```

**Rules:**
- Never check `attribute === null` — check `attribute === undefined` (or use `?? default`)
- When removing a feature (e.g., clearing a fill), delete the attribute instead of setting it to `null`
- The import/export pipeline strips `null` and `undefined` values automatically

### Attribute naming

Attribute names use **kebab-case in JSON/serialization** (matching the Rust `serde` output)
and **camelCase in TypeScript** (mapped at the boundary). The canonical definition is in
`rust/logos-types/`:

```rust
// rust/logos-types/src/fill.rs
#[serde(rename_all = "kebab-case")]
pub struct Fill {
    pub fill_color: Option<Color>,
    pub fill_opacity: Option<f32>,
}
```

Generated TypeScript:

```typescript
export interface Fill {
  "fill-color"?: Color;
  "fill-opacity"?: number;
}
```

---

## Type Source of Truth

All domain types are defined in `rust/logos-types/`. See
[Shared Types & Codegen](./common.md) for the full codegen workflow.

The key types:

| Rust file | TypeScript output | Domain entity |
|---|---|---|
| `shape.rs` | `shape.ts` | Shape, ShapeType, frame/group/path/text |
| `geometry.rs` | `geometry.ts` | Rect, Point, Matrix, Transform |
| `fill.rs` | `fill.ts` | Fill, FillType, Gradient |
| `stroke.rs` | `stroke.ts` | Stroke, StrokeType |
| `color.rs` | `color.ts` | Color (RGBA + library ref) |
| `shadow.rs` | `shadow.ts` | Shadow, ShadowStyle |
| `blur.rs` | `blur.ts` | Blur, BlurType |
| `token.rs` | `token.ts` | DesignToken, TokenType |
| `compat.rs` | `compat.ts` | ChangeSet, Change (CRDT compat) |

---

## Adding a New Shape Attribute

Checklist when adding an attribute to the data model:

1. **Add to `rust/logos-types/`** — add the field to the appropriate struct with `Option<T>`
2. **Regenerate TypeScript** — `make generate-rust-types`
3. **Frontend rendering** — add rendering logic in `logos-app/src/design/` for the new attribute
4. **Properties panel** — add an input control in `logos-app/src/workspace/components/`
5. **Export/import** — update `backend-go/internal/binfile/v3.go` if the attribute needs to survive file round-trips
6. **Component sync** — if the attribute should propagate from main component to copies, register it in the sync attributes map
7. **Dev Mode (CSS export)** — update the code generation in `logos-app/src/workspace/inspect/` if the attribute has a CSS equivalent
8. **Migration** (if breaking change) — see [Data Migrations](#data-migrations) below

---

## Data Migrations

The file format version is stored in the `files.data` JSONB column as `{"version": N, ...}`.
When loading a file, if its version is lower than the current app version, migration
functions are applied in order to bring it up to date.

Migrations are implemented in TypeScript in `logos-app/src/migration/`:

```typescript
// logos-app/src/migration/migrations.ts

const MIGRATIONS: Migration[] = [
  // v1 → v2: rename fill-color to fill.color
  {
    version: 2,
    migrate(file) {
      for (const page of Object.values(file.pages)) {
        for (const shape of Object.values(page.objects)) {
          if (shape["fill-color"]) {
            shape.fills = [{ type: "solid", color: shape["fill-color"] }];
            delete shape["fill-color"];
          }
        }
      }
      return file;
    },
  },
  // ... add new migrations here
];
```

**Rules:**
- Migrations are **append-only** — never modify an existing migration
- Each migration must be idempotent — safe to run twice
- After a migration, save the file to update its version in the database
- Breaking changes require a migration; non-breaking additions do not

---

## Shape Edit Forms

Shape properties are edited in `logos-app/src/workspace/components/`. The panel is
organized by attribute group:

| Directory | Attributes |
|---|---|
| `fill/` | Fill color, gradient, image fill |
| `stroke/` | Stroke color, width, position, type |
| `shadow/` | Drop shadow, inner shadow |
| `blur/` | Layer blur, background blur |
| `transform/` | Position, size, rotation, flip |
| `layout/` | Flex/grid layout container settings |
| `text/` | Font family, size, weight, line height, letter spacing |
| `design-tokens/` | Token binding for any attribute |

---

## Multiple Selection

When multiple shapes are selected, the properties panel shows a merged view.
For each attribute:

- If all selected shapes share the **same value** → show the value (editable)
- If selected shapes have **different values** → show `mixed` placeholder

Editing a `mixed` attribute sets it to the new value on **all** selected shapes,
but only on shapes that can have that attribute (e.g., font size is only applied
to text shapes).

---

## File Export & Import

The `.logos` v3 ZIP format is documented in detail in the
[Backend Guide](./backend.md#file-format-logos). The key points for data integrity:

- Every attribute that must survive export/import must be in the `attrs.json` or `changes.json`
- The CRDT change history in `changes.json` is the authoritative source — `attrs.json` is derived
- Media objects referenced by shapes must be included in the `media/` and `objects/` sections of the ZIP

---

## Dev Mode — Code Generation

The **Dev Mode** panel in the properties inspector generates CSS, SVG variables, and
annotation links from the selected shape's attributes.

The code generation lives in `logos-app/src/workspace/inspect/codegen/`. Each attribute
group has its own generator:

| File | Output |
|---|---|
| `fill-css.ts` | `background: rgba(...)` / `background: linear-gradient(...)` |
| `stroke-css.ts` | `border: N px solid rgba(...)` |
| `shadow-css.ts` | `box-shadow: X Y blur spread rgba(...)` |
| `blur-css.ts` | `filter: blur(Npx)` |
| `text-css.ts` | `font-family`, `font-size`, `font-weight`, etc. |
| `layout-css.ts` | `display: flex`, `gap`, `align-items`, etc. |

When adding a new attribute with a CSS equivalent, add a generator here and register
it in the main code-generation router.
