#!/usr/bin/env bash
set -Eeuo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_SCRIPT="$REPO_ROOT/render-wasm/build"
VALIDATOR="$REPO_ROOT/render-wasm/scripts/validate-legacy-editor-artifacts.sh"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/logos-p04-build-gate.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

expect_failure() {
    local name="$1"
    shift
    if "$@" >"$TMP/$name.log" 2>&1; then
        cat "$TMP/$name.log" >&2
        echo "ERROR: expected failure for $name" >&2
        exit 1
    fi
}

expect_failure missing-emsdk env EMSDK="$TMP/missing-emsdk" bash "$BUILD_SCRIPT"
grep -q 'refusing to skip required renderer build' "$TMP/missing-emsdk.log"

mkdir -p "$TMP/wrong-sdk/bin"
cat >"$TMP/wrong-sdk/emsdk_env.sh" <<EOF
export EMSDK="$TMP/wrong-sdk"
export PATH="$TMP/wrong-sdk/bin:\$PATH"
EOF
cat >"$TMP/wrong-sdk/bin/emcc" <<'EOF'
#!/usr/bin/env bash
echo 'emcc (Emscripten gcc/clang-like replacement + linker emulating GNU ld) 4.0.5'
EOF
chmod +x "$TMP/wrong-sdk/bin/emcc"
expect_failure wrong-emscripten-version env EMSDK="$TMP/wrong-sdk" bash "$BUILD_SCRIPT"
grep -q 'expected Emscripten 4.0.6; got 4.0.5' "$TMP/wrong-emscripten-version.log"

make_valid_bundle() {
    local root="$1"
    mkdir -p "$root/js/worker"
    for module in \
        main-auth.js main-dashboard.js main-settings.js main-viewer.js \
        main-workspace.js main.js rasterizer.js render-wasm.js render.js \
        shared.js util-highlight.js worker/main.js worker/render.js \
        config.js translation.en.js; do
        printf 'export const smoke = true;\n' >"$root/js/$module"
    done
    printf 'export const wasmUrl = "render-wasm.wasm?version=test";\n' >"$root/js/render-wasm.js"
    for wasm in render-wasm.wasm logos_layout_wasm.wasm logos_vector_wasm.wasm; do
        printf '\0asm\1\0\0\0' >"$root/js/$wasm"
    done
    {
        printf '<!doctype html><html><head><script type="importmap">{ "imports": {'
        local separator=""
        for module in \
            main-auth.js main-dashboard.js main-settings.js main-viewer.js \
            main-workspace.js main.js rasterizer.js render-wasm.js render.js \
            shared.js util-highlight.js; do
            printf '%s"./js/%s":"./js/%s"' "$separator" "$module" "$module"
            separator=,
        done
        printf '} }</script><script type="module">import "./js/main.js";</script>'
        printf '<link rel="preload" href="./js/worker/main.js"></head><body></body></html>\n'
    } >"$root/workspace.html"
}

GOOD="$TMP/good"
make_valid_bundle "$GOOD"
"$VALIDATOR" "$GOOD" >/dev/null

MISSING="$TMP/missing-module"
cp -a "$GOOD" "$MISSING"
rm "$MISSING/js/main-workspace.js"
expect_failure missing-main-workspace "$VALIDATOR" "$MISSING"
grep -q 'missing or empty required artifact: js/main-workspace.js' "$TMP/missing-main-workspace.log"

EMPTY="$TMP/empty-worker"
cp -a "$GOOD" "$EMPTY"
: >"$EMPTY/js/worker/main.js"
expect_failure empty-worker "$VALIDATOR" "$EMPTY"
grep -q 'missing or empty required artifact: js/worker/main.js' "$TMP/empty-worker.log"

MALFORMED_JS="$TMP/malformed-js"
cp -a "$GOOD" "$MALFORMED_JS"
printf 'export const = ;\n' >"$MALFORMED_JS/js/main.js"
expect_failure malformed-js "$VALIDATOR" "$MALFORMED_JS"
grep -q 'malformed JavaScript artifact: js/main.js' "$TMP/malformed-js.log"

MALFORMED_WASM="$TMP/malformed-wasm"
cp -a "$GOOD" "$MALFORMED_WASM"
printf 'not wasm' >"$MALFORMED_WASM/js/render-wasm.wasm"
expect_failure malformed-wasm "$VALIDATOR" "$MALFORMED_WASM"
grep -q 'malformed WebAssembly artifact: js/render-wasm.wasm' "$TMP/malformed-wasm.log"

INCOMPLETE="$TMP/incomplete-copy"
cp -a "$GOOD" "$INCOMPLETE"
rm "$INCOMPLETE/js/translation.en.js"
expect_failure incomplete-copy "$VALIDATOR" "$INCOMPLETE"
grep -q 'missing or empty required artifact: js/translation.en.js' "$TMP/incomplete-copy.log"

MALFORMED_HTML="$TMP/malformed-html"
cp -a "$GOOD" "$MALFORMED_HTML"
printf '<html><body>partial shell' >"$MALFORMED_HTML/workspace.html"
expect_failure malformed-html "$VALIDATOR" "$MALFORMED_HTML"
grep -q 'workspace shell has no module entrypoint' "$TMP/malformed-html.log"

INCOMPLETE_IMPORT_MAP="$TMP/incomplete-import-map"
cp -a "$GOOD" "$INCOMPLETE_IMPORT_MAP"
printf '<script type="module">import "./js/main.js";</script>\n' >"$INCOMPLETE_IMPORT_MAP/workspace.html"
expect_failure incomplete-import-map "$VALIDATOR" "$INCOMPLETE_IMPORT_MAP"
grep -q 'workspace shell import map is missing ./js/main-auth.js' "$TMP/incomplete-import-map.log"

echo "P0.4 build gate failure-path tests passed (synthetic fixture bundle only)."
