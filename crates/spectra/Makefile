.PHONY: all build run check test test-rust clippy fmt fmt-check \
       frontend frontend-install frontend-dev lint ci clean dev help

all: frontend build  ## Full production build (frontend + backend)

help:  ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# --- Rust ---

build:  ## Build release binary
	cargo build --release

run: all  ## Build + run release binary
	./target/release/spectra spectra.dev.toml

dev:  ## Run development server (debug build)
	cargo run -- spectra.dev.toml

check:  ## Fast compile check
	cargo check

test-rust:  ## Run Rust tests
	cargo test

clippy:  ## Run clippy lints
	cargo clippy -- -D warnings

fmt:  ## Format Rust code
	cargo fmt

fmt-check:  ## Check Rust formatting
	cargo fmt -- --check

# --- Frontend ---

frontend-install:  ## Install frontend dependencies
	cd frontend && npm ci

frontend: frontend-install  ## Build frontend
	cd frontend && npm run build

frontend-dev:  ## Run Vite dev server
	cd frontend && npm run dev

# --- Combined ---

test: test-rust  ## Run all tests
lint: clippy fmt-check  ## Run all lints
ci: lint test frontend  ## Full CI check

# --- Cleanup ---

clean:  ## Remove build artifacts
	rm -rf target/ frontend/dist/ frontend/node_modules/
