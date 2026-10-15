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
