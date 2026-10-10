# Plan: lens check-config without data

## Phase 1 — Config-only check

## Groups

G1: C1–C6 (one function)

## Plan

### G1
- `crates/netray/src/main.rs` `lens_registry(cfg, startup)`: shared parsing and rules; data files only on startup.
- `CLAUDE.md`: the data-file exception.
