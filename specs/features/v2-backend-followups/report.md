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
