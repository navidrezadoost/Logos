#!/usr/bin/env bash
set -Eeuo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EMSDK_ROOT="${EMSDK:-/opt/emsdk}"
EXPECTED_EMSDK_COMMIT="24fc909c0da13ef641d5ae75e89b5a97f25e37aa"
EXPECTED_EMSCRIPTEN_VERSION="4.0.6"
EXPECTED_RUST_VERSION="1.93.0"
EXPECTED_PNPM_VERSION="10.28.0"
ASSET_DIR="$REPO_ROOT/logos-app/dist/js"

fail() {
    printf 'BLOCKED: %s\n' "$1" >&2
    exit 1
}

if [[ ! -r "$EMSDK_ROOT/emsdk_env.sh" || ! -d "$EMSDK_ROOT/.git" ]]; then
    fail "pinned Emscripten SDK checkout unavailable at $EMSDK_ROOT"
fi
actual_emsdk_commit="$(git -C "$EMSDK_ROOT" rev-parse HEAD 2>/dev/null)" || fail "cannot identify Emscripten SDK checkout"
[[ "$actual_emsdk_commit" == "$EXPECTED_EMSDK_COMMIT" ]] || fail "expected emsdk commit $EXPECTED_EMSDK_COMMIT; got $actual_emsdk_commit"
sdk_status="$(git -C "$EMSDK_ROOT" status --porcelain --untracked-files=all)"
[[ -z "$sdk_status" ]] || fail "pinned Emscripten SDK checkout contains local modifications"

command -v cmake >/dev/null 2>&1 || fail "CMake is required by the renderer build but is unavailable"
command -v corepack >/dev/null 2>&1 || fail "Corepack is required to install the renderer workspace's pinned pnpm"
actual_rust_version="$(rustc --version 2>/dev/null | awk '{print $2}')" || fail "Rust compiler unavailable"
actual_cargo_version="$(cargo --version 2>/dev/null | awk '{print $2}')" || fail "Cargo unavailable"
if ! rustup target list --installed 2>/dev/null | grep -qx 'wasm32-unknown-emscripten'; then
    fail "Rust target wasm32-unknown-emscripten is not installed"
fi
[[ "$actual_cargo_version" == "$EXPECTED_RUST_VERSION" ]] || fail "expected cargo $EXPECTED_RUST_VERSION; got $actual_cargo_version"
[[ "$actual_rust_version" == "$EXPECTED_RUST_VERSION" ]] || fail "expected rustc $EXPECTED_RUST_VERSION; got ${actual_rust_version:-unknown}"

source_status="$(git -C "$REPO_ROOT" status --porcelain --untracked-files=all)"
[[ -z "$source_status" ]] || fail "source worktree must be clean before renderer build"

# Remove previous output so a failed build cannot leave stale renderer files.
rm -f "$ASSET_DIR/render-wasm.js" "$ASSET_DIR/render-wasm.wasm" \
    "$ASSET_DIR/worker/render.js" "$ASSET_DIR/shared.js" \
    "$ASSET_DIR/render-wasm-provenance.json"

export EMSDK="$EMSDK_ROOT"
export NODE_ENV=production
"$REPO_ROOT/render-wasm/build"

EMSDK_QUIET=1 source "$EMSDK_ROOT/emsdk_env.sh"
actual_emscripten_version="$(emcc --version 2>&1 | grep -Eo '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1 || true)"
actual_pnpm_version="$(pnpm --version 2>/dev/null)" || fail "pinned pnpm was not installed by Corepack"
[[ "$actual_pnpm_version" == "$EXPECTED_PNPM_VERSION" ]] || fail "expected pnpm $EXPECTED_PNPM_VERSION; got $actual_pnpm_version"
actual_cmake_version="$(cmake --version | awk 'NR == 1 {print $3}')"
actual_node_version="$(node --version)"
[[ "$actual_emscripten_version" == "$EXPECTED_EMSCRIPTEN_VERSION" ]] || fail "expected Emscripten $EXPECTED_EMSCRIPTEN_VERSION after build; got ${actual_emscripten_version:-unknown}"

node "$REPO_ROOT/test/benchmarks/g1-b9-6-renderer-preflight.mjs" write-provenance \
    "$ASSET_DIR" "$actual_emsdk_commit" "$actual_emscripten_version" "$actual_rust_version" \
    "$actual_cargo_version" "$actual_pnpm_version" "$actual_cmake_version" "$actual_node_version"
node "$REPO_ROOT/test/benchmarks/g1-b9-6-renderer-preflight.mjs" verify-assets \
    "$ASSET_DIR" "$(git -C "$REPO_ROOT" rev-parse HEAD):$(git -C "$REPO_ROOT" rev-parse 'HEAD^{tree}')"

printf 'RENDERER_BUILD_VERIFIED source=%s:%s emsdk=%s emscripten=%s rustc=%s cargo=%s pnpm=%s cmake=%s node=%s\n' \
    "$(git -C "$REPO_ROOT" rev-parse HEAD)" "$(git -C "$REPO_ROOT" rev-parse 'HEAD^{tree}')" \
    "$actual_emsdk_commit" "$actual_emscripten_version" "$actual_rust_version" "$actual_cargo_version" "$actual_pnpm_version" \
    "$actual_cmake_version" "$actual_node_version"
