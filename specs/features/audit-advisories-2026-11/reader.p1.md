### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result found. What I traced:

- Cargo.lock. I extracted baseline 25b1bd88 into a scratch directory and ran `cargo update --offline -p rustls -p rustls-webpki -p tokio -p webpki-roots -p webpki-root-certs -p rustix` there. The resulting lockfile is byte-identical to the working-tree `Cargo.lock`. Cargo printed only the six "Updating" lines (to 1.1.6, 0.23.46, 0.103.17, 1.54.1, 1.0.12 and 1.0.12) and locked "to highest Rust 1.88 compatible versions". The diff also moves the windows-sys and getrandom edges of errno, quinn-udp, tempfile and winapi-util. Cargo's resolver makes those moves itself, so they are not a hand edit, and no package version outside the six changes. `cargo metadata --locked --offline` accepts the lockfile.
- deny.toml. The ignore list holds exactly RUSTSEC-2024-0436, -2026-0459, -2026-0374 and -2024-0449. Each sits under its own comment block, separated by blank lines. I checked each comment's claim against `cargo tree -i`:
  - libsqlite3-sys arrives via sqlx-sqlite 0.8.6 → sqlx → netray-store and netray-web.
  - basic-toml arrives via askama_derive → askama_macros (proc-macro) → askama → netray-web.
  - unsafe-libyaml arrives via serde_yaml 0.9 → uaparser 0.6.4 → netray-ip, and via serde_yaml_ng → netray-ip.

  The only use of serde_yaml_ng is `serde_yaml::to_string` (crates/ip/src/format.rs:45). The only YAML parse is `UserAgentParser::from_yaml(path)`, called from the startup config path (crates/ip/src/enrichment.rs:138).
- `tests/repo/test_advisories.sh` passes on the working tree.
- `cargo deny --offline check advisories` (cargo-deny 0.20.2, cached advisory-db from 2026-11-13) prints `advisories ok` with no `advisory-not-detected` warning.
- No manifest changed, and `crates/common/Cargo.toml` still declares `rust-version = "1.88"`.
