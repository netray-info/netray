# Report: v2-release-cleanup

## Phase 1 — Image-run budget

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: badge and OG fresh runs take a token of `[admission] image_runs_per_minute` (default 10), a bucket refilled continuously; join, hit and prefetch take none | green | crates/web/tests/it/admission_image_budget.rs |
| C2 | R2: an empty budget answers the head entry or the unscored image, status 200; counted as `image_budget`, registered at zero | green | crates/web/tests/it/admission_image_budget.rs, tests/repo/test_serve_admission_metrics.sh |
| C3 | R3: checked after throttle, slot and recompute limiter, spent only on start; visits never touch it | green | crates/web/tests/it/admission_image_budget.rs |
| C4 | R4: zero refused at load, listed in startup_rejects; example shows the default | green | crates/netray/tests/serve_image_budget.rs, tests/repo/test_check_config.sh |
| C5 | R5: SDD S44 and section 4.4 and the CHANGELOG name the budget | green | specs/sdd/v2.md (this phase), CHANGELOG.md (prose commit at the end) |
| C6 | three distinct badge domains at budget 2: the third is refused, unscored badge, short Cache-Control, counter 1 | green | crates/web/tests/it/admission_image_budget.rs |
| C7 | budget spent, stale head: badge renders the head grade, short Cache-Control | green | crates/web/tests/it/admission_image_budget.rs |
| C8 | budget spent: OG card without head answers 200 image/png | green | crates/web/tests/it/admission_image_budget.rs |
| C9 | after 30 s one more run starts | green | crates/web/tests/it/admission_image_budget.rs |
| C10 | a join takes no token | green | crates/web/tests/it/admission_image_budget.rs |
| C11 | a prefetch takes no token | green | crates/web/tests/it/admission_image_budget.rs |
| C12 | a throttle refusal spends no token | green | crates/web/tests/it/admission_image_budget.rs |
| C13 | a budget refusal spends no recompute token | green | crates/web/tests/it/admission_image_budget.rs |
| C14 | a visit starts with the budget spent | green | crates/web/tests/it/admission_image_budget.rs |
| C15 | zero exits 1 naming admission.image_runs_per_minute | green | crates/netray/tests/serve_image_budget.rs |
| C16 | default 10; env 5 accepted | green | crates/netray/tests/serve_image_budget.rs |
| C17 | the zero series image_budget renders at startup | green | tests/repo/test_serve_admission_metrics.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 admission, config, metrics | 1 | implementer (no coder agent: one group, written in place) | n/a | n/a |

Tests and code were written by the implementer directly, under the operator's autonomy mandate
(2026-10-22); the independent reading of the whole range runs in the review step.

### Behavioural verification

`netray serve` with `[admission] image_runs_per_minute = 1`, no modules, then
`/badge/example.com.svg`, `/badge/example.net.svg`, `/og/example.org.png` on `Host: netray.info`:

```
example.com 200 image/svg+xml; charset=utf-8
example.net 200 image/svg+xml; charset=utf-8
og 200 image/png
netray_runs_total{outcome="incomplete",trigger="badge"} 1
netray_admission_rejections_total{reason="image_budget"} 2
```

## Phase 2 — Label and logo

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: default badge label and OG label are netray; the example shows it | green | crates/web/tests/it/release_label_logo.rs |
| C2 | R7: the header logo links to https://netray.info/ through Frame::origin on every host, byte-identical on apex and inspector hosts | green | crates/web/tests/it/release_label_logo.rs |
| C3 | default badge renders netray, no lens | green | crates/web/tests/it/release_label_logo.rs |
| C4 | ?label=custom renders custom | green | crates/web/tests/it/release_label_logo.rs |
| C5 | OG footer reads netray · netray.info | green | crates/web/tests/it/release_label_logo.rs |
| C6 | apex pages carry the absolute logo link, no host-relative one | green | crates/web/tests/it/release_label_logo.rs |
| C7 | inspector hosts carry it and equal the apex's /inspect/tool | green | crates/web/tests/it/release_label_logo.rs |

Three earlier tests encoded "no absolute URL in the header" and "logo inside a link to /"
(`fidelity_header.rs`, `pages_frame.rs`, `site_nav_links.rs`); requirement 7 supersedes them for
the logo only, so each now exempts exactly the logo's apex link and keeps its rule for every other
link. The apex `/` is absent from the logo test's path list: the test's router (no store)
answers it 404, and `pages_frame.rs` covers the landing page.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 label, logo | 1 | implementer (no coder agent) | n/a | n/a |

### Behavioural verification

`just design-check` with this build copied to main's binary path (backed up, restored
byte-identical, `NETRAY_BIN` set): `0 open deviation(s), 0 stale accepted entries`.

## Phase 3 — Legacy destinations load

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8: every Now row (each module for <m>) loads through serve::check_config at a sample value; a row without a sample fails; caught rows fixed | already_implemented | crates/netray/src/legacy.rs |
| C2 | every Now destination is accepted | already_implemented | crates/netray/src/legacy.rs |
| C3 | a Now row with no sample value fails naming the row | already_implemented | crates/netray/src/legacy.rs |
| C4 | a destination the config refuses fails naming it | already_implemented | crates/netray/src/legacy.rs |

The phase was already in place: every `Now(dest)` row of `ROWS` names a key that loads (the two
stale rows had been fixed on the cutover branch), so the new tests pass and stay as the guard.
`a_destination_the_config_refuses_is_caught` proves the check refuses the two historic stale
destinations; `[modules.ip]` rows are written with both GeoIP databases, which the table requires.

### Behavioural verification

skipped: the check is the unit test itself (`cargo test -p netray --bin netray legacy`, 4 passed).

## Phase 4 — Crate docs describe V2

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R9: README and CLAUDE.md of dns, tls, http, email, ip, common describe the V2 module or library, its keys, checks or exports and root-verb testing, and name no V1 service as current | green | tests/repo/test_crate_docs.sh |
| C2 | R10: each CHANGELOG keeps its history and opens with the pointer to the root CHANGELOG | green | tests/repo/test_crate_docs.sh |
| C3 | R11: the repo check enforces the pointer and the stale-string list | green | tests/repo/test_crate_docs.sh |
| C4 | today's docs fail the check naming each file | green | tests/repo/test_crate_docs.sh |
| C5 | the rewritten docs pass | green | tests/repo/test_crate_docs.sh |
| C6 | every old CHANGELOG entry stays below the pointer | green | tests/repo/test_crate_docs.sh (git diff: each CHANGELOG +2 lines, 0 removed) |
| C7 | a README naming a V1 subcommand and config file fails | green | tests/repo/test_crate_docs.sh |

One writer per crate traced each claim to the code; corrections found on the way (the old docs
said otherwise): DNS check cost is 19 tokens (`CHECK_TOTAL_STEPS`), authcompare charges types x 2
+ 16, the DNS crate is MIT only; the HTTP module reads no body (`read_body: false`) and a refused
or reset HTTPS probe is a measured fail; email's `ModuleConfig` has no `backends` key.

Leftovers noticed outside the three files (mentioned, not changed): crate doc comments in
`crates/email/src/lib.rs` ("fifth pillar") and `crates/common/src/lib.rs` ("service ecosystem"),
`crates/common/src/server.rs` (a static file handler that no longer exists), `crates/common`
`Cargo.toml` description and repository URL; unused `netray_common::ecosystem` and
`netray_common::metrics`; unused `netray_tls::RequestId` re-export; `crates/tls/src/routes.rs`
holds no router; the per-crate `docs/`, `specs/` and `ROADMAP.md` are V1 records.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 dns docs | 1 | writer agent | 116908 | 163 |
| G2 tls docs | 1 | writer agent | 115630 | 187 |
| G3 http docs | 1 | writer agent | 96909 | 108 |
| G4 email docs | 1 | writer agent | 92639 | 104 |
| G5 ip docs | 1 | writer agent | 123374 | 203 |
| G6 common docs | 1 | writer agent | 98649 | 134 |

### Behavioural verification

skipped: documentation; `bash tests/repo/test_crate_docs.sh` passes.
