#!/usr/bin/env bash
set -Eeuo pipefail
set +x

ROOT="$(git rev-parse --show-toplevel)"
: "${TEST_DATABASE_URL:?provide a fresh loopback PostgreSQL test database}"
: "${TEST_REDIS_URL:?provide a disposable loopback Redis service}"
: "${TEST_REDIS_DISPOSABLE:?set TEST_REDIS_DISPOSABLE=true only for a disposable Redis instance}"
: "${DEPLOYMENT_ENVIRONMENT:?confirm the test environment classification}"
: "${DEPLOYMENT_OWNER_AUTHORIZED:?confirm deployment-owner authorization}"
if [[ "$TEST_REDIS_DISPOSABLE" != "true" ]]; then
  echo "BLOCKED: disposable Redis confirmation is required." >&2
  exit 2
fi
if [[ "$DEPLOYMENT_ENVIRONMENT" != "nonproduction" || "$DEPLOYMENT_OWNER_AUTHORIZED" != "true" ]]; then
  echo "BLOCKED: owner-authorized non-production execution is required." >&2
  exit 2
fi

python3 <<'PY'
from urllib.parse import urlsplit
import ipaddress
import os

for label, value in (("PostgreSQL", os.environ["TEST_DATABASE_URL"]), ("Redis", os.environ["TEST_REDIS_URL"])):
    parsed = urlsplit(value)
    try:
        loopback = parsed.hostname in {"localhost"} or ipaddress.ip_address(parsed.hostname or "").is_loopback
    except ValueError:
        loopback = False
    if not loopback:
        print(f"BLOCKED: {label} test endpoint must be loopback-only.", file=sys.stderr)
        raise SystemExit(2)

database = urlsplit(os.environ["TEST_DATABASE_URL"]).path.lstrip("/")
if not database.startswith("logos_p04"):
    print("BLOCKED: PostgreSQL database name must begin with logos_p04.", file=sys.stderr)
    raise SystemExit(2)
if urlsplit(os.environ["TEST_REDIS_URL"]).scheme not in {"redis", "rediss"}:
    print("BLOCKED: Redis URL scheme is invalid.", file=sys.stderr)
    raise SystemExit(2)
print("Disposable test endpoint guards passed; no endpoint values recorded.")
PY

# The initializer independently verifies a fresh public schema before applying
# migrations; it refuses non-loopback hosts and non-logos_p04 database names.
bash "$ROOT/test/p0.5/browser-persistence/initialize-disposable-db-schema.sh"

cd "$ROOT/backend-go"
go test -race ./internal/handler \
  -run 'Test(NotificationsWebSocket.*Integration|UpdateFileHandler.*Integration)$' \
  -count=1 -timeout=180s
