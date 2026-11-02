#!/bin/bash
# Logos CI test runner — Go + Rust + TypeScript
# Zero Clojure. Run from the repository root.
set -euo pipefail

echo "################ build + test logos-app (React/TypeScript) ################"
pushd logos-app
pnpm install
npx tsc --noEmit
npx vitest run
popd

echo "################ build + test Go backend ################"
pushd backend-go
go build ./...
go vet ./...
go test ./...
popd

echo "################ test render-wasm (Rust) ################"
pushd render-wasm
cargo fmt --check
./lint --debug
./test
popd

echo "################ test Rust workspace ################"
cargo test --workspace --manifest-path rust/Cargo.toml

echo "################ all checks passed ################"
