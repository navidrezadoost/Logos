## Logos — top-level build orchestrator
##
## Targets
## ───────
##  make build          Build everything (Rust native + WASM, ClojureScript deps)
##  make build-rust     Build Rust crates (native release) in rust/
##  make build-wasm     Build logos-layout WASM target with wasm-pack
##  make test           Run all Rust unit tests in rust/
##  make test-rust      Same as test
##  make clean          Remove all build artefacts
##  make fmt            Format Rust sources with rustfmt
##  make lint           Check Rust sources with clippy
##
## Prerequisites
## ─────────────
##  cargo      (rustup: https://rustup.rs)
##  wasm-pack  (cargo install wasm-pack)  — only for build-wasm / build targets
##
## WASM output lands in:
##  frontend/pkg/logos-layout/

CARGO        := cargo
WASM_PACK    := wasm-pack
RUST_DIR     := rust
LAYOUT_CRATE := $(RUST_DIR)/logos-layout
WASM_OUT     := frontend/pkg/logos-layout

.PHONY: build build-rust build-wasm test test-rust clean fmt lint

## ── Default target ──────────────────────────────────────────────

build: build-rust build-wasm

## ── Rust native build ───────────────────────────────────────────

build-rust:
	@echo "==> Building Rust workspace (native release)"
	$(CARGO) build --release --manifest-path $(RUST_DIR)/Cargo.toml

## ── WASM build ──────────────────────────────────────────────────

build-wasm:
	@echo "==> Building logos-layout WASM module"
	$(WASM_PACK) build $(LAYOUT_CRATE) \
		--release \
		--target web \
		--out-dir ../../$(WASM_OUT) \
		-- --features wasm

## ── Tests ───────────────────────────────────────────────────────

test test-rust:
	@echo "==> Running Rust tests"
	$(CARGO) test --manifest-path $(RUST_DIR)/Cargo.toml

## ── Formatting / linting ────────────────────────────────────────

fmt:
	@echo "==> Formatting Rust sources"
	$(CARGO) fmt --manifest-path $(RUST_DIR)/Cargo.toml --all

lint:
	@echo "==> Linting Rust sources (clippy)"
	$(CARGO) clippy --manifest-path $(RUST_DIR)/Cargo.toml --all-targets --all-features -- -D warnings

## ── Clean ───────────────────────────────────────────────────────

clean:
	@echo "==> Cleaning Rust build artefacts"
	$(CARGO) clean --manifest-path $(RUST_DIR)/Cargo.toml
	rm -rf $(WASM_OUT)
