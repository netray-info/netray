# Intent: release-toolchain-pin

## Problem

Release run 43612265112 for v0.31.1 failed in the Dockerfile builder (`clux/muslrust:stable`,
`rust-toolchain.toml` `channel = "stable"`): rustup updates to Rust 1.100.0 (released 2026-11-09)
and aborts with `could not rename component file ... Invalid cross-device link (os error 18)`.
v0.31.0 built green before 1.100.0. No v0.31.1 image exists in ghcr; the deployment waits.

## Proposed outcome

The toolchain is pinned to the version the builder image ships (1.99.x), in `rust-toolchain.toml`
and in the Dockerfile builder's image tag, so no rustup update runs during the image build.
The `RUSTSEC-2026-0459` ignore in `deny.toml` carries an expiry date, 2027-01-13 (60 days from
2026-11-14), next to its existing reason. A 0.31.2 release can then build.

## Affected users and systems

The release workflow and its Docker image build; local and CI toolchains that read
`rust-toolchain.toml`; `cargo deny` over `deny.toml`; repository checks in `tests/repo` that pin
the channel or the advisory ignore set.

## Constraints

- No rustup update inside the image build.
- Protected tests that move carry an `ADLC-Test-Change` trailer.
- The release itself (`just release 0.31.2`, signed commit and tag, push) is the operator's,
  after this feature merges; it is not part of the work block.

## Open questions

- Exact 1.99 patch version the pinned builder image ships, and which image tag carries it.
- Whether the cargo-deny version in use supports an expiry on an ignore entry, or the date goes
  into the reason text.

Out of scope: sqlx 0.9 bump; review-1/2 test rows of audit-advisories-2026-11.
