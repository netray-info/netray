# Plan: advisories

## Phase 1 — Fixable advisories

### Groups

One group: every criterion ends in `Cargo.lock` and `deny.toml`, so the changes share files.

### Plan

- `crates/common/Cargo.toml`: `rust-version = "1.88"`.
- `Cargo.lock`: `cargo update -p time` (MSRV-aware resolver now allows ≥ 0.3.47; time-core and time-macros follow).
- `crates/ifconfig-rs/Cargo.toml`: `lru = "0.18"`; adapt `crates/ifconfig-rs/src/backend/mod.rs` (`new_dns_cache`, the `get`/`put` call sites in `get_ifconfig`) to the 0.18 API if anything changed. Capacity 1024 unchanged.
- `crates/tlsight/Cargo.toml`: drop `rustls-pemfile`; add `rustls-pki-types` (workspace or direct, with the `std` feature) if not already a dependency. `crates/tlsight/src/state.rs` `load_custom_cas`: `CertificateDer::pem_slice_iter(&data)` from `rustls_pki_types::pem::PemObject`, same warn-and-skip per bad item and per empty file.
- `deny.toml`: remove RUSTSEC-2025-0134, -2023-0071, -2026-0009, -2026-0253 with their comments; rewrite the remaining comments as true reachability statements: rustybuzz/ttf-parser (RUSTSEC-2026-0206, -0192) ← usvg/resvg 0.45 and fontdb ← lens's OG renderer only (`crates/lens/src/og/`), fonts bundled, inputs: the domain (lens's input parser) and the OG `label` (printable ASCII 0x20–0x7E, ≤ 32 bytes, `crates/lens/src/og/handler.rs`, pinned by `crates/lens/tests/og_label_bounds.rs`); the badge never shapes text. `paste` (RUSTSEC-2024-0436) ← utoipa-axum ← lens, build-time proc-macro only. hickory (RUSTSEC-2026-0118, -0119): fixed in hickory-proto/hickory-net ≥ 0.26.1, waiting for mhost 0.12.0; -0118 needs a validating `DnssecDnsHandle`, which mhost does not configure and prism's raw DNSSEC queries do not use; -0119 hits encoding, and only our own small queries are encoded.
- Plan gap found in group 1, run 1: `rust-version` 1.88 enables let-chains, so `clippy::collapsible_if` fires on nested `if`/`if let` (first at `crates/common/src/backend.rs:232`). Collapse every one clippy reports into a let-chain, behaviour unchanged; files are whatever clippy names.
- Review fix: the rustybuzz/ttf-parser comment in `deny.toml` no longer claims the OG text is unshaped.
