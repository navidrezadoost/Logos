#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(git rev-parse --show-toplevel)"
: "${TEST_DATABASE_URL:?set TEST_DATABASE_URL to a fresh, loopback-only database}"
python3 - "$ROOT" <<'PY'
from pathlib import Path
from urllib.parse import unquote, urlparse
import os
import re
import subprocess
import sys

root = Path(sys.argv[1])
legacy_commit = "665bb1ef13c3eeff2e84bf6e0d588a6bdecc6a92"
parsed = urlparse(os.environ["TEST_DATABASE_URL"])
database = parsed.path.lstrip("/")
if parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
    raise SystemExit("ERROR: test schema initialization only permits loopback PostgreSQL")
if not database.startswith("logos_p04"):
    raise SystemExit("ERROR: test schema initialization requires a database name beginning with logos_p04")
if not parsed.username or not parsed.password or not parsed.port:
    raise SystemExit("ERROR: TEST_DATABASE_URL must include user, password, port, and database")

env = dict(os.environ, PGPASSWORD=unquote(parsed.password))
psql = [
    "psql", "-X", "-v", "ON_ERROR_STOP=1", "-q", "-h", parsed.hostname,
    "-p", str(parsed.port), "-U", unquote(parsed.username), "-d", database,
]

def scalar(query):
    result = subprocess.run(psql + ["-At", "-c", query], env=env, check=True, capture_output=True, text=True)
    return result.stdout.strip()

if scalar("SELECT to_regclass('public.profile') IS NULL") != "t":
    raise SystemExit("ERROR: target database is not fresh (profile table already exists)")
if scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public'") != "0":
    raise SystemExit("ERROR: target database is not fresh (public schema already contains tables)")

registry = subprocess.run(
    ["git", "show", f"{legacy_commit}:backend/src/app/migrations.clj"],
    cwd=root, check=True, capture_output=True, text=True,
).stdout
steps = re.findall(r'\{:name "([^"]+)"\s+:fn \(mg/resource "([^"]+)"\)\}', registry)
if len(steps) != 149:
    raise SystemExit(f"ERROR: expected 149 registered historical SQL migrations, found {len(steps)}")

for name, resource in steps:
    sql = subprocess.run(
        ["git", "show", f"{legacy_commit}:backend/src/{resource}"],
        cwd=root, check=True, capture_output=True,
    ).stdout
    subprocess.run(psql, input=sql, env=env, check=True, stdout=subprocess.DEVNULL)
    if name == "0022-page-file-refactor":
        # Clojure migration 0023 migrates page rows to file.data, then drops
        # page. This disposable database is fresh, so only the DDL effect is
        # required; assert no data is present before dropping it.
        if scalar("SELECT count(*) FROM file") != "0" or scalar("SELECT count(*) FROM page") != "0":
            raise SystemExit("ERROR: refusing Clojure migration 0023's empty-database effect on non-empty tables")
        subprocess.run(psql + ["-c", "DROP TABLE page CASCADE;"], env=env, check=True, stdout=subprocess.DEVNULL)

# Clojure migration 0145 rewrites existing profile plugin URLs. It is a no-op
# on this fresh database; assert that before applying the Go proof migration.
if scalar("SELECT count(*) FROM profile") != "0":
    raise SystemExit("ERROR: expected no profile rows before skipping data-only migration 0145")

go_migration = root / "backend-go/migrations/0001_project_description_photo.sql"
subprocess.run(psql + ["-f", str(go_migration)], env=env, check=True, stdout=subprocess.DEVNULL)
print("Initialized fresh test-only schema from 149 registered historical SQL steps, empty-data migration effects 0023/0145, and Go migration 0001.")
PY
