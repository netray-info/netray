# Report: v2-fixture-consistency

## Phase 1 — HTTP, TLS and IP from evidence

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 HTTP from_evidence rebuilds the inspection from the http blocks and translates it | green | crates/lens/tests/it/fixture_evidence_http.rs |
| C2 | R2 TLS from_evidence rebuilds the inspect response and runs the module's assessment at the run's measurement time | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C3 | R5 IP from_evidence translates one lookup per ip.addresses row, total from dns.a and dns.aaaa | green | crates/lens/tests/it/fixture_evidence_ip.rs |
| C4 | R6 from_evidence over its own output returns the same outcome | green | crates/lens/tests/it/fixture_evidence_http.rs, crates/lens/tests/it/fixture_evidence_tls.rs, crates/lens/tests/it/fixture_evidence_ip.rs |
| C5 | http.headers without x-frame-options: x_frame_options not pass; with DENY it passes | green | crates/lens/tests/it/fixture_evidence_http.rs |
| C6 | port-80 probe 200 without redirect: https_redirect fails; a hop to https passes | green | crates/lens/tests/it/fixture_evidence_http.rs |
| C7 | strict-transport-security max-age=31536000: hsts passes and an http.hsts block holds max_age | green | crates/lens/tests/it/fixture_evidence_http.rs |
| C8 | HTTP from_evidence is a fixpoint | green | crates/lens/tests/it/fixture_evidence_http.rs |
| C9 | TLS leaf days_remaining 10: expiry_window warns; 68 passes | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C10 | two TLS leaves with different fingerprints: consistency not pass | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C11 | chain in issuer order to a self-signed root: chain_trusted and chain_complete pass; SANs not covering the domain: hostname_match fails | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C12 | not_after before today with days_remaining saying valid: the chain's expired key reads false | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C13 | TLS from_evidence is a fixpoint | green | crates/lens/tests/it/fixture_evidence_tls.rs |
| C14 | ip.addresses row with tor true: ip.reputation fails with the finding address: tor; all flags false passes | green | crates/lens/tests/it/fixture_evidence_ip.rs |
| C15 | IP from_evidence is a fixpoint | green | crates/lens/tests/it/fixture_evidence_ip.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 HTTP | 1 | sonnet | 79930 | 225 |
| G2 IP | 1 | sonnet | 43598 | 244 |
| G3 TLS | 1 | sonnet | 119390 | 251 |

### Review

- BLOCKER (fixed): TLS read the chain's `expired` key back as input; now `days_remaining < 0` only, as the module computes it.
- BLOCKER (fixed): TLS treated an empty `dns.caa` or `dns.https` facts block as an answer; a facts block counts only when it holds records, as in the module.
- BLOCKER (fixed): IP counted CNAME rows of `dns.a`/`dns.aaaa` in the address total; now distinct A and AAAA data.
- AMENDMENT (repaired in phase): a lone self-signed certificate is not trusted; spec constraints updated.
- AMENDMENT (repaired in phase, spec): TLSA records and differing ALPN are outside the rebuild and stay out of the fixtures.
- DEFERRED: a chain whose leaf has no parseable `not_after` falls back to the wall clock (no fixture has one).
- DEFERRED (fixed anyway): probe suites are parsed from the `*_answer` evidence for `tls.weak_suites` findings.

### Behavioural verification

skipped: the phase adds test-support functions (`testing` feature) with no runtime entry point; the guard of Phase 3 exercises them over every fixture.
