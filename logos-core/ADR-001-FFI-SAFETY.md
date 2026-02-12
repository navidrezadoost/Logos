# ADR-001: FFI Safety Boundaries
Date: 2026-02-12
Status: Accepted
Context: We expose a C API for potential integration with legacy code or other languages.
Decision:
  1. Use opaque types (Rep C struct with private field) for handles.
  2. All external functions MUST handle NULL pointers gracefully and return error codes where applicable.
  3. Use `Arc` for shared ownership of the Document.
  4. Explicit error reporting via `**char` out-parameters.
Consequences:
  - Safer interop.
  - Prevents segfaults from trivial null pointer usage.
  - Explicit lifetime management via `logos_document_free`.
