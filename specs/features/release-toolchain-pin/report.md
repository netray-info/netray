# Report: release-toolchain-pin

## Phase 1 — Pinned toolchain in image, CI and file

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 rust-toolchain.toml channel = "1.99.1", components rustfmt, clippy | green | tests/repo/test_toolchain_pin.sh |
| C2 | R2 builder FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder | green | tests/repo/test_toolchain_pin.sh |
| C3 | R3 builder sets ENV RUSTUP_TOOLCHAIN=stable before its first cargo call | green | tests/repo/test_toolchain_pin.sh |
| C4 | R4 ci.yml toolchain: 1.99.1, components unchanged | green | tests/repo/test_toolchain_pin.sh |
| C5 | R5 drift check test_toolchain_pin.sh with the parsing rules, offline, root override | green | tests/repo/test_toolchain_pin.sh |
| C6 | R6 workflow-rules.md R-A2 and crates/common/CLAUDE.md name the pin and the check | green | tests/repo/test_toolchain_pin.sh |
| C7 | S1 just image builds with no rustup download, builder rustc 1.99.1 | test-unwritable | behavioural verification (docker and network, not offline) |
| C8 | S2 pinned tree: check exits 0 | green | tests/repo/test_toolchain_pin.sh |
| C9 | S3 channel stable: check fails naming rust-toolchain.toml | already_implemented | tests/repo/test_toolchain_pin_selftest.sh |
| C10 | S4 builder tag 1.99.0 vs channel 1.99.1: fails naming both versions | already_implemented | tests/repo/test_toolchain_pin_selftest.sh |
| C11 | S5 ci.yml toolchain: stable: fails naming ci.yml | already_implemented | tests/repo/test_toolchain_pin_selftest.sh |
| C12 | S6 builder stage without RUSTUP_TOOLCHAIN=stable: fails | already_implemented | tests/repo/test_toolchain_pin_selftest.sh |
| C13 | S7 dated and undated 1.99.1 tags both pass | already_implemented | tests/repo/test_toolchain_pin_selftest.sh |

C9 to C13 are self-test cases of the drift check; the check is itself a test file written in the same batch, so they passed at RED and stay as its protection.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 to G4 (one coder, one test) | 1 | sonnet | 20249 | 20 |
| G5 | 0 | already in place | 0 | 0 |

### Reader

specs/features/release-toolchain-pin/reader.p1.md: blockers=0 amendments=0 majors=0 deferred=0 nits=0.

### Behavioural verification

`docker build --progress=plain --no-cache -t netray:pin-check .` (the `just image` command): exit 0; `grep -ciE 'rustup|syncing channel|downloading component|installing component'` over the build log: 0.
`docker run --rm --entrypoint netray netray:pin-check --version`: `netray 0.31.1`.
In `clux/muslrust:1.99.1-stable-2026-11-03` with a `channel = "1.99.1"` toolchain file, `RUSTUP_TOOLCHAIN=stable rustc --version`: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
