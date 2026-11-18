#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SPIKE="$ROOT/spikes/fg-c3-2-1"
OUT="$ROOT/target/fg-c3-2-1"
export CARGO_NET_OFFLINE=true
mkdir -p "$OUT"
export CARGO_TARGET_DIR="$OUT/cargo"

cargo fmt --manifest-path "$SPIKE/rust-planner/Cargo.toml" -- --check
cargo test --manifest-path "$SPIKE/rust-planner/Cargo.toml" --offline
cargo build --manifest-path "$SPIKE/rust-planner/Cargo.toml" --release
export FG_C3_2_2_RUST_PLANNER="$CARGO_TARGET_DIR/release/fg-c3-2-1-rust-planner-spike"
(
  cd "$SPIKE/go-coordinator"
  go test -v ./...
  go build -o "$OUT/go-coordinator" .
)

wasm-pack build "$SPIKE/rust-planner" --target nodejs --out-dir "$OUT/wasm-node" --out-name fg_c3_2_1_rust_planner_spike --features wasm
node "$SPIKE/compare-wasm.cjs" \
  "$OUT/wasm-node/fg_c3_2_1_rust_planner_spike.js" \
  "$OUT/go-coordinator" \
  "$FG_C3_2_2_RUST_PLANNER" \
  "$SPIKE/fixtures"

printf 'Local benchmark environment: '
date -u '+%Y-%m-%d %H:%M:%S UTC'
uname -srmo
printf 'Go: '
go version
printf 'Rust: '
rustc --version
printf 'CPU: '
awk -F: '/model name/ {gsub(/^ +/, "", $2); print $2; exit}' /proc/cpuinfo
printf 'Node: '
node --version
printf 'wasm-pack: '
wasm-pack --version
"$OUT/go-coordinator" benchmark "$SPIKE/fixtures/reorder.json"

printf 'FG-C3.2.2 isolated service/protocol qualification completed.\n'
