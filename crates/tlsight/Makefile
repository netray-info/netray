.PHONY: all build check test clippy fmt fmt-check lint ci clean \
       dev run help \
       frontend frontend-install frontend-dev frontend-test \
       test-rust test-frontend

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

# --- Production ---

all: frontend build ## Build frontend + release binary

build: frontend ## Build release binary (depends on frontend)
	cargo build --release

run: build ## Build and run release binary
	./target/release/tlsight

# --- Rust ---

check: ## Fast compile check (cargo check)
	cargo check

test-rust: ## Run Rust tests
	cargo test

clippy: ## Run clippy with -D warnings
	cargo clippy -- -D warnings

fmt: ## Format Rust code
	cargo fmt

fmt-check: ## Check Rust formatting
	cargo fmt -- --check

# --- Frontend ---

frontend-install: ## Install frontend dependencies (npm ci)
	cd frontend && npm ci

frontend: frontend-install ## Build frontend (npm ci + build)
	cd frontend && npm run build

frontend-dev: ## Start Vite dev server (:5173, proxies /api to :8080)
	cd frontend && npm run dev

frontend-test: frontend-install ## Run frontend tests (vitest)
	cd frontend && npx vitest run --passWithNoTests --environment node

# --- Combined ---

test: test-rust test-frontend ## Run all tests (Rust + frontend)

lint: clippy fmt-check ## Run all lints (clippy + fmt-check)

ci: lint test frontend ## Full CI pipeline (lint + test + frontend build)

# --- Development ---

dev: ## Run dev server with tlsight.dev.toml
	cargo run -- tlsight.dev.toml

clean: ## Remove target/, frontend/dist/, node_modules/
	cargo clean
	rm -rf frontend/dist frontend/node_modules
