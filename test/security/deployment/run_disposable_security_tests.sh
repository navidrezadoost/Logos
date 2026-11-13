#!/usr/bin/env bash
set -Eeuo pipefail
set +x

ROOT="$(git rev-parse --show-toplevel)"
blocked() {
  printf 'BLOCKED: %s\n' "$1" >&2
  exit 2
}

[[ -n "${TEST_DATABASE_URL:-}" ]] || blocked "TEST_DATABASE_URL is required"
[[ -n "${TEST_REDIS_URL:-}" ]] || blocked "TEST_REDIS_URL is required"
[[ -n "${TEST_REDIS_DISPOSABLE:-}" ]] || blocked "TEST_REDIS_DISPOSABLE confirmation is required"
[[ -n "${DEPLOYMENT_ENVIRONMENT:-}" ]] || blocked "DEPLOYMENT_ENVIRONMENT confirmation is required"
[[ -n "${DEPLOYMENT_OWNER_AUTHORIZED:-}" ]] || blocked "DEPLOYMENT_OWNER_AUTHORIZED confirmation is required"
if [[ "$TEST_REDIS_DISPOSABLE" != "true" ]]; then
  blocked "TEST_REDIS_DISPOSABLE must be true"
fi
[[ "$DEPLOYMENT_ENVIRONMENT" == "nonproduction" ]] || blocked "DEPLOYMENT_ENVIRONMENT must be nonproduction"
[[ "$DEPLOYMENT_OWNER_AUTHORIZED" == "true" ]] || blocked "DEPLOYMENT_OWNER_AUTHORIZED must be true"

python3 <<'PY'
import ipaddress
import os
import sys
from urllib.parse import urlsplit

def blocked(message):
    print(f"BLOCKED: {message}", file=sys.stderr)
    raise SystemExit(2)

def parse_url(label, value):
    try:
        return urlsplit(value)
    except ValueError:
        blocked(f"{label} URL is malformed")

def loopback_host(label, host):
    try:
        loopback = host == "localhost" or ipaddress.ip_address(host or "").is_loopback
    except ValueError:
        loopback = False
    if not loopback:
        blocked(f"{label} test endpoint host must be loopback-only")

database_url = os.environ["TEST_DATABASE_URL"]
redis_url = os.environ["TEST_REDIS_URL"]
database = parse_url("PostgreSQL", database_url)
redis = parse_url("Redis", redis_url)

if database.scheme not in {"postgres", "postgresql"}:
    blocked("TEST_DATABASE_URL scheme must be postgres or postgresql")
loopback_host("PostgreSQL", database.hostname)
try:
    database_port = database.port
except ValueError:
    blocked("TEST_DATABASE_URL port is malformed")
if not database.username or not database.password or database_port is None:
    blocked("TEST_DATABASE_URL must include username, password, and port")
database_name = database.path.lstrip("/")
if not database_name.startswith("logos_p04") or "/" in database_name or not database_name:
    blocked("PostgreSQL database name must begin with logos_p04")

if redis.scheme not in {"redis", "rediss"}:
    blocked("TEST_REDIS_URL scheme must be redis or rediss")
loopback_host("Redis", redis.hostname)
try:
    redis.port
except ValueError:
    blocked("TEST_REDIS_URL port is malformed")
if redis.fragment:
    blocked("TEST_REDIS_URL fragment is not allowed")

print("Disposable test endpoint guards passed; no endpoint values recorded.")
PY

# The initializer rechecks URL shape and public-schema emptiness before applying
# migrations. These mechanical checks cannot attest that a service is isolated.
bash "$ROOT/test/p0.5/browser-persistence/initialize-disposable-db-schema.sh"

cd "$ROOT/backend-go"
go test -race ./internal/handler \
  -run 'Test(NotificationsWebSocket.*Integration|UpdateFileHandler.*Integration)$' \
  -count=1 -timeout=180s
