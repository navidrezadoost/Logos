# G0-B characterization fixtures

These fixtures record boundary behavior from the audit baseline. They are
inputs for the Rust shape/serialization characterization tests or the existing
A8 renderer-projection tests; they do not define an approved durable document
schema.

- `react-shape-projection.json` is representative of the React `Shape` shape
  (`bounds`, `children`, and UI flags). Rust `logos_types::Shape` currently
  requires `selrect`, uses `shapes`, and does not define those UI extensions.
  The characterization test records that direct Rust deserialization rejects
  this object.
- `graph-invalid-cases.json` enumerates small duplicate, dangling, cycle and
  parent/backlink examples. The existing A8 projection is the executable
  renderer-boundary validator. The examples are not accepted as evidence that
  the canonical editor document store validates these cases.
- Layout numeric behavior is covered by `rust/logos-layout/tests/g0b_layout_determinism.rs`.
