## Phase 1 — Pinned toolchain in image, CI and file

## Groups
G1: C1 · G2: C2, C3 · G3: C4 · G4: C6 · G5: C5, C8, C9, C10, C11, C12, C13

## Plan
### G1
1. rust-toolchain.toml — change — `:2` `channel = "stable"` to `channel = "1.99.1"`; `:3` components stay `["rustfmt", "clippy"]` (the check greps that exact list) — hangs on the `[toolchain]` table, rust-toolchain.toml:1-3. The channel is what every other group is compared against, so it goes first; locally, rustup then installs 1.99.1 (with rustfmt, clippy) on the next cargo call, which needs network once before `just adlc-verify`.

### G2
1. Dockerfile — change — `:7` `FROM clux/muslrust:stable AS builder` to `FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder` (C2) — hangs on the builder stage, Dockerfile:7-14.
2. Dockerfile — change — insert `ENV RUSTUP_TOOLCHAIN=stable` inside the builder stage, after `WORKDIR /build` (`:8`) and before the `RUN cargo build` line (`:12`); the `:9` comment line holds no `cargo`, so any spot between `:7` and `:12` passes the check's "before the first line containing cargo" rule. Add a one-line comment above it saying the image's own `stable` toolchain (1.99.1, musl target) builds and `rust-toolchain.toml` triggers no rustup install; that comment must not contain the word `cargo` (C3) — hangs on the builder stage, Dockerfile:7-14.

### G3
1. .github/workflows/ci.yml — change — `:28` `toolchain: stable` to `toolchain: 1.99.1`; `:29` `components: rustfmt, clippy` and the pinned action SHA `:26` unchanged. It is the only `toolchain:` in ci.yml; release.yml has none and is not touched — hangs on the `dtolnay/rust-toolchain` step, .github/workflows/ci.yml:26-29.

### G4
1. specs/rules/workflow-rules.md — change — R-A2 (`:111`) rewritten on its single line: Node `22`; Rust toolchain pinned to `1.99.1` with `rustfmt` and `clippy` in `rust-toolchain.toml`, the Dockerfile builder tag and `ci.yml`'s `toolchain:`, enforced by `tests/repo/test_toolchain_pin.sh`. The line must name `1.99.1` and `tests/repo/test_toolchain_pin.sh` and must not contain the backticked word `stable` (write `RUSTUP_TOOLCHAIN=stable` only if it is not the backticked `` `stable` `` token, or leave it out) — hangs on the R-A2 bullet, specs/rules/workflow-rules.md:111.
2. crates/common/CLAUDE.md — change — `:7-8` "the toolchain is `stable` (`rust-toolchain.toml`)" to name `1.99.1` pinned in `rust-toolchain.toml` and checked by `tests/repo/test_toolchain_pin.sh`; the phrase "the toolchain is `stable`" must disappear — hangs on the edition/toolchain bullet, crates/common/CLAUDE.md:7-8.

### G5
1. (no file) — none — C5 is the committed check itself (tests/repo/test_toolchain_pin.sh, root override by copying it into a fixture repo, as test_toolchain_pin_selftest.sh does); C9–C13 are its selftest scenarios, already passing; C8 turns green once G1–G4 land. Verify with `bash tests/repo/test_toolchain_pin.sh` (expect `PASS: toolchain pinned to 1.99.1 …`), `bash tests/repo/test_toolchain_pin_selftest.sh`, then `just test-repo` (its `tests/repo/test_*.sh` glob, justfile:28, already runs both) — hangs on nothing new; the check and the `test-repo` recipe, justfile:24-32, exist. C7 is verified separately with `just image` (justfile:84-85) and is not a group.
