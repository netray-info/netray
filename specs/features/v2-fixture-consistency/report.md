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

## Phase 2 — DNS and email from evidence

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3 DNS from_evidence rebuilds one lookup per dns block with computed DNSKEY key tags, runs the lints, the NS checks over an authoritative stub and translate | green | crates/lens/tests/it/fixture_evidence_dns.rs |
| C2 | R4 email from_evidence seeds an offline resolver from the email blocks, serves a recorded MTA-STS policy on loopback, runs the inspection and translate | green | crates/lens/tests/it/fixture_evidence_email.rs |
| C3 | R6 DNS and email from_evidence over their own output return the same outcome | green | crates/lens/tests/it/fixture_evidence_dns.rs, crates/lens/tests/it/fixture_evidence_email.rs |
| C4 | dns.caa empty: dns.caa warns; an issue and an issuewild record: it passes | green | crates/lens/tests/it/fixture_evidence_dns.rs |
| C5 | DS key tag equal to the KSK's: dnssec_rollover passes; a DS tag no DNSKEY carries: it fails | green | crates/lens/tests/it/fixture_evidence_dns.rs |
| C6 | one NS record: dns.ns warns; two: it passes | green | crates/lens/tests/it/fixture_evidence_dns.rs |
| C7 | DNS from_evidence is a fixpoint | green | crates/lens/tests/it/fixture_evidence_dns.rs |
| C8 | DMARC p=reject: email.dmarc not warn with a p=none finding; p=none: it warns | green | crates/lens/tests/it/fixture_evidence_email.rs |
| C9 | no DKIM key: email.dkim not pass; one valid RSA 2048 key: it passes | green | crates/lens/tests/it/fixture_evidence_email.rs |
| C10 | MTA-STS policy status 200 enforce listing the MX: email.mta_sts passes; testing mode: it warns | green | crates/lens/tests/it/fixture_evidence_email.rs |
| C11 | email from_evidence is a fixpoint | green | crates/lens/tests/it/fixture_evidence_email.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 DNS | 1 | opus | 168723 | 437 |
| G2 email | 1 (+1 review repair) | opus | 227331 | 566 |

### Review

- AMENDMENT (repaired in phase): a missing email block is rebuilt as a failed lookup of its input (Rule M), as in DNS; DANE trims the row host's trailing dot.
- AMENDMENT (spec, repaired in phase): the NSEC `\000` next name, SPF include trees, BIMI logo fetches and non-blocked MTA-STS fetch failures are outside the rebuild; the fixtures avoid them (Phase 3).
- Sound per the reader: the optional email dependencies do not reach the `netray` binary (`cargo tree -p netray --edges normal,build` holds no rcgen); DNSKEY key tags match hickory's.

### Behavioural verification

skipped: test-support functions behind the `testing` feature, no runtime entry point; Phase 3's guard runs them over every fixture.
