#!/usr/bin/env bash
set -Eeuo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <distribution-root>" >&2
    exit 2
fi

DIST_ROOT="$(cd "$1" && pwd)"
JS_DIR="$DIST_ROOT/js"

modules=(
    main-auth.js
    main-dashboard.js
    main-settings.js
    main-viewer.js
    main-workspace.js
    main.js
    rasterizer.js
    render-wasm.js
    render.js
    shared.js
    util-highlight.js
)
workers=(worker/main.js worker/render.js)
wasm=(render-wasm.wasm logos_layout_wasm.wasm logos_vector_wasm.wasm)
assets=(config.js translation.en.js)

require_nonempty() {
    local file="$1"
    if [[ ! -s "$file" ]]; then
        echo "ERROR: missing or empty required artifact: ${file#"$DIST_ROOT"/}" >&2
        return 1
    fi
}

for module in "${modules[@]}" "${workers[@]}" "${assets[@]}"; do
    require_nonempty "$JS_DIR/$module"
    if ! node --input-type=module --check < "$JS_DIR/$module" >/dev/null; then
        echo "ERROR: malformed JavaScript artifact: js/$module" >&2
        exit 1
    fi
done

for module in "${wasm[@]}"; do
    file="$JS_DIR/$module"
    require_nonempty "$file"
    if ! node --input-type=module -e \
        'import { readFileSync } from "node:fs"; await WebAssembly.compile(readFileSync(process.argv[1]));' \
        "$file" >/dev/null 2>&1; then
        echo "ERROR: malformed WebAssembly artifact: js/$module" >&2
        exit 1
    fi
done

workspace="$DIST_ROOT/workspace.html"
require_nonempty "$workspace"
python3 - "$workspace" "${modules[@]}" <<'PY'
from html.parser import HTMLParser
from pathlib import Path
import sys

path = Path(sys.argv[1])
modules = sys.argv[2:]
source = path.read_text(encoding="utf-8")

class CheckHTML(HTMLParser):
    has_module_script = False
    def handle_starttag(self, tag, attrs):
        if tag == "script" and dict(attrs).get("type") == "module":
            self.has_module_script = True

parser = CheckHTML()
parser.feed(source)
if not parser.has_module_script:
    raise SystemExit(f"ERROR: workspace shell has no module entrypoint: {path}")
for module in modules:
    if f"./js/{module}" not in source:
        raise SystemExit(f"ERROR: workspace shell import map is missing ./js/{module}")
if "./js/worker/main.js" not in source:
    raise SystemExit("ERROR: workspace shell is missing its main worker URL")
PY

echo "Legacy editor artifact contract passed: $DIST_ROOT"
