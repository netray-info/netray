# Spec: v2-v1-remnants

Status: Ready for Implementation
Created: 2026-10-22

## Goal

The last cleanup before `0.30.0` (operator decisions of 2026-10-22). The OG card footer reads
"netray.info" without a label unless the caller sets one. The crate-level doc comments and the
manifests of the module and library crates describe V2, not the V1 services. The dead V1 code
in `netray-common` and `netray-tls` is gone and the TLS inspection file is named for what it
holds. The per-crate V1 record files are gone, what is still true lives at the root, the root
`SECURITY.md` covers the whole repository, and a repository check keeps the files from coming
back.

## Non-goals

- No change to the badge label (`[badges] default_label` stays "netray") or to `?label=` on badges.
- No change to the OG card layout, colours or sizes.
- No change to the finished features' records under `specs/features/` (CLAUDE.md: never edit
  another feature's spec directory on a feature branch).
- No change to the per-crate `README.md`, `CLAUDE.md`, `CHANGELOG.md` and `LICENSE` beyond the
  references this feature moves.
- No deploy, tag or release.

## Context and constraints

- OG footer: `crates/web/src/og/render.rs` `card_svg` writes `<label> · netray.info`, and
  ` · netray.info` for an empty label. `og::layout::DEFAULT_LABEL` is `"netray"`
  (v2-release-cleanup requirement 6), used by `/og/<domain>.png` without `?label=` and by the entry
  card (`pages/og.rs` `entry_card_renderer`). The label is part of the card's ETag. No golden holds
  the footer; tests: `render.rs` unit tests, `crates/web/tests/it/release_label_logo.rs`, and the
  tests passing `DEFAULT_LABEL` to the renderer (`og_plate*.rs`, `entries_og_card.rs`,
  `admission_prefetch.rs`, `history_cache_badge_og.rs`, `crates/netray/tests/agreement.rs`).
- Doc comments: `crates/email/src/lib.rs` (beacon, "fifth pillar", a nonexistent
  `specs/done/sdd/beacon.md`), `crates/common/src/lib.rs` ("service ecosystem", "backend
  services"), `crates/common/src/server.rs` (a "static file handler" it no longer has),
  `crates/common/src/config.rs` and `domain.rs` ("all services", "lens"). Manifests: descriptions
  of `netray-dns`, `-tls`, `-http`, `-ip`, `-common` call the crates services; `netray-common` and
  `netray-ip` name their former repositories; `netray-ip` names `Readme.md` and an `include` list of
  V1 files (`ifconfig.example.toml`, `frontend/dist/**`). The monorepo is
  `https://github.com/netray-info/netray`.
- Dead code: `netray_common::ecosystem` (the V1 `[ecosystem]` URLs) and `netray_common::metrics`
  (`MetricsNamespace`) have no user in the workspace; `serde_json` is a `netray-common` dependency
  only for `ecosystem` outside tests. `netray_tls::RequestId` re-exports
  `netray_common::middleware::RequestId` and is the only use of the `middleware` feature in
  `netray-tls`. `crates/tls/src/routes.rs` holds no router: `resolve_target`, `filter_allowed`,
  the cost plan and `inspect` with their response types (`crates/tls/CLAUDE.md`: "the inspection
  core despite its name"). `crates/netray/src/legacy.rs`'s `ecosystem` row refuses the old config
  table and stays.
- Record files: `crates/dns/{docs/,SECURITY.md}`, `crates/tls/docs/`, `crates/http/specs/`,
  `crates/ip/{docs/,specs/,ROADMAP.md}`. Of these only `crates/ip/docs/enrichment.md` is current
  (the IP module's classification reference, linked from `crates/ip/README.md` and `CLAUDE.md`);
  the rest are V1 reviews, SDDs, roadmaps and screenshots. `crates/dns/SECURITY.md` asks for public
  issues, against the root policy, and scopes prism; its out-of-scope list (upstream mhost, DNS
  provider infrastructure) still holds. `crates/tls/docs/quality.md` is named by the finished
  `specs/features/v2-check-fixes/{spec.md,report.md}`; nothing else in `specs/`, the root docs or
  the crates names a record file by path.

## Requirements

1. Without `?label=`, `/og/<domain>.png` and the entry card (`/r/<id>/og.png`) carry no label:
   the footer reads exactly "netray.info", with no separator before it.
2. With a non-empty `?label=<text>` the footer stays `<text> · netray.info`; an empty label
   renders as no label, "netray.info".
3. CHANGELOG `[Unreleased]` states the footer.
4. The crate-level doc comment (`//!` or `//` before the first item of `src/lib.rs`) of every
   workspace crate, and the module headers of `netray-common`, describe V2: no V1 service name
   (prism, tlsight, spectra, beacon, ifconfig-rs, lens), no "service ecosystem", no "pillar", no
   path that does not exist.
5. Every workspace crate's `description` names no service; a `repository` key, where present, is
   `https://github.com/netray-info/netray`; no manifest names a file the crate does not have
   (`netray-ip`'s `readme` and `include`).
6. No tracked file under `crates/<crate>/docs/`, `crates/<crate>/specs/`, and no
   `crates/<crate>/ROADMAP.md` or `crates/<crate>/SECURITY.md`, for every crate.
7. The IP classification reference lives at `docs/ip-enrichment.md`, describes the V2 IP module
   (no ifconfig-rs, its gauge and its reload as the code has them), and `crates/ip/README.md` and
   `crates/ip/CLAUDE.md` link it there.
8. The root `SECURITY.md` covers the whole repository: what is in scope (`netray serve` on every
   host, API v1, the outbound fetch and target policy, rate limits and client-address
   extraction, the inspector and result pages) and out of scope (upstream mhost, DNS provider and
   resolver infrastructure, the third-party data sources); private reporting stays the only
   channel.
9. `tests/repo/test_crate_docs.sh` enforces 4 to 6 by text: it fails on any tracked file of 6,
   on a crate description naming a service, on a repository other than the monorepo, and on a
   V1 name in a crate-level doc comment.
10. `netray_common::ecosystem` and `netray_common::metrics` are deleted; `serde_json` is a
    dev-dependency of `netray-common` only.
11. `netray_tls::RequestId` is gone, as are the same unused re-exports `netray_dns::RequestId`,
    `netray_http::RequestId` and `netray_email::RequestId`; none of the four module crates enables
    `netray-common`'s `middleware` feature any more.
12. `crates/tls/src/routes.rs` is `crates/tls/src/inspection.rs` (module `netray_tls::inspection`);
    every user, `crates/tls/CLAUDE.md` included, names the new path.
13. `tests/repo/test_v1_remnants.sh` fails while 10 to 12 do not hold.
14. The deleted record files' last content stays reachable: this spec's decision log names the
    commit to read them from, and the one path a finished spec names
    (`crates/tls/docs/quality.md`) is superseded there.

## Phase 1 — OG footer

**Depends on:** none
**Requirements:** 1, 2, 3

`crates/web/src/og/{layout.rs,render.rs}`, `CHANGELOG.md`; tests in `crates/web/src/og/render.rs`
and `crates/web/tests/it/release_label_logo.rs`.

### Test Scenarios

GIVEN the default label WHEN `svg_for_grade` builds the card for example.com THEN the footer text element reads exactly "netray.info" and no "· netray.info" appears (the timestamp line keeps its own "·").
GIVEN the default label WHEN `svg_for_entry` builds an entry card THEN its footer reads exactly "netray.info".
GIVEN the label "custom" WHEN the card is built THEN the footer reads "custom · netray.info".
GIVEN an empty label WHEN the card is built THEN the footer reads exactly "netray.info".
GIVEN the apex router WHEN `/og/example.com.png` is fetched without `?label=` THEN it answers 200 image/png equal to the PNG of the card built with no label.

## Phase 2 — Docs

**Depends on:** none
**Requirements:** 4, 5, 6, 7, 8, 9, 14

`crates/*/src/lib.rs` headers, `crates/common/src/{server.rs,config.rs,domain.rs}` headers,
`crates/*/Cargo.toml` metadata, the record files, `docs/ip-enrichment.md`,
`crates/ip/{README.md,CLAUDE.md}`, `SECURITY.md`, `tests/repo/test_crate_docs.sh`.

### Test Scenarios

GIVEN today's tree WHEN `tests/repo/test_crate_docs.sh` runs THEN it fails naming the record files, the service descriptions, the old repository URLs and the beacon and ecosystem doc comments.
GIVEN the cleaned tree WHEN it runs THEN it passes.
GIVEN a staged `crates/dns/docs/notes.md` WHEN it runs THEN it fails naming that file.
GIVEN the cleaned tree WHEN the root `SECURITY.md` is read THEN it names the private report channel, an in-scope and an out-of-scope list, and mhost as out of scope.
GIVEN the cleaned tree WHEN `crates/ip/README.md` is read THEN its classification link resolves to `docs/ip-enrichment.md`.

## Phase 3 — Dead code

**Depends on:** none
**Requirements:** 10, 11, 12, 13

`crates/common/src/{lib.rs,ecosystem.rs,metrics.rs}`, `crates/common/Cargo.toml`,
`crates/tls/src/{lib.rs,routes.rs,inspection.rs}` and its users, `crates/tls/Cargo.toml`,
`crates/tls/CLAUDE.md`, `tests/repo/test_v1_remnants.sh`.

### Test Scenarios

GIVEN today's tree WHEN `tests/repo/test_v1_remnants.sh` runs THEN it fails naming `ecosystem`, `metrics`, the `serde_json` dependency, the four `RequestId` re-exports and `routes.rs`.
GIVEN the cleaned tree WHEN it runs THEN it passes, and `cargo build --workspace --all-targets` and the gate pass.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22). The four outcomes are the operator's decisions of 2026-10-22;
  the details are the implementer's.
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-22).
- `DEFAULT_LABEL` becomes the empty label over removing the constant: the renderer already treats
  an empty label as "no label", the callers and tests keep one name for the default, and an
  explicit `?label=netray` still renders "netray · netray.info" (implementer under the operator's
  autonomy mandate, 2026-10-22).
- This supersedes v2-release-cleanup's requirement 6 for the OG card and its decision that the
  card footer reads "netray · netray.info": the default card carries no label and reads
  "netray.info"; the badge label stays "netray" (operator, 2026-10-22).
- An explicit empty `?label=` renders "netray.info" too, over keeping " · netray.info": the
  operator's "no label, no leading separator", and the old form was a leading separator with no
  label (implementer under the operator's autonomy mandate, 2026-10-22).
- `docs/ip-enrichment.md` at a new root `docs/` over `specs/`: it is a reference for the code as it
  is, not a design record or a rule (implementer under the operator's autonomy mandate,
  2026-10-22).
- Roadmaps (`crates/dns/docs/roadmap.md`, `crates/tls/docs/future.md`, `crates/ip/ROADMAP.md`) are
  deleted, not moved: they plan V1 service features (batch endpoints, per-service trusted IPs,
  SPA buttons); forward planning lives in the private netray.info roadmap (implementer under the
  operator's autonomy mandate, 2026-10-22).
- The deleted record files are read from `e2bb5899` (`git show e2bb5899:<path>`), the main commit
  this feature branched from. This supersedes the path `crates/tls/docs/quality.md` in
  `specs/features/v2-check-fixes/spec.md` (requirement text) and `report.md` (`docs/quality.md`):
  read it as `e2bb5899:crates/tls/docs/quality.md`. The finished feature's files stay unedited
  (implementer under the operator's autonomy mandate, 2026-10-22).
- `inspection.rs` over `inspect.rs` or `plan.rs`: the file holds the whole TLS inspection (target
  resolution, the cost plan, the run and its response types), and `inspector.rs` beside it holds
  the inspector seam (implementer under the operator's autonomy mandate, 2026-10-22).

- The `RequestId` re-exports of `netray-dns`, `netray-http` and `netray-email` go with
  `netray-tls`'s, with `netray-dns`'s `request_id_middleware` re-export: the same dead lines,
  no user in the workspace, and the `middleware` feature each crate enabled only for them (implementer under the operator's autonomy
  mandate, 2026-10-22).

## Open decisions

None.

## Out of scope

- `crates/*/LICENSE` and the per-crate CHANGELOG history.
- Unused items outside the three named and their orphans; any found are listed in the report.
