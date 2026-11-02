---
title: 3.04. Shared Types & Codegen
desc: "Logos Technical Guide: the rust/logos-types crate as the canonical type source of truth and the TypeScript codegen pipeline."
---

# Shared Types & Codegen

Logos uses Rust as the single source of truth for all domain types. A codegen
step produces TypeScript `.d.ts` files from the Rust structs, eliminating
hand-maintained type duplication between the backend and frontend.

---

## Architecture

```
rust/logos-types/         Rust structs — canonical domain types
    src/
    ├── shape.rs           Shape, ShapeType, frame/group/path/text…
    ├── geometry.rs        Rect, Point, Matrix, Selrect, Transform
    ├── fill.rs            Fill, FillType, Gradient, GradientStop
    ├── stroke.rs          Stroke, StrokeType, StrokePosition
    ├── color.rs           Color (RGBA + library ref)
    ├── shadow.rs          Shadow, ShadowStyle
    ├── blur.rs            Blur, BlurType
    ├── token.rs           DesignToken, TokenType, TokenValue
    └── compat.rs          ChangeSet, Change, ChangeOp (CRDT compat shim)
         ↓
    make generate-rust-types
         ↓
logos-app/src/types/rust-generated/
    blur.ts  color.ts  compat.ts  fill.ts  geometry.ts
    index.ts  shadow.ts  shape.ts  stroke.ts  token.ts
```

---

## Running Codegen

```bash
# Generate TypeScript types from Rust structs
make generate-rust-types

# Check for drift (what CI runs — fails if generated files are out of date)
make generate-rust-types CHECK=1
```

If you change any type in `rust/logos-types/src/`, always regenerate and commit
both the Rust source and the TypeScript output in the same PR.

---

## Importing Types in TypeScript

```typescript
// logos-app/src/types/rust-generated/index.ts re-exports everything
import type { Shape, ShapeType, Fill, Stroke } from "@/types/rust-generated";
```

Do not import from the individual generated files — use the barrel `index.ts`.

---

## Adding a New Type

1. Add the Rust struct to the appropriate file in `rust/logos-types/src/`
2. Derive `serde::Serialize` + `serde::Deserialize` and add `#[serde(rename_all = "kebab-case")]`
3. Run `make generate-rust-types`
4. Commit both the Rust struct and the regenerated `.ts` files

Example:

```rust
// rust/logos-types/src/color.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub lib_color_id: Option<String>,
}
```

After codegen:

```typescript
// logos-app/src/types/rust-generated/color.ts (auto-generated)
export interface Color {
  r: number;
  g: number;
  b: number;
  a: number;
  "lib-color-id"?: string;
}
```

---

## Logging and Debugging

### Backend (Go)

The Go backend uses the standard library `log/slog` for structured logging:

```go
slog.Info("file updated",
    "file_id", fileID,
    "revn", revn,
    "changes", len(changes),
)
```

Log level is set via the `LOG_LEVEL` environment variable (`debug`, `info`, `warn`, `error`).
Logs stream to stdout in JSON format in production.

### Frontend (TypeScript)

The frontend uses `console` directly for development traces. Feature-gated debug
tools are available at `http://localhost:3449/dbg` during development.

Structured traces can be added anywhere in the TypeScript source:

```typescript
console.debug("[collab] rebase applied", { fileId, revn, changeCount });
```

---

## Testing Shared Logic

Types in `rust/logos-types/` are tested as part of the Rust workspace:

```bash
cd rust
cargo test -p logos-types
```

TypeScript type correctness is verified at compile time:

```bash
cd logos-app
npx tsc --noEmit
```
