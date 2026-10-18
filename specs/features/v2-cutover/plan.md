# Plan: v2-cutover

## Phase 1 — `netray serve` and `netray.toml`

### Groups

G1: C4, C12–C14, C16–C22 (`legacy.rs`) · G2: C2, C3, C9–C11 (`config.rs`) · G3: C1, C8, C23, C24 (`main.rs`, `lens::run_config`) · G4: C5–C7 (`netray.example.toml`, `netray.dev.toml`). Every test runs through `netray serve`, so G1, G2 and G4 go green only with G3's `mod` lines and subcommand; the four groups are coded by one coder in order G1 → G4.

### Plan

- **G1** `crates/netray/Cargo.toml`: `toml`, `serde`. `crates/netray/src/legacy.rs` (new): `pub fn refuse(raw: &toml::Table) -> Result<(), String>` over one static table of legacy key paths per origin (requirement 11), run on the raw file before deserialization; every message names the key path, its destination or "removed in 0.30.0", and the CHANGELOG 0.30.0 configuration table. Top-level `inspect` is legacy only for spectra's four keys. A bare key owned by exactly one module config names `[modules.<m>].<key>`; shared ones point to the CHANGELOG.
- **G2** `crates/netray/src/config.rs` (new): `Config` with `[server]` (+ `apex`), `[engine]`, `[store]`, `[admission]` (8 keys), `[badges]`/`[og_cards]` (lens's types), `[telemetry]`, `[modules]` (tables with `deadline_ms`); all `deny_unknown_fields`. `load(path, env)`: refuse the eleven legacy prefixes, legacy table on the raw file, filter `NETRAY_LIVE_TESTS`, `load_with_env(path, "NETRAY_", env)`, `validate()` (lens's rules under the new key names). `to_lens()` builds `lens::config::Config` (deadlines → `backends`, `deadline_ms` stripped from module tables, snapshots off). `specs/sdd/v2.md` S10 lists the tables.
- **G3** `crates/lens/src/lib.rs`: `run_config(config, registry)` split out of `run_with`. `crates/netray/src/main.rs`: `mod config; mod legacy;`, `Command::Serve { config, --check-config }`; check = load + `to_lens` + `lens_registry(.., false)`; serve = path from argument or `NETRAY_CONFIG`, load, `to_lens`, telemetry, `lens_registry(.., true)`, SIGHUP reload, `lens::run_config`.
- **G4** `crates/netray/netray.example.toml`: table headers uncommented, every key commented at its default, `[modules.*]` commented out wholly with `deadline_ms`. `crates/netray/netray.dev.toml`: local binds, dev store, no `[modules.ip]`.

CLAUDE.md stays out of the phase commit (shared prose, Phase 8).
