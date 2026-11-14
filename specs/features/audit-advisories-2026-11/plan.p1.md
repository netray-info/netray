## Phase 1 — Cargo advisories

## Groups
G1: C1, C4, C6, C7, C8, C9, C13, C14 · G2: C2, C3, C10, C11 · G3: C5, C12

## Plan
### G1
1. Cargo.lock — change — first run `cargo update --dry-run -p rustls -p rustls-webpki -p tokio -p webpki-roots -p webpki-root-certs -p rustix` and confirm that only these six packages move (expected 0.23.46, 0.103.17, 1.54.1, 1.0.12, 1.0.12, 1.1.6). Then run the same command without `--dry-run`. Do not change any `Cargo.toml`. `rust-version = "1.88"` stays in place (crates/common/Cargo.toml:11). The current entries are rustix (Cargo.lock:4059), rustls (:4072), rustls-webpki (:4137), tokio (:5035), webpki-root-certs (:5724) and webpki-roots (:5733) — hangs on `check_floors`, tests/repo/test_advisories.sh:126 (C4/C7/C8/C9 fixtures already committed under tests/repo/fixtures/advisories/), and `check_lock`, tests/repo/test_advisories.sh:89, whose existing checks stay unchanged.
2. (no file) — verify — `git diff --stat` shows only `Cargo.lock` (C14). Rebuild and run `tests/repo/test_check_config.sh`, which runs `netray serve --check-config crates/netray/netray.example.toml` (tests/repo/test_check_config.sh:19,48) for C13. C6 was met by baseline 25b1bd88, and the test needs no change — hangs on nothing existing in production.

### G2
1. deny.toml — change — in `[advisories] ignore` (deny.toml:21-26), keep the RUSTSEC-2024-0436 block. Below it, append three entries, each with its own `#` comment block directly above its ID and a blank line between entries (`parse_ignores` cuts a comment at a blank line and lets back-to-back IDs share one comment):
   - RUSTSEC-2026-0459: libsqlite3-sys via sqlx-sqlite (sqlx 0.8, Cargo.toml:75). No sqlx release allows ≥ 0.39, and the WAL disclosure needs local access to `history.db`. Revisit when sqlx lifts its cap. Keyword `sqlx`.
   - RUSTSEC-2026-0374: basic-toml via askama_derive, a build-time proc-macro only. Keyword `askama`.
   - RUSTSEC-2024-0449: unsafe-libyaml via uaparser, which parses only the operator-fetched uap-core `regexes.yaml` at startup, and via serde_yaml_ng (imported as `serde_yaml`), used only to serialize netray's own values. Keywords `uaparser` and `serde_yaml`.
   — hangs on `parse_ignores`/`check_ignores`, tests/repo/test_advisories.sh:25/:41, with `EXPECTED_IGNORES` at :17 (C3; the C10/C11 fixtures are already committed).

### G3
1. (no file) — verify — after G1 and G2, run `cargo deny check advisories` and confirm it exits 0 with no `advisory-not-detected` warning (C5, C12). This is a behavioural check outside the gate (justfile:19-21, R-J6) — hangs on nothing existing; it depends on G1 (Cargo.lock) and G2 (deny.toml).
