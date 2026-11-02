## Logos — top-level build orchestrator
##
## Targets
## ───────
##  make build               Build Rust native + WASM
##  make build-app           Build logos-app (React + TypeScript) production bundle
##  make build-rust          Build Rust crates (native release) in rust/
##  make build-wasm          Build logos-layout WASM, output → logos-app/public/logos-layout/
##  make build-go-backend    Build the Go backend binary into backend-go/bin/server
##  make run-go-backend      Start the Go backend server (port 8080)
##  make test                Run all Rust unit tests in rust/
##  make test-rust           Same as test
##  make test-go             Run Go backend tests
##  make generate-rust-types Regenerate logos-app/src/types/rust-generated/*.ts from Rust
##  make clean               Remove all build artefacts
##  make fmt                 Format Rust + Go sources
##  make lint                Check Rust (clippy) + Go (vet) sources
##
## Prerequisites
## ─────────────
##  cargo      (rustup: https://rustup.rs)
##  wasm-pack  (cargo install wasm-pack)  — only for build-wasm / build targets
##  go 1.23+   (go.dev/dl)               — only for go-backend targets
##
## WASM output lands in:
##  logos-app/public/logos-layout/

CARGO        := cargo
WASM_PACK    := wasm-pack
RUST_DIR     := rust
LAYOUT_CRATE := $(RUST_DIR)/logos-layout
WASM_OUT     := logos-app/public/logos-layout
GOBACKEND_DIR := backend-go
GOBIN        := go

.PHONY: build build-app build-rust build-wasm build-go-backend test test-rust test-go generate-rust-types run-go-backend go-mod-tidy clean fmt lint

## ── Default target ──────────────────────────────────────────────
## Add 'build-app' to also build the React frontend

build: build-rust build-wasm

build-app:
	@echo "==> Building logos-app (React + TypeScript)"
	cd logos-app && npm run build:spa

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

## ── TypeScript type generation ─────────────────────────────────
##
## Runs the Rust-based TS type generator and writes
## logos-app/src/types/rust-generated/*.ts
## Usage:
##   make generate-rust-types
##   make generate-rust-types CHECK=1  (exit non-zero if files changed)

## ── Go backend ───────────────────────────────────────────────────────────────
## Go backend server (Phase G3). Requires DATABASE_URL in environment.
run-go-backend:
	@echo "==> Starting Go backend (port 8080)…"
	cd $(GOBACKEND_DIR) && $(GOBIN) run ./cmd/server

## Build the Go backend binary into backend-go/bin/server.
build-go-backend:
	@echo "==> Building Go backend binary…"
	cd $(GOBACKEND_DIR) && $(GOBIN) build -o bin/server ./cmd/server

## Download Go module dependencies.
go-mod-tidy:
	cd $(GOBACKEND_DIR) && $(GOBIN) mod tidy

generate-rust-types:
	@echo "==> Generating TypeScript types from logos-types (Rust)"
	$(CARGO) run \
		--manifest-path $(RUST_DIR)/Cargo.toml \
		--package logos-types \
		--bin generate-types \
		--features logos-types/ts
	@if [ "$$CHECK" = "1" ]; then \
		git diff --exit-code logos-app/src/types/rust-generated/ || \
			(echo "ERROR: Generated types changed. Run make generate-rust-types and commit."; exit 1); \
	fi

## ── Tests ───────────────────────────────────────────────────────

test test-rust:
	@echo "==> Running Rust tests"
	$(CARGO) test --manifest-path $(RUST_DIR)/Cargo.toml

test-go:
	@echo "==> Running Go backend tests"
	cd $(GOBACKEND_DIR) && $(GOBIN) test ./...

## ── Formatting / linting ────────────────────────────────────────

fmt:
	@echo "==> Formatting Rust sources"
	$(CARGO) fmt --manifest-path $(RUST_DIR)/Cargo.toml --all
	@echo "==> Formatting Go sources"
	cd $(GOBACKEND_DIR) && $(GOBIN) fmt ./...

lint:
	@echo "==> Linting Rust sources (clippy)"
	$(CARGO) clippy --manifest-path $(RUST_DIR)/Cargo.toml --all-targets --all-features -- -D warnings
	@echo "==> Vetting Go sources"
	cd $(GOBACKEND_DIR) && $(GOBIN) vet ./...

## ── Clean ───────────────────────────────────────────────────────

clean:
	@echo "==> Cleaning Rust build artefacts"
	$(CARGO) clean --manifest-path $(RUST_DIR)/Cargo.toml
	rm -rf $(WASM_OUT)
	@echo "==> Cleaning logos-app build output"
	rm -rf logos-app/build
