# The verb contract of lens (pdt-adlc ADR 0008).
#
# Migrated from a Makefile on 2026-08-18, by reduction: six of 24 targets are
# gone — help (`just --list` builds the listing), all (frontend + build, which
# `build` already says), the aggregates ci and pre-push (the contract names
# their contents `check` and `adlc-verify`), and the alias test-frontend.
#
# The removal that matters: `check` was `cargo check` — a compile, no test run —
# and the ADLC contract resolver preferred a target of that name, so every
# attestation this repository produced proved only that the code COMPILES
# (pdt-adlc backlog I14, found in seven netray repositories on one afternoon).
#
# adlc-verify is fmt-check + clippy + the Rust suite, offline. Two things stay
# out of it: the frontend, whose `npm ci` needs NODE_AUTH_TOKEN for GitHub
# Packages, and `deny`, which fetches the advisory database — repo-contract
# requirement 4 rules both out of a gate.
#
# What the gate depends on is frontend/dist: src/spa.rs embeds it with RustEmbed
# and the LIBRARY DOES NOT COMPILE without it — measured on 2026-08-18, when
# clippy and cargo test were both red here for exactly that reason and for no
# other. dist is gitignored, so a clone has to run `just frontend` once.
# check-frontend-dist says that instead of leaving a reader with a compiler
# error about a missing folder.

app          := "lens"
cargo        := "cargo"
npm          := "npm"
frontend_dir := "frontend"
docker_tag   := "ghcr.io/netray-info/lens:latest"

default: adlc-verify

# --- the contract ------------------------------------------------------------

# What the ADLC gate runs: fmt-check, clippy, the Rust suite. No network.
adlc-verify: check-frontend-dist lint test-rust

# Everything: lint, cargo-deny, all tests, and the frontend build.
check: lint deny test frontend

# The whole suite — Rust and frontend.
test: test-rust frontend-test

# clippy + fmt-check.
lint: clippy fmt-check

# --- preconditions -----------------------------------------------------------

# src/spa.rs embeds frontend/dist (RustEmbed); without it nothing compiles.
check-frontend-dist:
    #!/usr/bin/env bash
    if [ ! -d "{{frontend_dir}}/dist" ]; then
        echo "{{frontend_dir}}/dist is missing — src/spa.rs embeds it (RustEmbed), so the" >&2
        echo "  library does not compile without it. Run 'just frontend' once;" >&2
        echo "  it needs NODE_AUTH_TOKEN for GitHub Packages." >&2
        exit 1
    fi

# --- rust --------------------------------------------------------------------

# The Rust suite.
test-rust:
    {{cargo}} test

clippy:
    {{cargo}} clippy -- -D warnings

fmt-check:
    {{cargo}} fmt -- --check

fmt:
    {{cargo}} fmt

# cargo-deny licence and advisory checks (network: advisory database).
deny:
    {{cargo}} deny check

# --- build -------------------------------------------------------------------

# Release binary (builds the frontend first — it is embedded).
build: frontend
    {{cargo}} build --release

# Build and run the release binary.
run: build
    ./target/release/{{app}}

# Dev server with lens.dev.toml.
dev:
    {{cargo}} run -- lens.dev.toml

clean:
    {{cargo}} clean
    rm -rf {{frontend_dir}}/dist {{frontend_dir}}/node_modules

# --- frontend (network: GitHub Packages needs NODE_AUTH_TOKEN) ---------------

frontend-install:
    cd {{frontend_dir}} && {{npm}} ci

# npm ci + vite build.
frontend: frontend-install
    cd {{frontend_dir}} && {{npm}} run build

# Vite dev server with API proxy.
frontend-dev:
    cd {{frontend_dir}} && {{npm}} run dev

# vitest.
frontend-test: frontend-install
    cd {{frontend_dir}} && npx vitest run --passWithNoTests --environment node

# --- docker ------------------------------------------------------------------

docker:
    docker build -t {{docker_tag}} .

# Run the image locally (ports 8085, 9095).
docker-run:
    docker run --rm -p 8085:8085 -p 9095:9095 {{docker_tag}}

# --- git ---------------------------------------------------------------------

# Install the local hooks (cargo fmt --check on commit).
#
# NOTE: this sets core.hooksPath to .githooks, which REPLACES the global hook
# path — and the global path is where the ADLC gate lives. A repository that
# runs this gets fmt-check at commit time and no gate at all. That is the state
# this repository is in today (pdt-adlc backlog A1).
hooks:
    git config core.hooksPath .githooks
    @echo "Git hooks installed."
