#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(git rev-parse --show-toplevel)"
if [[ -z "${TEST_DATABASE_URL:-}" ]]; then
    echo "ERROR: set TEST_DATABASE_URL to a fresh disposable PostgreSQL database" >&2
    exit 2
fi

APP="$ROOT/logos-app"
ASSETS="$APP/build-p05-browser-persistence"
if [[ ! -x "$APP/node_modules/.bin/vite" ]]; then
    echo "ERROR: install the locked logos-app dependencies with npm ci before running this browser proof" >&2
    exit 2
fi

chrome_bin="${P05_BROWSER_BIN:-}"
if [[ -z "$chrome_bin" ]]; then
    for candidate in google-chrome google-chrome-stable chromium chromium-browser; do
        if command -v "$candidate" >/dev/null 2>&1; then
            chrome_bin="$(command -v "$candidate")"
            break
        fi
    done
fi
if [[ -z "$chrome_bin" ]]; then
    echo "ERROR: no headless Chrome/Chromium binary is available" >&2
    exit 2
fi

(
    cd "$APP"
    ./node_modules/.bin/vite build --config vite.p05-browser-persistence.config.ts
)

P05_BROWSER_ASSETS="$ASSETS" \
P05_BROWSER_BIN="$chrome_bin" \
bash "$ROOT/test/p0.4/document-contract/run-disposable-db-proof.sh"
