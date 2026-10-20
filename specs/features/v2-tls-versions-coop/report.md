# Report: v2-tls-versions-coop

## Phase 1 — ClientHello probes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 client_hello builds one record per probe kind with the versions, suites and extensions stated | green | crates/tls/tests/it/probe_client_hello.rs |
| C2 | R2 classify reads the first answer into accepted, refused or unanswered | green | crates/tls/tests/it/probe_classify.rs, probe_alerts.rs |
| C3 | R3 probe_address runs the fixed probe set concurrently with derived TLS 1.2 and 1.3 | green | crates/tls/tests/it/probe_address.rs |
| C4 | TLS 1.0 with SNI: record 22, versions 0x0301, server_name, no supported_versions, signature_algorithms or fallback SCSV | green | crates/tls/tests/it/probe_client_hello.rs |
| C5 | SSLv3: record and client version 0x0300, no extensions | green | crates/tls/tests/it/probe_client_hello.rs |
| C6 | TLS 1.2: client version 0x0303 with signature_algorithms | green | crates/tls/tests/it/probe_client_hello.rs |
| C7 | address SNI or none: no server_name | green | crates/tls/tests/it/probe_client_hello.rs |
| C8 | suite groups offer exactly their suites plus the renegotiation SCSV | green | crates/tls/tests/it/probe_client_hello.rs |
| C9 | ServerHello TLS 1.0 suite 0xC013 on the TLS 1.0 probe is accepted with names | green | crates/tls/tests/it/probe_classify.rs |
| C10 | ServerHello TLS 1.2 on the TLS 1.1 probe is refused naming TLSv1.2 | green | crates/tls/tests/it/probe_classify.rs |
| C11 | alert 70 is refused naming protocol_version | green | crates/tls/tests/it/probe_classify.rs |
| C12 | ServerHello TLS 1.0 with an insecure suite on the insecure probe is accepted | green | crates/tls/tests/it/probe_classify.rs |
| C13 | ServerHello with an unoffered suite is unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C14 | ServerHello split over two records is read whole | green | crates/tls/tests/it/probe_classify.rs |
| C15 | end of stream, reset or HTTP text is unanswered | green | crates/tls/tests/it/probe_classify.rs |
| C16 | responder accepting 1.0 and 1.1 with handshake 1.3: seven connections and the stated verdicts | green | crates/tls/tests/it/probe_address.rs |
| C17 | handshake 1.2: TLS 1.2 enabled from handshake, 1.3 disabled, six connections | green | crates/tls/tests/it/probe_address.rs |
| C18 | silent responder with 200 ms timeout: every probe unanswered within about one timeout | green | crates/tls/tests/it/probe_address.rs |
| C19 | responder resetting after the ClientHello: unanswered (reset) | green | crates/tls/tests/it/probe_address.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 60517 | 147 |

### Review

- BLOCKER (fixed): a warning alert before the ServerHello was read as a refusal; warnings are now skipped (`probe_alerts.rs`).
- AMENDMENT (repaired in phase): only fatal handshake_failure, protocol_version or insufficient_security alerts refuse; other alerts are unanswered (spec R2 amended).
- NIT (fixed): a suite-group ServerHello below SSLv3 was accepted.
- NIT: a read error after connect is labelled "connect failed"; the verdict (unanswered) is right.

### Behavioural verification

skipped: the phase adds library functions only, exercised end to end against the in-process responder on 127.0.0.1; a live probe runs through the inspection in Phase 2.

## Phase 2 — Probes in the inspection and the evidence

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R4 inspect_port probes after a successful handshake via TlsBackend::probe into a serde-skipped IpInspectionResult.probes; stub answers none unless canned | green | crates/tls/tests/inspector.rs |
| C2 | R5 per-address tls.probes@addr:port with version and suite keys and their _answer; unanswered keys omitted | green | crates/tls/tests/it/probe_evidence.rs |
| C3 | R6 per-port tls.protocols (qualified with several ports) aggregating agree, mixed or omitted, with forward_secrecy_all_suites; registered, not volatile | green | crates/tls/tests/it/probe_evidence.rs |
| C4 | stub with canned probes, one address: tls.probes and tls.protocols present and the outcome validates | green | crates/tls/tests/it/probe_evidence.rs, crates/tls/tests/inspector.rs |
| C5 | TLS 1.0 unanswered: no tls1_0 or tls1_0_answer in tls.probes, no tls1_0 in tls.protocols | green | crates/tls/tests/it/probe_evidence.rs |
| C6 | two addresses differing on TLS 1.0: mixed; both disabled: disabled | green | crates/tls/tests/it/probe_evidence.rs |
| C7 | forward_secrecy_all_suites false, true or absent | green | crates/tls/tests/it/probe_evidence.rs |
| C8 | stub without canned probes: no tls.probes and no tls.protocols | green | crates/tls/tests/it/probe_evidence.rs, crates/tls/tests/inspector.rs |
| C9 | two-port translate_ports: each outcome holds tls.protocols@port | green | crates/tls/tests/it/probe_evidence.rs |
| C10 | failed handshake: no probe call; successful: one call with the negotiated version | green | crates/tls/tests/inspector.rs |
| C11 | IpInspectionResult with probes serialises without a probes key | green | crates/tls/tests/it/probe_evidence.rs |
| C12 | no volatile path names tls.probes or tls.protocols | green | crates/tls/tests/it/probe_evidence.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 58877 | 93 |

### Review

- AMENDMENT (repaired in phase): SDD §3.7 tls row now states the probes and the two blocks.
- DEFERRED: the handshake permit is held until the slowest probe ends; under contention with servers that never answer, queued inspections can reach the request timeout. Accepted for now (spec R4); revisit with production latency data.
- DEFERRED: `/api/v1/tls/inspect` and V1 `/api/inspect` run the probes but serialise no probe data (serde-skipped); the inspector page and the domain run use them.
- DEFERRED: `forward_secrecy_all_suites` reads the static-RSA probe only; static ECDH/DH suites (0xC031, 0x0031) are not offered by any probe.

### Behavioural verification

The Rust example binary path is blocked locally (Little Snitch per binary path; every probe timed out while the same host answered python). The ClientHellos `client_hello` builds, sent from python to 1.1.1.1:443 with SNI one.one.one.one, got:

```
ssl3              15 03 01 00 02 02 46  fatal alert protocol_version  -> refused
tls1_0            15 03 01 00 02 02 28  fatal alert handshake_failure -> refused
tls1_1            15 03 02 00 02 02 28  fatal alert handshake_failure -> refused
tls1_2            16 03 03 00 5b 02 ...  ServerHello TLS 1.2         -> accepted
suites_insecure   15 03 03 00 02 02 28  fatal alert handshake_failure -> refused
suites_cbc        16 03 03 00 5b 02 ...  ServerHello                 -> accepted (ECDHE CBC)
suites_static_rsa 15 03 03 00 02 02 28  fatal alert handshake_failure -> refused
```

## Phase 3 — Checks, catalogue and profile v4

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R7 tls.legacy_versions per address and over addresses, not_applicable without probes, follows tls_reachable without handshake | green | crates/tls/tests/it/probe_checks.rs |
| C2 | R8 tls.weak_suites per address and over addresses | green | crates/tls/tests/it/probe_checks.rs |
| C3 | R9 both checks reference the probes blocks at the reported status and tls.protocols; unmeasured references nothing; 23 TLS checks | green | crates/tls/tests/it/probe_checks.rs |
| C4 | R10 http.coop pass, warn, probe override, references; 13 HTTP checks | green | crates/http/tests/it/coop_check.rs |
| C5 | R11 catalogue texts, checkMeta, guide, compare rows | green | tests/repo/test_checkmeta_catalogue.sh, crates/engine/tests/it/profile_v4.rs, crates/web/tests/it/pages_compare.rs |
| C6 | R12 profile v4 is v3 plus three weights; current is v4; v3 loadable; 57 catalogue ids | green | crates/engine/tests/it/profile.rs, crates/engine/tests/it/catalogue.rs |
| C7 | R13 SDD and CHANGELOG state probes, blocks, checks, v4 and the grade change | test-unwritable | prose (SDD §2, §3.7-§3.9, CHANGELOG); tests/repo/test_spectrum_deps.sh holds the SDD and token counts |
| C8 | TLS 1.0 enabled: legacy_versions fail naming TLS 1.0 with references | green | crates/tls/tests/it/probe_checks.rs |
| C9 | all three legacy versions disabled: pass | green | crates/tls/tests/it/probe_checks.rs |
| C10 | TLS 1.1 unanswered, others disabled: unmeasured, no references, TLS section not scored | green | crates/tls/tests/it/probe_checks.rs |
| C11 | TLS 1.0 enabled on one address, TLS 1.1 unanswered on another: fail | green | crates/tls/tests/it/probe_checks.rs |
| C12 | insecure suite accepted: weak_suites fail naming group and suite | green | crates/tls/tests/it/probe_checks.rs |
| C13 | insecure refused, CBC or static RSA accepted: warn | green | crates/tls/tests/it/probe_checks.rs |
| C14 | all groups refused: pass | green | crates/tls/tests/it/probe_checks.rs |
| C15 | insecure unanswered, CBC accepted: unmeasured | green | crates/tls/tests/it/probe_checks.rs |
| C16 | no probes: both not_applicable not reported; no handshake: not_applicable no TLS handshake | green | crates/tls/tests/it/probe_checks.rs |
| C17 | TLS module checks are the 23 catalogue TLS ids | green | crates/tls/tests/it/probe_checks.rs |
| C18 | COOP same-origin pass with references; allow-popups with report-to pass; unsafe-none warn; missing warn | green | crates/http/tests/it/coop_check.rs |
| C19 | refused HTTPS probe: coop fail; timed out: unmeasured | green | crates/http/tests/it/coop_check.rs |
| C20 | catalogue 57 ids, 23 TLS, 13 HTTP, texts, owner, severities by weight | green | crates/engine/tests/it/profile_v4.rs, crates/engine/tests/it/catalogue.rs |
| C21 | Profiles::current is v4 with v3 plus three weights; v3 loads without them; lock matches both | green | crates/engine/tests/it/profile.rs |
| C22 | stored v3 and current v4 run diffed: grade change carries the v3 run rescored under v4 | green | crates/engine/tests/it/profile_v4.rs |
| C23 | checkMeta catalogue test and compare.toml ids pass | green | tests/repo/test_checkmeta_catalogue.sh, crates/web/tests/it/pages_compare.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 engine (catalogue, v4, lock) | 0 (orchestrator, data) | orchestrator | 0 | 0 |
| G2 tls checks | 1 | sonnet | 44633 | 49 |
| G3 http coop | 1 | sonnet | 35798 | 40 |

### Fallout

The new ids changed counts and goldens in protected tests of tls, http, lens and web (each named by an `ADLC-Test-Change` trailer of the phase commit): 21 to 23 TLS, 12 to 13 HTTP, 54 to 57 catalogue lines, current profile 3 to 4, lens goldens and island fixtures regenerated with `UPDATE_GOLDEN=1`, history fixtures given the three ids. Lens golden scores drop about 0.6 points (COOP warns), no letter changes.

### Review

- AMENDMENT (repaired in phase): COOP was judged by changing spectra's V1 security scorer, which changed the V1 wire; it is now judged in the HTTP module (`coop_verdict`) and the V1 scorer is unchanged.
- DEFERRED: a second Cross-Origin-Opener-Policy line is not seen (the V1 report keeps the first line); browsers would fail the joined value. The token is no longer trimmed around `;`, so `same-origin ;` warns.
- NIT (fixed): the CHANGELOG line on unanswered legacy probes now says "and accepts no other legacy version".

### Behavioural verification

skipped: the live domain run needs DNS, blocked locally for the netray binary; the probes were verified live in Phase 2 (python, same ClientHello bytes), and the checks are pure functions of the probe results, covered by `probe_checks.rs`.
