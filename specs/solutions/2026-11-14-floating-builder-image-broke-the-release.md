---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-21-worktree-binary-measured-a-timed-out-board.md
---
# A floating builder image and a `stable` toolchain file let an upstream release break the image build

**What failed.** Release run 43612265112 (v0.31.1) built `FROM clux/muslrust:stable` with `rust-toolchain.toml` `channel = "stable"`. After Rust 1.100.0 reached the stable channel on 2026-11-12, rustup updated inside the build and aborted with `could not rename component file ... Invalid cross-device link (os error 18)`. The same tree built green for v0.31.0 on 2026-11-01 (run 41310757555): the date of the build, not the code, decided the outcome.
**What worked.** Pin one version everywhere: `channel = "1.99.1"`, `FROM clux/muslrust:1.99.1-stable-2026-11-03 AS builder`, `ci.yml` `toolchain: 1.99.1`. The image names its toolchain `stable`, so a pinned file alone makes rustup sync a separate `1.99.1` toolchain without the musl target; `ENV RUSTUP_TOOLCHAIN=stable` in the builder keeps the image's own toolchain (`docker run --network none` proves no download). `tests/repo/test_toolchain_pin.sh` refuses drift between the three.
**How to notice next time.** A release build fails in a step no commit touched, and its log shows rustup syncing or installing a toolchain.
