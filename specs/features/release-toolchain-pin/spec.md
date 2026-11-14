# Spec: release-toolchain-pin

Status: Done
Created: 2026-11-14
Finished: 2026-11-14

## Goal

One Rust version, 1.99.1, is named in `rust-toolchain.toml`, in the Dockerfile builder's image tag
and in CI's toolchain input, a repository check refuses any drift between the three, and the image
build uses the builder image's own installed toolchain, so `just image` runs no rustup download or
update. The `RUSTSEC-2026-0459` ignore in `deny.toml` carries `Expires: 2027-01-13` in its comment,
and `tests/repo/test_advisories.sh` refuses that comment without an expiry date.

## Non-goals

- The 0.31.2 release itself (`just release 0.31.2`, CHANGELOG, signed tag, push): the operator's, after the merge.
- The sqlx 0.9 bump: a later feature; it does not lift the `RUSTSEC-2026-0459` ignore either (sqlx-sqlite 0.9 caps libsqlite3-sys below 0.38, `deny.toml:28`).
- Automatic bumping of the pin (Renovate, Dependabot): the pin moves by hand.
- A clock comparison on the expiry date in the gate: advisory state never gates (R-J6).
- The review-1/2 test rows of audit-advisories-2026-11.

## Context and constraints

- Release run 43612265112 (v0.31.1) failed in the builder: rustup updated to 1.100.0 (stable channel
  update 2026-11-12, rustc commit date 2026-11-09, per `gh run view 43612265112 --log-failed`) and
  aborted with `Invalid cross-device link (os error 18)`; v0.31.0 built green in release run
  41310757555 on 2026-11-01 (`gh run list --workflow release.yml`).
- `rust-toolchain.toml:2` `channel = "stable"`, `:3` `components = ["rustfmt", "clippy"]`.
- `Dockerfile:7` `FROM clux/muslrust:stable AS builder`; `:10` `COPY . .` brings `rust-toolchain.toml`
  into the builder; `:12` `RUN cargo build --release -p netray`.
- `clux/muslrust:1.99.1-stable-2026-11-03` (amd64, arm64; digest
  `sha256:12e02bbe193e3bd9255f9b5a9312f0039a0c79b9e015cf5e81eeab83a1eb8e0f`) ships one toolchain,
  named `stable-aarch64-unknown-linux-gnu`, rustc 1.99.1, with the `aarch64-unknown-linux-musl`
  target, `RUSTUP_HOME=/opt/rustup`, `CARGO_BUILD_TARGET=aarch64-unknown-linux-musl` (checked with
  `docker run … rustup show`). With a `channel = "1.99.1"` file and no network
  (`docker run --network none`), rustup prints `syncing channel updates for
  '1.99.1-aarch64-unknown-linux-gnu'` and fails; with `-e RUSTUP_TOOLCHAIN=stable` cargo 1.99.1 runs,
  `overridden by environment variable RUSTUP_TOOLCHAIN` (reproduced by the spec reading, 2026-11-14).
- `.github/workflows/ci.yml:26-29` `dtolnay/rust-toolchain`, `toolchain: stable`,
  `components: rustfmt, clippy`. `release.yml:57` builds the image only through the Dockerfile.
- No `tests/repo` check reads the channel, the Dockerfile `FROM` line or the workflow toolchain;
  `test_cargo_workspace.sh:24` only requires the file to exist.
- `deny.toml:27-31`: `"RUSTSEC-2026-0459"` as a plain string with a `#` comment above it.
  cargo-deny 0.20.2 (local; CI installs it unpinned, `ci.yml:41-43`) knows `id` and `reason` on an
  ignore entry, no expiry field (`src/advisories/cfg.rs:13-16`).
- `tests/repo/test_advisories.sh:17` pins the ignore set, `:54-55` requires a `#` comment above each
  id, `:68-70` requires the 0459 comment to mention `sqlx`; the header `:12-13` names it a protected
  test (`ADLC-Test-Change`).
- `specs/rules/workflow-rules.md:111` R-A2: "The Rust toolchain is `stable` with `rustfmt` and
  `clippy`, matching `rust-toolchain.toml`." `crates/common/CLAUDE.md:7-8` says the toolchain is
  `stable`. `adlc.toml:22` P35: a new rule comes with the `tests/repo` check that enforces it.
- Gate: `just adlc-verify` (offline). `tests/repo/test_workflows.sh` checks workflow shape only.

## Requirements

1. `rust-toolchain.toml` names `channel = "1.99.1"`; its components stay `rustfmt`, `clippy`.
2. The Dockerfile builder stage is `FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder`.
3. The builder stage sets `ENV RUSTUP_TOOLCHAIN=stable` before its first `cargo` call, so the
   image's installed toolchain builds and `rust-toolchain.toml` triggers no rustup install or update.
4. `.github/workflows/ci.yml` passes `toolchain: 1.99.1` to `dtolnay/rust-toolchain`; its components stay unchanged.
5. A new check `tests/repo/test_toolchain_pin.sh` fails, naming the file and both values, when the
   `rust-toolchain.toml` channel is not an exact `X.Y.Z`, when the version in the builder's
   `clux/muslrust:` tag differs from it, when `ci.yml`'s `toolchain:` differs from it, or when the
   builder stage lacks `RUSTUP_TOOLCHAIN=stable`; it passes when all agree. Parsing:
   - channel: quotes and a trailing `#` comment stripped, then `^[0-9]+\.[0-9]+\.[0-9]+$`; `stable` or `1.99` fails naming `rust-toolchain.toml`.
   - builder tag: the `FROM` line ending in `AS builder` (case-insensitive); the version is the leading `X.Y.Z` after `clux/muslrust:`; no such line or no version fails.
   - ENV: only lines between the builder `FROM` and the next `FROM`; `ENV RUSTUP_TOOLCHAIN=stable` or `ENV RUSTUP_TOOLCHAIN stable`, quoted or not, before the first line containing `cargo`; an ENV after it or in another stage fails.
   - CI: every `toolchain:` value in `.github/workflows/ci.yml` must equal the channel; none found fails; `release.yml` is not read.
   - It runs offline with grep/sed over tracked files, never docker, and takes a root directory override so its scenarios run against fixture copies.
6. `specs/rules/workflow-rules.md` R-A2 and `crates/common/CLAUDE.md` name the pinned version and
   the check of 5 instead of `stable`.
7. The `# ` comment above `"RUSTSEC-2026-0459"` in `deny.toml` keeps its reason and adds
   `# Expires: 2027-01-13 (revisit the sqlx cap then).` as the last line of the contiguous comment
   run directly above the id, no blank line between.
8. `tests/repo/test_advisories.sh` fails when the `RUSTSEC-2026-0459` comment carries no match of
   `Expires: [0-9]{4}-[0-9]{2}-[0-9]{2}` (no calendar validation, anywhere in the joined comment);
   it compares no date against the clock; the ignore set `EXPECTED_IGNORES` stays unchanged.
   The fixture `tests/repo/fixtures/advisories/deny-all-correct.toml` gains the same `Expires:` line
   (`ADLC-Test-Change`), so its passing case stays green.

## Phases

## Phase 1 — Pinned toolchain in image, CI and file

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6

Pins 1.99.1 in the toolchain file, the builder tag and CI, makes the builder use the image's own
toolchain, adds the drift check and amends R-A2. Demonstrable with `just image` (local docker build)
and `just test-repo`.

### Test Scenarios

GIVEN the pinned tree WHEN `just image` runs THEN the build succeeds, its log shows no rustup
`downloading` or `installing` line, and the builder's `rustc --version` is 1.99.1.
GIVEN the pinned tree WHEN `tests/repo/test_toolchain_pin.sh` runs THEN it exits 0.
GIVEN `rust-toolchain.toml` with `channel = "stable"` WHEN the check runs THEN it exits non-zero and names the file.
GIVEN the builder tag `clux/muslrust:1.99.0-stable-2026-10-14` and channel `1.99.1` WHEN the check runs THEN it exits non-zero naming both versions.
GIVEN `ci.yml` with `toolchain: stable` and channel `1.99.1` WHEN the check runs THEN it exits non-zero naming `ci.yml`.
GIVEN a builder stage without `RUSTUP_TOOLCHAIN=stable` WHEN the check runs THEN it exits non-zero.
GIVEN a dated tag `clux/muslrust:1.99.1-stable-2026-11-03` and the undated `clux/muslrust:1.99.1-stable` with channel `1.99.1` WHEN the check runs THEN both pass (only the version part is compared).

## Phase 2 — Expiry on the sqlx advisory ignore

**Depends on:** none
**Requirements:** 7, 8

Adds the expiry line to the `RUSTSEC-2026-0459` comment and makes `test_advisories.sh` require it.

### Test Scenarios

GIVEN the tree WHEN `cargo deny check advisories` runs (network) THEN `deny.toml` parses and `RUSTSEC-2026-0459` is reported as ignored, not as an error.
GIVEN the tree WHEN `tests/repo/test_advisories.sh` runs THEN it exits 0.
GIVEN a fixture `deny.toml` whose 0459 comment mentions sqlx but has no `Expires:` line WHEN the check runs THEN it exits non-zero naming `RUSTSEC-2026-0459`.
GIVEN a fixture whose 0459 comment carries `Expires: 2020-01-01` (in the past) WHEN the check runs THEN it passes (no clock comparison).
GIVEN the other three ignore comments without `Expires:` WHEN the check runs THEN it passes (only 0459 requires it).

## Decision log

- 1.99.1 over 1.99.0: the newest 1.99 patch the builder image ships; argus already builds with it (agent default, confirmed by operator, 2026-11-14).
- Dated tag `1.99.1-stable-2026-11-03` over `1.99.1-stable` or a digest: the undated tag is rebuilt in place by upstream, a digest is unreadable in review; the check compares only the version (agent default, confirmed by operator, 2026-11-14).
- `ENV RUSTUP_TOOLCHAIN=stable` in the builder over deleting `rust-toolchain.toml` in the build: the image's toolchain is named `stable` and carries the musl target; a `1.99.1` file would install a toolchain without it (agent default, confirmed by operator, 2026-11-14).
- CI `toolchain: 1.99.1` over leaving `stable`: one version everywhere; dtolnay would otherwise install stable and rustup a second toolchain from the file (agent default, confirmed by operator, 2026-11-14).
- A new `test_toolchain_pin.sh` over extending `test_cargo_workspace.sh`: one script per check (`tests/repo`), and P35 requires a check beside the amended R-A2 (agent default, confirmed by operator, 2026-11-14).
- Expiry as an `Expires:` comment line over an `expires` key: cargo-deny 0.20.2 has no expiry field and would reject the key (agent default, confirmed by operator, 2026-11-14).
- No clock comparison on the expiry over failing the gate after 2027-01-13: R-J6 keeps advisory state out of the gate; the date is a review prompt (agent default, confirmed by operator, 2026-11-14).
- Two independent phases over one: the toolchain pin and the advisory expiry touch disjoint files (agent default, confirmed by operator, 2026-11-14).
- Drift-check parsing (channel strip and exact `X.Y.Z`, the `AS builder` FROM line, ENV in the builder stage before the first `cargo` in either ENV form, every `toolchain:` in `ci.yml` only, offline with a root override) over a looser grep: each closes an input the dry run named as ambiguous (agent default, confirmed by operator, 2026-11-14).
- No existing check asserts `stable`, so none moves; R-A2 and `crates/common/CLAUDE.md:7` are amended text, no check greps them (agent default, confirmed by operator, 2026-11-14).
- `Expires:` as the last line of the comment run, matched by a date-shaped regex without calendar validation, and `deny-all-correct.toml` gains the line under `ADLC-Test-Change` over leaving the fixture: otherwise the passing fixture case turns red (agent default, confirmed by operator, 2026-11-14).
- Spec-reading fixes: the sqlx Non-goal no longer claims the bump lifts the ignore; the 1.100.0 date and the green v0.31.0 run cite their source; the rustup install/override claims cite the offline reproduction; Phase 2 gains a `cargo deny check advisories` scenario at the tool that reads `deny.toml` (agent default, confirmed by operator, 2026-11-14).
- The release excluded from the block over running `just release 0.31.2` in it: the cycle never runs a release, the signed commit and tag are the operator's (agent default, confirmed by operator, 2026-11-14).

## Open decisions

None.

## Out of scope

- The 0.31.2 release and its CHANGELOG section: `just release` (`justfile:159-184`), after the merge.
- cargo-deny version pinning in CI (`ci.yml:41-43`) and `adlc-setup` (`justfile:191`).
- The stale `Dockerfile:8` reference in `specs/features/advisories/spec.md:30`: another feature's spec.
