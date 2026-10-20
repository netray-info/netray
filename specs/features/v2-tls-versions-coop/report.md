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
