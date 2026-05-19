#!/usr/bin/env bash
# build-logos-vector-wasm.sh
# ──────────────────────────
# Compile the logos-vector-wasm crate to a WASM binary and copy it into the
# frontend public JS directory so the browser can fetch it at runtime.
#
# Usage:
#   ./scripts/build-logos-vector-wasm.sh [--debug]
#
# Environment:
#   CARGO   — path to cargo binary (default: cargo)
#   OUT_DIR — destination directory (default: ../frontend/resources/public/js)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$(cd "$SCRIPT_DIR/../rust" && pwd)"
FRONTEND_JS_DIR="${OUT_DIR:-"$SCRIPT_DIR/../frontend/resources/public/js"}"

CARGO="${CARGO:-cargo}"
PROFILE="wasm-release"
TARGET="wasm32-unknown-unknown"
CRATE="logos-vector-wasm"
BINARY="logos_vector_wasm.wasm"

if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="dev"
fi

echo "==> Building $CRATE ($TARGET / $PROFILE)"
cd "$RUST_DIR"
"$CARGO" build --target "$TARGET" --profile "$PROFILE" -p "$CRATE"

SRC="$RUST_DIR/target/$TARGET/$PROFILE/$BINARY"

if [[ ! -f "$SRC" ]]; then
    echo "ERROR: expected WASM binary at $SRC" >&2
    exit 1
fi

mkdir -p "$FRONTEND_JS_DIR"
cp "$SRC" "$FRONTEND_JS_DIR/$BINARY"

echo "==> Copied $BINARY to $FRONTEND_JS_DIR"
echo "    Size: $(du -h "$FRONTEND_JS_DIR/$BINARY" | cut -f1)"
