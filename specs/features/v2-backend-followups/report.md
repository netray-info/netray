# Report: v2-backend-followups

## Phase 1 — HTTP evidence: every Cache-Control directive, unreadable isolation headers

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 http.caching holds one directive per Cache-Control directive, received order, name lowercased, argument as received | green | crates/http/tests/it/caching_directives.rs |
| C2 | R2 the caching view adds the max-age chip only without a stored max-age directive | green | crates/web/tests/it/views_http_caching.rs |
| C3 | R3 http.isolation writes value, unreadable or missing per header; register declares unreadable | green | crates/http/tests/it/isolation_unreadable.rs |
| C4 | R4 http.coop keeps its status; unreadable finding names the case | green | crates/http/tests/it/isolation_unreadable.rs |
| C5 | R5 the isolation view claims unreadable and refuses contradictions | green | crates/web/tests/it/views_http_isolation.rs |
| C6 | R8 SDD states requirements 1 and 3 | test-unwritable | specs/sdd/v2.md (prose) |
| C7 | public, max-age=3600, must-revalidate gives three directives in order and max_age 3600 | green | crates/http/tests/it/caching_directives.rs |
| C8 | two Cache-Control lines give four directives in order, Max-Age lowercased | green | crates/http/tests/it/caching_directives.rs |
| C9 | a quoted argument with a comma stays one directive | green | crates/http/tests/it/caching_directives.rs |
| C10 | empty list members are skipped and the block validates | green | crates/http/tests/it/caching_directives.rs |
| C11 | no Cache-Control gives no directive | green | crates/http/tests/it/caching_directives.rs |
| C12 | a stored six-flag block renders three chips, the last max-age=3600 | already_implemented | crates/web/tests/it/views_http_caching.rs |
| C13 | a block with a max-age=3600 directive shows it once | green | crates/web/tests/it/views_http_caching.rs |
| C14 | an unreadable COOP gives unreadable coop and missing coep, corp | green | crates/http/tests/it/isolation_unreadable.rs |
| C15 | unreadable COEP and CORP beside readable COOP | green | crates/http/tests/it/isolation_unreadable.rs |
| C16 | all three readable give neither missing nor unreadable | already_implemented | crates/http/tests/it/isolation_unreadable.rs |
| C17 | http.coop findings for unreadable, missing, same-origin | green | crates/http/tests/it/isolation_unreadable.rs |
| C18 | the V1 security.coop of an unreadable COOP is unchanged | already_implemented | crates/http/tests/it/isolation_unreadable.rs |
| C19 | the isolation view shows unreadable, COOP in the check tone | green | crates/web/tests/it/views_http_isolation.rs |
| C20 | contradicting or unknown unreadable names are not claimed | already_implemented | crates/web/tests/it/views_http_isolation.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | orchestrator | 0 | 0 |
| G2 | 1 | orchestrator | 0 | 0 |
| G3 | 1 | orchestrator | 0 | 0 |

### Review

Reader: 0 blockers, 2 amendments (R4 replay wording, fingerprint note in the decision log; both
repaired in the spec in this phase), 2 nits (C20 status corrected; the protected
`caching_evidence.rs` change carries its `ADLC-Test-Change` trailer in this commit).

### Behavioural verification

skipped: a worktree binary has no network, and the HTTP module is reached through
`assemble_response` in the offline tests; the five history fixtures were recomputed
(`UPDATE_FIXTURES=1 ... every_history_fixture`) and gained the `max-age=300` directive.

## Phase 2 — Per-port checks in multi-port TLS exports

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6 multi-port TLS exports keep the band checks and add each port's checks, errors as checks@port rows; live equals shared | green | crates/web/tests/it/export_tls_ports.rs |
| C2 | R7 one-port TLS and other inspectors' exports unchanged | green | crates/web/tests/it/export_tls_ports.rs |
| C3 | JSON checks hold the band checks and ports hold 443 then 465 with their own checks | green | crates/web/tests/it/export_tls_ports.rs |
| C4 | CSV rows checks@443 and checks@465 follow checks with each port's status | green | crates/web/tests/it/export_tls_ports.rs |
| C5 | Markdown sections Checks · port 443 and Checks · port 465 follow Checks | green | crates/web/tests/it/export_tls_ports.rs |
| C6 | a port whose checks timed out gives the checks@465 error row in all three exports and no checks of 465 | green | crates/web/tests/it/export_tls_ports.rs |
| C7 | a one-port TLS run exports as today | already_implemented | crates/web/tests/it/export_tls_ports.rs |
| C8 | a DNS or HTTP run exports as today | already_implemented | crates/web/tests/it/export_tls_ports.rs |
| C9 | a finished two-port run and its shared copy export equally | green | crates/web/tests/it/export_tls_ports.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 export.rs + inspect.rs | 1 | orchestrator | 0 | 0 |

### Review

Reader: 0 blockers, 0 amendments, 2 nits. The c4 test's empty-finding expectation contradicted
R6 ("same columns as checks": a failing check without findings carries the catalogue finding);
corrected with an `ADLC-Test-Change` trailer, as are the `ports: &[]` additions to four protected
test files. c7 and c8 assert the absence of every per-port part, not byte equality with the old
output; the reader traced byte identity by code.

### Behavioural verification

skipped: a worktree binary has no network; the inspector route with a stub job is exercised end to
end by `crates/web/tests/it/export_tls_ports.rs` (live stream and shared page).
