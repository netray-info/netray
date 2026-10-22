# Spec: v2-release-cleanup

Status: Ready for Implementation
Created: 2026-10-22

## Goal

The last work before `0.30.0` (operator decisions of 2026-10-22). Badge and OG fresh runs draw
from their own site-wide budget per minute and, above it, answer the head entry or the unscored
`?` image, never an error. Badges and OG cards carry the label "netray" instead of "lens". The
header logo links to the apex through `pages::ORIGIN` on every host. A unit test proves every
`Now(dest)` row of `crates/netray/src/legacy.rs` names a key `serve --check-config` accepts. The
per-crate READMEs, CLAUDE.md files and CHANGELOGs of `crates/{dns,tls,http,email,ip,common}`
describe V2 modules, not V1 services, and a repository check holds them to it.

## Non-goals

- No change to the visit, re-check and API admission path (per-IP, global, client budget, slots).
- No change to the badge recompute limiter (one recompute per domain per `[badges] ttl_seconds`).
- No change to the per-crate `docs/`, `specs/`, `ROADMAP.md` and `SECURITY.md` files: V1 records,
  left as evidence.
- No badge or OG card redesign beyond the default label; no change to the `?label=` query.
- No deploy, tag or release.

## Context and constraints

- `crates/web/src/admission/mod.rs` `gate`: throttle, slot, then the client budget (visit,
  re-check, API) or the per-domain badge recompute limiter (badge, OG), all under the throttle
  mutex; a refusal spends nothing. CLAUDE.md: governor's `check` spends a token whatever follows,
  so a limiter that must cost nothing on a later refusal cannot be a governor limiter checked
  before another refusal.
- Badge and OG runs use the unspecified client address and are not counted against the per-IP,
  global or client limits (`Trigger::counted`); since S44 the DNS per-target limiter no longer
  bounds them either. Only the 8 slots (`[admission] domain_run_slots`) and the per-domain
  throttle (60 s) and recompute limiter do. `HARD_DEADLINE` is 20 s
  (`crates/engine/src/domain_run.rs`).
- `crates/web/src/pages/badge.rs` and `pages/og.rs` already answer `Admit::Refused` with the head
  entry at the short Cache-Control, or the unscored `?` image (badge grade `error`, OG card `?`);
  SDD §4.4 calls that "render the head entry with the short Cache-Control, or `?`".
- Config: `crates/netray/src/config.rs` (`AdmissionConfig`, `deny_unknown_fields`, `validate`,
  `apex_config`), `crates/web/src/api` `AdmissionLimits`, `crates/netray/netray.example.toml`,
  `tests/repo/test_check_config.sh` (`startup_rejects`), CLAUDE.md "Startup rejects are checked".
- `netray_admission_rejections_total{reason}` registers zero series for throttle, budget, limit,
  slots (`crates/web/src/metrics.rs`, `tests/repo/test_serve_admission_metrics.sh`).
- Labels: `BadgesConfig::default_label` is `"lens"` (`crates/web/src/config.rs`),
  `og::layout::DEFAULT_LABEL` is `"lens"`, used by `/og/<domain>.png` and the entry card
  (`pages/og.rs` `entry_card_renderer`); the OG footer reads `<label> · netray.info`. The canvas
  badge (`netray.info/specs/design/canvas/Logo.dc.html`, `Logo5.dc.html`) shows the mark and the
  word "netray"; neither canvas nor SDD names an OG card label.
- Logo: `crates/web/templates/base.html` has `<a href="/"><img src="/static/logo.svg" …>`; the
  inspector hosts serve the apex's `/inspect/<tool>` bytes (`serve_inspector_links.rs`), so on
  `dns.` the logo leads to `dns.`'s own inspector. `Frame::origin()` returns `pages::ORIGIN`.
- Legacy rows: `ROWS` in `crates/netray/src/legacy.rs` (private to the binary crate);
  destinations are written `[table] key`, `[table].key` or `[table].subtable`, and `<m>` stands
  for any module. `serve::check_config` loads the config (which runs `legacy::refuse`) and builds
  the module registry without data files. Lesson:
  `specs/solutions/2026-10-22-legacy-row-names-a-refused-destination.md`.
- Crate docs: the crates are libraries (`run`-less modules since the cutover, f7934cd1); the
  READMEs still describe prism, tlsight, spectra, beacon and ifconfig-rs as services with SPAs,
  `/api/*` routes, `netray <svc>` subcommands and per-service config files and prefixes, all
  removed in `0.30.0` (root `CHANGELOG.md` `[Unreleased]`). Every claim must trace to code.
- UI changes run `just design-check` with 0 open deviations (CLAUDE.md, design rules).

## Requirements

1. A badge or OG request that would start a fresh run takes one token from a site-wide image-run
   budget, `[admission] image_runs_per_minute`, default 10: a token bucket holding at most that
   many tokens, refilled continuously at that rate per minute. Joining a run in flight, a cache
   hit and a prefetch take none.
2. With the budget empty, the request starts no run and answers as any other badge or OG refusal:
   the head entry with the short Cache-Control, else the unscored `?` image, status 200, never an
   error. The refusal counts `netray_admission_rejections_total{reason="image_budget"}`, which
   registers at zero.
3. The budget is checked after the throttle, the slot and the per-domain recompute limiter would
   admit, and is spent only when the run starts: a refusal by any of them spends no budget token,
   and a budget refusal spends no recompute token and starts no throttle minute. Visit, re-check
   and API runs never touch it.
4. `image_runs_per_minute = 0` is refused at load ("must not be zero"), listed in
   `startup_rejects`; the key is shown at its default in `netray.example.toml`.
5. SDD S44 and §4.4 state the budget, its default and its order; CHANGELOG `[Unreleased]` names
   the key.
6. The default badge label (`[badges] default_label`) and the OG card label (`/og/<domain>.png`
   and the entry card) are "netray"; `netray.example.toml` shows it.
7. The header logo on every page links to `https://netray.info/` through `Frame::origin()`, so
   the inspector hosts' logo leads to the apex; the page stays byte-identical on the apex and the
   inspector hosts.
8. A unit test in `crates/netray/src/legacy.rs` writes, for every `Now(dest)` row (each module for
   `<m>`), a config with the destination at a valid value and asserts `serve::check_config`
   accepts it; a row without a sample value fails the test. Rows it catches are fixed.
9. `crates/{dns,tls,http,email,ip,common}/README.md` and `CLAUDE.md` describe the crate as a V2
   module or library of `netray serve`: its module type, the `[modules.<m>]` (and `[api.<m>]`,
   `[inspect.<m>]` where they exist) keys it reads, its check ids or exports, and how to test it
   from the root `justfile`. They name no V1 service binary, SPA, `frontend/`, per-service config
   file, retired env prefix or V1 `/api/` route as current.
10. Each of those crates' `CHANGELOG.md` keeps its history, and opens with a line saying the crate
    ships inside `netray` since `0.30.0` and its changes are in the root `CHANGELOG.md`.
11. `tests/repo/test_crate_docs.sh` enforces 9 and 10 by text: the pointer line in each
    CHANGELOG, and none of `netray dns|tls|http|email|ip ` subcommands, `frontend/`,
    `SolidJS`, `.toml` per-service names (`prism.toml`, `tlsight.toml`, `spectra.toml`,
    `beacon.toml`, `ifconfig.toml`), retired prefixes (`PRISM_`, `TLSIGHT_`, `SPECTRA_`,
    `BEACON_`, `IFCONFIG_`, `LENS_`) in the READMEs and CLAUDE.md files.

## Phase 1 — Image-run budget

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

`crates/web/src/admission/{mod.rs,state.rs}` (bucket, `Reason::ImageBudget`, gate order),
`crates/web/src/api` `AdmissionLimits`, `crates/web/src/config.rs`, `crates/web/src/metrics.rs`,
`crates/netray/src/config.rs`, `netray.example.toml`, `tests/repo/test_check_config.sh`,
`tests/repo/test_serve_admission_metrics.sh`, SDD, CHANGELOG. Tests in `crates/web/tests/it/`.

### Test Scenarios

GIVEN an apex state with `image_runs_per_minute = 2` and no head entries WHEN badges of three distinct domains are requested THEN two runs start and the third answers 200 with the `?` badge and the short Cache-Control, and the rejection counter `image_budget` is 1.
GIVEN that state with the budget spent and a head entry for the third domain WHEN its badge is requested THEN it answers 200 with the head's grade and the short Cache-Control.
GIVEN that state with the budget spent WHEN an OG card of a domain without a head is requested THEN it answers 200 image/png (the `?` card), never an error status.
GIVEN the budget spent (tokio clock paused) WHEN 30 s pass THEN one more badge run starts.
GIVEN `image_runs_per_minute = 2` and a badge run of example.com in flight WHEN a second badge of example.com arrives THEN it joins the run, and a run of example.net still starts.
GIVEN `image_runs_per_minute = 1` WHEN a prefetch badge of example.com arrives THEN no run starts and a badge of example.net still starts one.
GIVEN `image_runs_per_minute = 1` and a badge of example.com refused by its throttle minute WHEN a badge of example.net arrives THEN it starts a run (the throttle refusal spent no budget).
GIVEN `image_runs_per_minute = 1`, spent WHEN a badge of example.net is refused THEN a later badge of example.net, after a refill, starts a run (the budget refusal spent no recompute token).
GIVEN the budget spent WHEN a visit of a new domain arrives THEN its run starts.
GIVEN a config with `[admission] image_runs_per_minute = 0` WHEN `serve --check-config` runs THEN it exits 1 naming `admission.image_runs_per_minute`.
GIVEN no `image_runs_per_minute` WHEN the config loads THEN the budget is 10; GIVEN `NETRAY_ADMISSION__IMAGE_RUNS_PER_MINUTE=5` THEN it is 5.
GIVEN `netray serve` started WHEN `/metrics` is read THEN `netray_admission_rejections_total{reason="image_budget"} 0` is present.

## Phase 2 — Label and logo

**Depends on:** none
**Requirements:** 6, 7

`crates/web/src/config.rs`, `crates/web/src/og/layout.rs`, `crates/web/templates/base.html`,
`netray.example.toml`; tests in `crates/web/tests/it/` and `crates/netray/tests/`.

### Test Scenarios

GIVEN the default config WHEN `/badge/example.com.svg` renders THEN its label text is "netray" and no text "lens" appears.
GIVEN `?label=custom` WHEN the badge renders THEN its label is "custom" (unchanged).
GIVEN the default config WHEN `/og/example.com.png`'s SVG is built THEN the footer reads "netray · netray.info".
GIVEN any apex page (`/`, `/inspect/dns`) WHEN rendered THEN the logo link is `<a href="https://netray.info/">`, and no `<a href="/"><img` remains.
GIVEN `netray serve` WHEN `dns.<apex>/` is fetched THEN the logo link is `https://netray.info/` and the body equals the apex's `/inspect/dns`.

## Phase 3 — Legacy destinations load

**Depends on:** none
**Requirements:** 8

`crates/netray/src/legacy.rs` (`#[cfg(test)] mod tests`), and any row the test catches.

### Test Scenarios

GIVEN every `Now(dest)` row of `ROWS` WHEN its destination is written at its sample value into a config file THEN `serve::check_config` accepts it.
GIVEN a `Now(dest)` row with no sample value WHEN the test runs THEN it fails naming the row.
GIVEN a destination naming a key the config refuses (a legacy row or no field) WHEN the test runs THEN it fails naming that destination.

## Phase 4 — Crate docs describe V2

**Depends on:** none
**Requirements:** 9, 10, 11

`crates/{dns,tls,http,email,ip,common}/{README.md,CLAUDE.md,CHANGELOG.md}`,
`tests/repo/test_crate_docs.sh`.

### Test Scenarios

GIVEN today's crate docs WHEN `tests/repo/test_crate_docs.sh` runs THEN it fails naming each stale file.
GIVEN the rewritten docs WHEN it runs THEN it passes.
GIVEN a CHANGELOG with its old `## [x.y.z]` entries WHEN rewritten THEN every old entry is still present below the pointer line.
GIVEN a README naming `netray dns crates/dns/prism.toml` WHEN the check runs THEN it fails naming the file.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate (operator,
  2026-10-19, extended 2026-10-22). The five outcomes are the operator's decisions of 2026-10-22;
  the details are the implementer's.
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-22).
- Default 10 image runs per minute over 20 or 60: production's section windows let one run take
  17 s (15 s for the first wave, 2 s for IP), so 10 runs a minute that all hit their windows hold
  under 3 of the 8 slots and leave the rest to visits; with the per-domain recompute limiter (one
  per hour) 10 a minute still cover 600 distinct badged domains an hour. 20 was the first draft
  and could hold 6 of 8 slots in that worst case (implementer under the operator's autonomy
  mandate, 2026-10-22).
- An own token bucket under the gate's mutex over a governor limiter: governor spends on `check`,
  and either order with the per-domain recompute limiter would spend one limiter on the other's
  refusal; the bucket is peeked before the recompute limiter and taken after it (implementer
  under the operator's autonomy mandate, 2026-10-22).
- Key `[admission] image_runs_per_minute` over a key in `[badges]`: it bounds OG cards too, and
  `[admission]` holds the run limits (implementer under the operator's autonomy mandate,
  2026-10-22).
- Zero refused over zero meaning "no image runs": `[badges] enabled` and `[og_cards] enabled`
  already switch the routes off, and every other per-minute run limit refuses zero (implementer
  under the operator's autonomy mandate, 2026-10-22).
- "Not checked" is the existing unscored `?` image (badge grade `error`, OG `?`), which SDD §4.4
  names for every badge and OG refusal; no new image (implementer under the operator's autonomy
  mandate, 2026-10-22).
- Badge label "netray" from the canvas badge (`Logo.dc.html`, `Logo5.dc.html`). The OG card label
  has no name in canvas or SDD, so "netray" as the operator ruled; the footer reads
  "netray · netray.info" (implementer under the operator's autonomy mandate, 2026-10-22).
- Logo absolute on every host over only on inspector hosts: the inspector hosts serve the apex's
  bytes, so a host-dependent link would break that identity; CLAUDE.md sends absolute URLs through
  `pages::ORIGIN` (implementer under the operator's autonomy mandate, 2026-10-22).
- The legacy check is a unit test in the binary crate over an integration test: `ROWS` is private
  to the binary; the test calls `serve::check_config`, the code `--check-config` runs
  (implementer under the operator's autonomy mandate, 2026-10-22).

## Open decisions

None.

## Out of scope

- The per-crate `docs/`, `specs/`, `ROADMAP.md`, `SECURITY.md` (V1 records).
- The operator's production config (pdt-public): it sets no `[badges]` table, so the defaults
  apply.
