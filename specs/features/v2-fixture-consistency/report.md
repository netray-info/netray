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

## Phase 3 — Guard and consistent fixtures

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7 every fixture equals lens's run over modules answering from_evidence of its blocks, engine_version aside; a difference names the fixture and the first differing check or block | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C2 | R8 UPDATE_FIXTURES=1 rewrites each fixture's run, keeping domain, age_seconds and engine_version | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C3 | R9 every fixture passes R7, the story statuses and grades hold, the catalogue and design fixture checks pass | green | crates/lens/tests/it/history_fixtures_consistent.rs, crates/lens/tests/it/history_fixtures_story.rs, tests/repo/test_design_fixtures.sh, crates/engine/tests/it/history_fixtures.rs |
| C4 | R10 just design-check exits 0 against the private manifest | green | design-check |
| C5 | a stored email.dmarc status differing from the recomputed one fails naming the fixture and email.dmarc | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C6 | every committed fixture passes the guard | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C7 | a regenerated fixture keeps domain, age_seconds and engine_version and then passes the guard | green | crates/lens/tests/it/history_fixtures_consistent.rs |
| C8 | shop run 1: dkim fail, dmarc fail, mta_sts warn, hsts pass, expiry_window warn; run 2 the same with dmarc warn; run 3: dkim, mta_sts, expiry_window pass, dmarc warn, hsts fail | green | crates/lens/tests/it/history_fixtures_story.rs |
| C9 | new, the shop runs and the long domain graded A; example.com and stale B | green | crates/lens/tests/it/history_fixtures_story.rs |
| C10 | just design-check exits 0 | green | design-check |

C5, C7, C8 and C9 passed on the committed fixtures when written (`already_implemented`
in effect: the guard's mechanics and the story statuses held); they are kept as protection. C1/C6
failed on all seven fixtures until the evidence below was corrected.

### Contradictions found and fixed

Recomputing every fixture through the modules (UPDATE_FIXTURES=1 over the unedited fixtures)
showed these statuses contradicting their own evidence; each was resolved by changing the
evidence so the stored status follows, except where noted.

| fixture(s) | check | stored | evidence said | resolution |
|---|---|---|---|---|
| new, shop 1–3, long domain | http.https_redirect | pass | port-80 probe 200, no redirect | port-80 hop 301 to https |
| new, shop 1–3, long domain | http.x_frame_options, x_content_type_options, referrer_policy, permissions_policy | pass | headers not sent | headers added |
| new, shop 1–3, long domain | http.info_leakage | pass | `server: example-edge` sent | Server header removed |
| new, shop 1–3, long domain | http.caching | pass | no Cache-Control | `cache-control: public, max-age=300` |
| shop 1, 2 | http.hsts | pass | no Strict-Transport-Security | header `max-age=31536000` added |
| all | tls.chain_trusted, tls.chain_complete | pass | chain ends at a cross-signed root (not self-signed) | last item a self-signed root |
| all | tls.aia_reachability | not_applicable | (follows chain_complete) | follows the root fix |
| all | tls.ocsp_stapled | pass | leaf has no OCSP URL | leaf `ocsp_url` added |
| shop 1, 2 | tls.expiry_window | warn | leaf 68 days left, same cert as run 3 | older certificate (own fingerprint) with 21 and 14 days left at runs 14 and 7 days before run 3; every chain's `days_remaining`, the `date` and `last-modified` headers follow that one clock |
| shop 1, 2 | email.dkim | fail | a valid RSA 2048 key | RSA 512 key (fails < 1024) |
| shop 1 | email.dmarc | fail | a p=reject record | no DMARC record |
| shop 1, 2 | email.mta_sts | warn | enforce policy | policy in testing mode |
| new, shop 2, 3 | email.dmarc | warn "p=none" | p=reject record | status kept; finding now the real cause "no aggregate reports" (no rua) |
| all | email.dane, email.fcrdns, email.dnsbl | pass/warn | no TLSA, PTR or DNSBL evidence at all | DANE, FCrDNS and DNSBL blocks added; example.com and stale have no TLSA: dane not_applicable (was warn, unreachable from evidence) |
| all | http.coop | not_applicable "not reported" | the current module judges COOP | A fixtures send `same-origin` (pass); example.com, stale warn |
| all | dns.nsec block | `\000` next name | unreadable by the parser | next name `www.<domain>.` |

Also regenerated with the module output: `profile_version` 3 to 4, `http.server` `response_ms` to
`inspection_ms`, the MX host without its trailing dot, DKIM state `valid` to the module's `ok`, and
the blocks the current modules emit (`http.hsts`, `http.caching`, `http.isolation`, `email.*`).
Grades hold: new, shop 1–3, long domain A; example.com and stale B.

Two web tests asserted the old hand-made values (MX trailing dot, DKIM state `valid`) and inserted
`null` for a check without `evidence` (the module omits an empty list): updated with
`ADLC-Test-Change`.

### Review

- AMENDMENT (repaired): the new self-signed roots carried an RSA signature on an EC key, a validity shorter than their subordinates' and the cross-sign's fingerprint: now `ecdsa-with-SHA384`, 2020 to 2045, own fingerprint.
- AMENDMENT (repaired): shop runs 1 and 2 mixed run 3's CA `days_remaining`, `date` and `last-modified` with the older leaf: all derived from one measurement time per run.
- AMENDMENT (repaired): `http.server/inspection_ms` regenerated as 0 (a placeholder): the measured 289 ms carried over from the old `response_ms` key.
- AMENDMENT (repaired): NSEC next name `www.<domain>.` denied names the zone uses: `0.<domain>.`.
- NIT (repaired): shop runs 1 and 2 have their own MTA-STS id `20260901`; the older leaf's dates fit `age_seconds`.
- NIT (decision log): the DNSBL blocks name the module's real zones (zen.spamhaus.org and others): the rebuild queries the module's configured zones, as the contract goldens do.
- DEFERRED (repaired): DS digests are the SHA-256 over each zone's own KSK; leaf fingerprints are distinct per domain.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 fixtures (evidence edits and regeneration, by the implementer) | 3 regenerations | implementer | n/a | n/a |

### Behavioural verification

`just design-check` against the private manifest: `0 open deviation(s), 0 stale accepted entries`
(seeded 7 fixtures). The manifest is unchanged.

## Review fixes

The range review (review.md, f22cbe6e..06750cd2) found two blockers, both fixed on this branch:
the email rebuild passed the DKIM rows' selectors as caller selectors, which a recorded run never
has (fixtures now keep their key under `default`); the TLS rebuild denied a lone self-signed
certificate its anchor, failing `tls.chain_complete` where the module passes. Regression tests:
`email_from_evidence_finds_only_keys_under_selectors_the_module_probes`,
`a_lone_self_signed_certificate_is_untrusted_but_complete` (red before the fix, green after). The
fixtures also carry an empty `dns.rrsig` block (a minor of both readers). `just design-check`: 0
open after the regeneration.
