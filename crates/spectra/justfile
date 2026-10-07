# The verb contract of spectra (pdt-adlc ADR 0008).
#
# Migrated from a Makefile on 2026-08-18, by reduction: four of 17 targets are
# gone — help (`just --list` builds the listing), all (frontend + build, which
# `build` already says) and the aggregate ci, whose contents the contract
# calls `check`.
#
# The removal that matters: `check` was `cargo check` — a compile, no test run —
# and the ADLC contract resolver preferred a target of that name, so every
# attestation this repository produced proved only that the code COMPILES
# (pdt-adlc backlog I14, found in seven netray repositories on one afternoon).
#
# adlc-verify is fmt-check + clippy + the Rust suite, offline. The frontend is
# not in it: `npm ci` fetches from GitHub Packages and needs NODE_AUTH_TOKEN,
# which repo-contract requirement 4 rules out of a gate.
#
# What the gate does depend on is frontend/dist — the Rust build embeds it with
# RustEmbed and does not compile without it, and it is gitignored. That is a
# host-owned input, so check-frontend-dist names it before the compiler fails
# on it.

app          := "spectra"
cargo        := "cargo"
npm          := "npm"
frontend_dir := "frontend"

default: adlc-verify

# --- the contract ------------------------------------------------------------

# What the ADLC gate runs: fmt-check, clippy, the Rust suite. No network.
adlc-verify: check-frontend-dist lint test-rust

# Everything: lint, all tests, and the frontend build.
check: lint test frontend

# The whole suite. This repository has no frontend tests.
test: test-rust

# clippy + fmt-check.
lint: clippy fmt-check

# --- preconditions -----------------------------------------------------------

# The Rust build embeds frontend/dist (RustEmbed) and it is gitignored.
check-frontend-dist:
    #!/usr/bin/env bash
    if [ ! -d "{{frontend_dir}}/dist" ]; then
        echo "{{frontend_dir}}/dist is missing — the Rust build embeds it (RustEmbed)." >&2
        echo "  run 'just frontend' once; it needs NODE_AUTH_TOKEN for GitHub Packages." >&2
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

# --- build -------------------------------------------------------------------

# Release binary (builds the frontend first — it is embedded).
build: frontend
    {{cargo}} build --release

# Build and run the release binary.
run: build
    ./target/release/{{app}} {{app}}.dev.toml

# Development server (debug build).
dev:
    {{cargo}} run -- {{app}}.dev.toml

clean:
    rm -rf target/ {{frontend_dir}}/dist {{frontend_dir}}/node_modules

# --- frontend (network: GitHub Packages needs NODE_AUTH_TOKEN) ---------------

frontend-install:
    cd {{frontend_dir}} && {{npm}} ci

# npm ci + vite build.
frontend: frontend-install
    cd {{frontend_dir}} && {{npm}} run build

# Vite dev server with API proxy.
frontend-dev:
    cd {{frontend_dir}} && {{npm}} run dev
