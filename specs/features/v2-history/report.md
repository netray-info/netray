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

## Phase 4 — lens records every run

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R2 Run built from lens's scored result; every catalogue check per section (N/A → not_applicable, incomplete → unmeasured) | green | crates/lens/tests/history_record.rs |
| C2 | R12 `[history] enabled, db_path`; startup opens/migrates or exits 1 naming the store; `--check-config` does not open it; hourly sweep; gate scripts set `LENS_HISTORY__DB_PATH` | green | tests/repo/test_check_config.sh, tests/repo/test_history_db_path.sh |
| C3 | R13 every run without options recorded/diffed once, any route; dkim_selectors runs skipped; wire, goldens, cache, OG, snapshots unchanged | green | crates/lens/tests/history_record.rs |
| C4 | R14 badge renders `complete == false` as unscored with short Cache-Control | green | crates/lens/tests/history_badge.rs |
| C5 | R15 failed record: error log, `lens_history_record_errors_total` +1, response unaffected; counter registered at startup | green | crates/lens/tests/history_record.rs |
| C6 | R16 SDD amendments | test-unwritable | specs/sdd/v2.md (§3.1, §3.7, §4.1, §4.3, §4.4, §10 Phase 2) |
| C7 | R17 http and dns volatile lists | green | crates/http/tests/history_volatile.rs, crates/dns/tests/history_volatile.rs |
| C8 | S: /api/check complete run → one entry; response equals golden | green | crates/lens/tests/history_record.rs |
| C9 | S: badge cache miss complete run → entry recorded | green | crates/lens/tests/history_record.rs |
| C10 | S: dkim_selectors run → no entry | green | crates/lens/tests/history_record.rs |
| C11 | S: incomplete run → no entry | green | crates/lens/tests/history_record.rs |
| C12 | S: IP NoDnsResults → not stored, IP checks unmeasured | green | crates/lens/tests/history_record.rs |
| C13 | S: incomplete hard-fail F → badge unscored, short Cache-Control | green | crates/lens/tests/history_badge.rs |
| C14 | S: complete F → badge F, long Cache-Control | already_implemented | crates/lens/tests/history_badge.rs |
| C15 | S: `enabled = false` → no history.db | green | crates/lens/tests/history_config.rs |
| C16 | S: db_path in nonexistent dir → startup exits non-zero naming the store; `--check-config` exits 0 | green | tests/repo/test_check_config.sh |
| C17 | S: failing store → response unchanged, counter 1 | green | crates/lens/tests/history_record.rs |
| C18 | S: unknown key under `[history]` → rejected | green | crates/lens/tests/history_config.rs |
| C19 | S: gate leaves no `history.db*` in the tree | green | tests/repo/test_history_db_path.sh |
| C20 | S: http runs differing in Expires, cf-cache-status, A order → equal fingerprints | green | crates/http/tests/history_volatile.rs |
| C21 | S: dns runs differing in RRSIG data → equal fingerprints | green | crates/dns/tests/history_volatile.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 volatile lists | 1 | sonnet | 36024 | 48 |
| G2 lens | 1 (stopped: Config literals in protected tests; fixed by the implementer under `ADLC-Test-Change`) | opus | 100366 | 255 |
| G3 gate rows | implementer | — | — | — |

Follow-ups the implementer made beyond the coders: `history: HistoryConfig::default()` in nine test `Config` literals and in `config.rs`'s test literal; C1/C8 relaxed (a complete run may hold not-applicable sections); the dns and http registered-volatile tests extended for R17.

### Reader

COUNTS blocker=0 amendment=2 deferred=1 nit=1

| class | finding | outcome |
|---|---|---|
| AMENDMENT | CDN headers changing per response (`report-to`, `x-fastly-request-id`, `via`, `x-vercel-id`) are not volatile; every rerun behind such a CDN stores an entry | repaired in phase: seven headers added (R17 extended), SDD §3.7 |
| AMENDMENT | rotating address sets: `email.mx_addresses` (and its per-address FCrDNS/DNSBL rows) is keyed by MX addresses that load-balanced providers rotate per query (verified live on a Microsoft 365 domain); the path grammar cannot drop rows whose key rotates | **open: operator decision** (handoff) |
| DEFERRED | recording runs on the request path after the 20 s deadline, behind the store's write mutex and sqlx's 5 s busy timeout; a locked store delays responses | for `v2-admission`, which moves runs off the request path (§4.4) |
| NIT | the incomplete-F badge was tested with the cache off only | test added for the cache-on path |

### Behavioural verification

```
$ netray lens <lens.dev.toml with temp db paths>
== health: 200
== metrics: lens_history_record_errors_total 0
== files: history.db history.db-shm history.db-wal snapshots.db ...
$ netray lens <same, [history] db_path = "/nonexistent-dir/history.db">
exit 1 — ERROR lens: failed to open history store ... db_path=/nonexistent-dir/history.db
$ netray lens --check-config <same>
config ok: ... (exit 0)
```

A live check against a real domain was not run: a worktree build has no outbound network (memory `reference_new_binary_no_network`).
