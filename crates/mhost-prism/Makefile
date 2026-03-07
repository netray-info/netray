.PHONY: all build check test clippy fmt frontend clean dev run pre-push

all: frontend build

# Rust
build: frontend
	cargo build --release

check:
	cargo check

test:
	cargo test

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

fmt-check:
	cargo fmt -- --check

# Frontend
frontend:
	cd frontend && npm ci && npm run build

frontend-dev:
	cd frontend && npm run dev

# Combined
dev:
	cargo run -- prism.dev.toml

lint: clippy fmt-check

# Matches .github/workflows/ci.yml — run before pushing
pre-push: fmt-check clippy test frontend

ci: lint test frontend build

clean:
	cargo clean
	rm -rf frontend/dist frontend/node_modules

run: build
	./target/release/prism
