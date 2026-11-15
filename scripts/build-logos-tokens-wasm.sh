#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_DIR="$(cd "$SCRIPT_DIR/../rust" && pwd)"
APP_JS_DIR="${OUT_DIR:-"$SCRIPT_DIR/../logos-app/dist/js"}"
CARGO="${CARGO:-cargo}"
TARGET="wasm32-unknown-unknown"
PROFILE="wasm-release"
ARTIFACT="$RUST_DIR/target/$TARGET/$PROFILE/logos_tokens_wasm.wasm"

cd "$RUST_DIR"
"$CARGO" build --target "$TARGET" --profile "$PROFILE" -p logos-tokens-wasm --locked
test -s "$ARTIFACT" || { echo "ERROR: missing token engine WASM artifact: $ARTIFACT" >&2; exit 1; }
mkdir -p "$APP_JS_DIR"
cp "$ARTIFACT" "$APP_JS_DIR/logos_tokens_wasm.wasm"
echo "Built and copied Rust token engine to $APP_JS_DIR/logos_tokens_wasm.wasm"
