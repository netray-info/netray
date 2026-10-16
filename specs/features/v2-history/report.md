# Report: v2-history

## Phase 1 — Run, fingerprint and diff in netray-model

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 serialisable `Run`; `Grade: Deserialize`; round-trip | green | crates/model/tests/run.rs |
| C2 | R3 fingerprint: SHA-256 over canonical JSON, volatile removed by block name, canonical sets, CSP nonces normalised | green | crates/model/tests/fingerprint.rs |
| C3 | R4 status transitions incl. not tested / not measured, current status decides, never fixed/worse | green | crates/model/tests/diff_status.rs |
| C4 | R5 added/removed, by release when engine versions differ, engine change marked | green | crates/model/tests/diff_presence.rs |
| C5 | R6 evidence diff over six shapes, only sections measured in both, volatile/nonce/order never reported | green | crates/model/tests/diff_evidence.rs |
| C6 | S: Run with six shapes round-trips | green | crates/model/tests/run.rs |
| C7 | S: date header + A record order differ → equal fingerprints | green | crates/model/tests/fingerprint.rs |
| C8 | S: changed HSTS header → fingerprints differ | green | crates/model/tests/fingerprint.rs |
| C9 | S: tls.handshake@addr handshake_ms differs → equal under `tls.handshake/handshake_ms` | green | crates/model/tests/fingerprint.rs |
| C10 | S: repeated fields keys reordered → equal fingerprints, no evidence row | green | crates/model/tests/fingerprint.rs |
| C11 | S: one repeated fields key value changed → fingerprints differ | green | crates/model/tests/fingerprint.rs |
| C12 | S: CSP nonce differs → equal fingerprints, no evidence row | green | crates/model/tests/fingerprint.rs |
| C13 | S: CSP directive differs → fingerprints differ | green | crates/model/tests/fingerprint.rs |
| C14 | S: status table 6×6 → one class per unequal pair, none for equal, no fixed/worse for nt/um | green | crates/model/tests/diff_status.rs |
| C15 | S: check only in current → added; other engine version → added by release, marked | green | crates/model/tests/diff_presence.rs |
| C16 | S: tls N/A section → measured across engine versions → now applicable, not added by release | green | crates/model/tests/diff_presence.rs |
| C17 | S: incomplete email section → into not measured, no removed blocks | green | crates/model/tests/diff_presence.rs |
| C18 | S: table one changed row + one reordered → only changed row reported | green | crates/model/tests/diff_evidence.rs |
| C19 | S: volatile value changed → no evidence change | green | crates/model/tests/diff_evidence.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 + 1 (reader fixes) | sonnet | 166315 | 208 |

### Reader

COUNTS blocker=1 amendment=5 deferred=1 nit=0

| class | finding | outcome |
|---|---|---|
| BLOCKER | `canonical.rs`: an unterminated `'nonce-` swallowed later sources; different CSPs fingerprinted equal | fixed in phase (test `unterminated_nonce_does_not_hide_other_sources`) |
| AMENDMENT | a volatile row path kept the row's presence compared (`Age` on CDN hits) | repaired in phase: a row path covers the whole row (test `volatile_row_path_covers_the_whole_row`); spec R3 |
| AMENDMENT | table/fields items lost added/removed/changed once the predecessor is swept | repaired in phase: signed items `~ - +`; spec R6, `ADLC-Test-Change` |
| AMENDMENT | http volatile list misses `expires`, `x-timer` and other per-response values | forward: spec R17, Phase 4 |
| AMENDMENT | `http.server/address` follows resolver order | forward: spec R17, Phase 4 |
| AMENDMENT | `dns.rrsig` data changes on every re-signature | forward: spec R17, Phase 4 |
| DEFERRED | `diff` takes `rescored_prev` as given; `None` across profile versions gives one step | Phase 2 (R7) |

The forward amendments were decided by the implementer under the operator's autonomy mandate (spec decision log), so no `amendment-forward` halt.

### Behavioural verification

skipped: Phase 1 is a library with no entry point (no route, no CLI); lens calls it in Phase 4.

## Phase 2 — Profile-effect grade in netray-engine

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7 grade change; two steps when profile versions differ; re-scored grade computed by `netray-engine` from a stored `Run` and any loadable profile | green | crates/engine/tests/history.rs |
| C2 | S: predecessor under A, current under in-memory B reweighting one check → two steps | green | crates/engine/tests/history.rs |
| C3 | S: same profile → one step | green | crates/engine/tests/history.rs |
| C4 | S: stored `Run` re-scored under its own profile → stored grade | green | crates/engine/tests/history.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 31578 | 81 |

### Reader

COUNTS blocker=0 amendment=0 deferred=0 nit=1

| class | finding | outcome |
|---|---|---|
| NIT | `history::diff` does not check `cur_profile.version == cur.profile_version` | listed; the Phase 4 caller passes the profile it scored with |

The Phase 1 DEFERRED (`rescored_prev` `None` across profile versions) is closed: `history::diff` derives it from the versions.

### Behavioural verification

skipped: library function without an entry point; lens calls it in Phase 4.

## Phase 3 — netray-store

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R8 schema of §4.1 via portable sqlx migrations; 8-char nanoid id with collision retry | green | crates/store/tests/schema.rs |
| C2 | R9 record in one transaction: identical → last_seen only; else new entry with predecessor, run JSON, fingerprint, diff; returns outcome; writes serialised | green | crates/store/tests/record.rs |
| C3 | R10 incomplete never stored, whatever grade; diff returned; nothing written | green | crates/store/tests/record.rs |
| C4 | R11 sweep: whole domain past 90 days; else entries past 90 days except latest and its predecessor; survivor keeps diff, predecessor NULL | green | crates/store/tests/sweep.rs |
| C5 | S: fresh DB migrated → schema exists; migrations contain no SQLite-only type or function | green | crates/store/tests/schema.rs |
| C6 | S: first record → one entry, predecessor NULL, no diff, outcome first | green | crates/store/tests/record.rs |
| C7 | S: identical later → count unchanged, last_seen moved | green | crates/store/tests/record.rs |
| C8 | S: one status changed → second entry, predecessor = first, stored diff | green | crates/store/tests/record.rs |
| C9 | S: two different runs concurrently → both succeed, chained | green | crates/store/tests/record.rs |
| C10 | S: two identical runs concurrently → both succeed, one entry | green | crates/store/tests/record.rs |
| C11 | S: incomplete grade F → nothing written, last_seen unchanged, diff returned | green | crates/store/tests/record.rs |
| C12 | S: incomplete identical in fingerprint → last_seen unchanged | green | crates/store/tests/record.rs |
| C13 | S: domain latest 91 days → domain gone | green | crates/store/tests/sweep.rs |
| C14 | S: e1→e2→e3, e1/e2 91 days, e3 1 day → e1 deleted, e2/e3 remain | green | crates/store/tests/sweep.rs |
| C15 | S: e1→e2→e3→e4, e2 91 days → e2 deleted, e3 keeps diff, predecessor NULL | green | crates/store/tests/sweep.rs |
| C16 | S: 89 days → remains | green | crates/store/tests/sweep.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 + 1 (reader fixes) | sonnet | 105342 | 197 |

### Reader

First reader run lost to an expired OAuth token (API 401); rerun.

COUNTS blocker=0 amendment=1 deferred=2 nit=2

| class | finding | outcome |
|---|---|---|
| AMENDMENT | `latest` followed the caller's `created_at`; a backwards `now` forked the chain and moved `last_seen_at` back | repaired in phase: latest = chain head, times never move backwards (tests `a_clock_running_backwards_never_forks_the_chain`, `latest_is_the_chain_head_at_equal_created_at`); SDD §4.1 amended |
| DEFERRED | no index on `predecessor_id`; the head lookup scans the table | fixed in phase: index in `0001` (not shipped) |
| DEFERRED | sweep parsed every head's full `Run`; one bad row stalls retention | fixed in phase: sweep reads id, `last_seen_at`, `predecessor_id` only |
| NIT | `entries()` order among three or more equal `created_at` | listed |
| NIT | no test for the equal-`created_at` tie-break | test added (passes: already in place) |

### Behavioural verification

skipped: library crate without an entry point; lens opens it in Phase 4.
