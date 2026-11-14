#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
commit="$(git rev-parse HEAD)"
if [[ -z "$(git status --porcelain)" ]]; then clean=true; else clean=false; fi
rustc_json="$(rustc -Vv | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')"
cargo_lock_sha256="$(sha256sum rust/Cargo.lock | cut -d' ' -f1)"
cpu="$(lscpu | sed -n 's/^Model name:[[:space:]]*//p' | head -n1)"
os="$(uname -srm)"
target="$(rustc -Vv | sed -n 's/^host: //p')"
printf '{"metadata":{"commit":"%s","working_tree_clean":%s,"rustc_verbose":%s,"cargo_lock_sha256":"%s","target":"%s","cpu":"%s","os":"%s","profile":"release","warmup_calls":1,"timed_samples":31,"allocation_sample_calls":1,"fixture_schema":"g1-b2-v1","retained_memory_measured":false}}\n' \
  "$commit" "$clean" "$rustc_json" "$cargo_lock_sha256" "$target" "$cpu" "$os"
cargo run --manifest-path rust/Cargo.toml --package logos-types --example document_core_bench --release --locked
