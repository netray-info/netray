# Spec: SDD into the monorepo

Status: Draft
Created: 2026-10-14

## Goal

`specs/sdd/v2.md` holds the V2 SDD as cleaned in the planning repository (e28b9fc), `specs/sdd/v2-tokens.json` holds the token source, and `CLAUDE.md` points at both as this repository's own documents; `tests/repo/test_sdd_in_repo.sh` checks all three.

## Non-goals

- No history of the SDD; its file is copied, not merged.
- No `crates/web/` and no `build.rs`; V4.1 creates them and moves the tokens.
- PRD, reviews and baseline stay private.

## Context and constraints

- The source is `netray.info/specs/sdd/v2.md` at e28b9fc (1124 lines) and `netray.info/specs/prd/v2-tokens.json`; netray-v2 read the tokens file: design tokens only.
- §8.1 (`v2.md:863`) puts `tokens.json` in `netray-web`, a crate that does not exist; the workspace takes every `crates/*` as a member (`Cargo.toml:2`), so a `crates/web/` without a `Cargo.toml` breaks the build.
- `CLAUDE.md:17` cites "planning SDD `v2.md` §3".
- Domains in docs: only `example.com` and `netray.info` (operator's global rule). `v2.md:50` cites `data-star.dev/reference/security`, `v2.md:633` cites `html.spec.whatwg.org`.
- Repository checks are `tests/repo/test_*.sh`, run by `just adlc-verify`.

## Requirements

1. `specs/sdd/v2.md` is the planning repository's `specs/sdd/v2.md` at e28b9fc, byte for byte except: the §8.1 `tokens.json` row says the source sits at `specs/sdd/v2-tokens.json` until V4.1 moves it into `netray-web`; and the citation domains as decided (open decision 1).
2. `specs/sdd/v2-tokens.json` is the planning repository's `specs/prd/v2-tokens.json`, byte for byte.
3. `CLAUDE.md` names `specs/sdd/v2.md` as this repository's SDD, maintained in the same commit as the code that amends it, and cites PRD, reviews and baseline as "(private)"; no "planning SDD" remains.
4. `tests/repo/test_sdd_in_repo.sh` fails when `specs/sdd/v2.md` is missing, when `specs/sdd/v2-tokens.json` is missing or not valid JSON, or when `CLAUDE.md` does not name `specs/sdd/v2.md` or still says "planning SDD".

## Phase 1 — SDD and tokens in the repository

**Depends on:** none
**Requirements:** 1, 2, 3, 4

### Test Scenarios

- GIVEN the tree after this phase WHEN `tests/repo/test_sdd_in_repo.sh` runs THEN it exits 0.
- GIVEN `CLAUDE.md` still saying "planning SDD" WHEN the test runs THEN it exits non-zero.
- GIVEN `specs/sdd/v2-tokens.json` with a trailing comma WHEN the test runs THEN it exits non-zero.

## Decision log

- Tokens at `specs/sdd/v2-tokens.json` over a stub `crates/web/` crate or deferring them to V4.1: no build risk in a docs feature; §8.1 says where they sit until V4.1 (operator, 2026-10-14).
- Copy verbatim, no history (netray-v2 relaying the repo-layout plan, operator-approved 2026-10-14).

## Open decisions

1. The citation domains at `v2.md:50` and `v2.md:633`: keep as citations, or cite by name without URL. Asked of netray-v2 (2026-10-14).

## Out of scope

- Replacing the planning copy with a pointer: netray-v2, in the planning repository.
