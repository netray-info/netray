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

## Phase 2 — mhost 0.12.0

### Groups

One group: the bump moves `Cargo.lock` and every prism file that touches hickory or mhost types at once.

### Plan

- `Cargo.toml` (workspace): `mhost = { version = "=0.12.0", default-features = false }`.
- `crates/mhost-prism/Cargo.toml`: `hickory-proto = { version = "0.26.3", features = [<the 0.26 name of dnssec-ring>] }` — the version mhost 0.12.0 resolves, so one hickory-proto in the tree.
- `crates/mhost-prism/src/api/authcompare.rs:373`: `hickory_proto::rr::RecordType::from(u16::from(rt))`.
- prism's hickory-proto 0.26 port, behaviour unchanged, pinned by `dns_raw` tests: `dns_raw.rs` (`build_query` with `Message::new(id, MessageType::Query, OpCode::Query)`, RD off, EDNS/DO through the 0.26 `edns` field; `RawResponse` accessors over the public `answers`/`authorities`/`additionals` fields and the header through `Metadata`; the test helpers `encode_query_bytes`/`decode_response_bytes` may follow the call shapes but not the test bodies or hex), `dns_dnssec.rs`, `dns_trace.rs`, `api/trace.rs`, `api/check.rs`, `api/authcompare.rs` — wherever the compiler reports.
- Other mhost 0.12 API breaks (`mhost::Name`, `IntoName`, no `From<hickory…>`) in beacon, ifconfig-rs, tlsight and prism — wherever the compiler reports.
- `crates/mhost-prism/src/api/query.rs` `build_resolver_group`: `deny_non_global(true)` on the builder when no server is `ServerSpec::System`; map `mhost::Error::NameServerNotGlobal` from `build()` to `ApiError::BlockedTargetIp`.
- `deny.toml`: remove RUSTSEC-2026-0118/-0119 and their comment.
- Pinned rows: `crates/mhost-prism/tests/lint_results_table.rs` moves to mhost 0.12's output (signed-zone row loses "DNSKEY present but no RRSIG"; TTL duplicate rows as 0.12 emits them) and to 0.12's serde form if it changed; the lens goldens are regenerated only if a row moves. Both are test changes the orchestrator commits with `ADLC-Test-Change` naming requirement 7, not the coder.
- Review fix: `api/compare.rs`'s transport probe returns a `BlockedTargetIp` from `build_resolver_group` instead of dropping it.

## Phase 3 — OG renderer on resvg 0.48

### Groups

One group: `crates/lens` and `Cargo.lock`.

### Plan

- `crates/lens/Cargo.toml`: `resvg = { version = "0.48", default-features = false, features = ["text"] }`, `fontdb = "0.24"`.
- `crates/lens/src/og/render.rs` and font loading in `crates/lens/src/state.rs` (and wherever the compiler names): follow the 0.48 / fontdb 0.24 API; bundled fonts only, no system fonts.
- `deny.toml`: remove RUSTSEC-2026-0206 and RUSTSEC-2026-0192 and their comment block.
