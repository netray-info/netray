# tlsight — Top-level Makefile
# https://github.com/lukaspustina/tlsight

SHELL       := /bin/bash
.DEFAULT_GOAL := all

# ── Project metadata ─────────────────────────────────────────────
APP         := tlsight
VERSION     := $(shell grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/' 2>/dev/null || echo "unknown")
GIT_SHA     := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
DOCKER_TAG  := $(APP):$(VERSION)

# ── Tools ────────────────────────────────────────────────────────
CARGO       := cargo
NPM         := npm

# ── Directories ──────────────────────────────────────────────────
FRONTEND_DIR := frontend
DIST_DIR     := $(FRONTEND_DIR)/dist

# ── Flags (override from CLI: make CARGO_FLAGS=--release build) ──
CARGO_FLAGS  ?=
NPM_CI_FLAGS ?=

# ── Phony targets ────────────────────────────────────────────────
.PHONY: all build check test lint ci pre-push clean dev run \
        frontend frontend-install frontend-dev frontend-test \
        test-rust test-frontend \
        fmt fmt-check clippy \
        docker docker-run \
        data \
        acceptance \
        help

# ══════════════════════════════════════════════════════════════════
#  Help
# ══════════════════════════════════════════════════════════════════

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*## ' $(MAKEFILE_LIST) | \
		awk -F ':.*## ' '{printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}' | sort

# ══════════════════════════════════════════════════════════════════
#  Production
# ══════════════════════════════════════════════════════════════════

all: frontend build ## Build frontend + release binary

build: frontend ## Build release binary (depends on frontend)
	$(CARGO) build --release $(CARGO_FLAGS)

run: build ## Build and run release binary
	./target/release/$(APP)

# ══════════════════════════════════════════════════════════════════
#  Rust
# ══════════════════════════════════════════════════════════════════

check: ## Fast compile check (cargo check)
	$(CARGO) check $(CARGO_FLAGS)

test-rust: ## Run Rust tests
	$(CARGO) test $(CARGO_FLAGS)

clippy: ## Run clippy with -D warnings
	$(CARGO) clippy $(CARGO_FLAGS) -- -D warnings

fmt: ## Format Rust code
	$(CARGO) fmt

fmt-check: ## Check Rust formatting
	$(CARGO) fmt -- --check

# ══════════════════════════════════════════════════════════════════
#  Frontend
# ══════════════════════════════════════════════════════════════════

frontend-install: ## Install frontend dependencies (npm ci)
	cd $(FRONTEND_DIR) && $(NPM) ci $(NPM_CI_FLAGS)

frontend: frontend-install ## Build frontend (npm ci + build)
	cd $(FRONTEND_DIR) && $(NPM) run build

frontend-dev: ## Start Vite dev server (:5174, proxies /api to :8081)
	cd $(FRONTEND_DIR) && $(NPM) run dev

frontend-test: frontend-install ## Run frontend tests (vitest)
	cd $(FRONTEND_DIR) && npx vitest run --passWithNoTests --environment node

# ══════════════════════════════════════════════════════════════════
#  Combined
# ══════════════════════════════════════════════════════════════════

test: test-rust test-frontend ## Run all tests (Rust + frontend)

test-frontend: frontend-test ## Alias for frontend-test

lint: clippy fmt-check ## Run all lints (clippy + fmt-check)

ci: lint test frontend ## Full CI pipeline (lint + test + frontend build)

# NOTE: NODE_AUTH_TOKEN must be exported in your shell before running pre-push
#       (required for npm ci to authenticate against GitHub Packages).
#       Example: export NODE_AUTH_TOKEN=$(gh auth token)
pre-push: fmt-check clippy test frontend ## Run all checks locally before pushing (fmt-check → clippy → test → frontend)
	@echo ""
	@echo "All checks passed. Safe to push."

# ══════════════════════════════════════════════════════════════════
#  Development
# ══════════════════════════════════════════════════════════════════

dev: ## Run dev server with tlsight.dev.toml
	$(CARGO) run $(CARGO_FLAGS) -- tlsight.dev.toml

data: ## Fetch and process CA/CAA data into data/caa_domains.tsv
	$(MAKE) -C data

clean: ## Remove target/, frontend/dist/, node_modules/
	$(CARGO) clean
	rm -rf $(DIST_DIR) $(FRONTEND_DIR)/node_modules

# ══════════════════════════════════════════════════════════════════
#  Docker
# ══════════════════════════════════════════════════════════════════

docker: ## Build Docker image (ghcr.io/lukaspustina/tlsight:latest)
	docker build -t ghcr.io/lukaspustina/$(APP):latest .

docker-run: ## Run Docker image locally (port 8081)
	docker run --rm -p 8081:8081 -p 9090:9090 ghcr.io/lukaspustina/$(APP):latest

# ══════════════════════════════════════════════════════════════════
#  E2E Tests
# ══════════════════════════════════════════════════════════════════

acceptance: ## Run Playwright E2E tests
	$(MAKE) -C tests acceptance
