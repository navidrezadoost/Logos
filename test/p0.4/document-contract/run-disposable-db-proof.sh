#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(git rev-parse --show-toplevel)"
if [[ -z "${TEST_DATABASE_URL:-}" ]]; then
    echo "ERROR: set TEST_DATABASE_URL to a fresh disposable PostgreSQL database" >&2
    exit 2
fi
python3 - "$TEST_DATABASE_URL" <<'PY'
from urllib.parse import urlparse
import sys

url = urlparse(sys.argv[1])
database = url.path.lstrip("/")
if url.hostname not in {"127.0.0.1", "localhost", "::1"}:
    raise SystemExit("ERROR: P0.4 persistence proof only permits a loopback PostgreSQL host")
if not database.startswith("logos_p04"):
    raise SystemExit("ERROR: P0.4 persistence proof requires a database name beginning with logos_p04")
PY

TMP="$(mktemp -d "${TMPDIR:-/tmp/opencode}/logos-p04-document-proof.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

node --experimental-strip-types --test "$ROOT/test/p0.4/document-contract/adapter.test.mjs"
node --experimental-strip-types "$ROOT/test/p0.4/document-contract/prepare.mjs" "$TMP"

(
    cd "$ROOT/backend-go"
    TEST_DATABASE_URL="$TEST_DATABASE_URL" \
    P04_UPDATE_FILE="$TMP/request.json" \
    P04_BEFORE_OUT="$TMP/before.json" \
    P04_RELOAD_OUT="$TMP/reload.json" \
    go test ./internal/handler -run '^TestP04FixturePersistenceSaveReloadIntegration$' -count=1 -timeout=60s -v
)

node --experimental-strip-types "$ROOT/test/p0.4/document-contract/verify.mjs" "$TMP"
