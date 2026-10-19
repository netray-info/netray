# Report: v2-http-checks

## Phase 1 — Check fixes: Referrer-Policy lists, inspection duration, Set-Cookie rows

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 Referrer-Policy graded by its effective policy (last known comma-separated token) | green | crates/http/tests/referrer_policy_list.rs |
| C2 | R2 http.server carries inspection_ms in place of response_ms, volatile likewise; lens fills response_duration_ms from it | green | crates/http/tests/server_inspection_ms.rs |
| C3 | R3 http.headers holds one row per Set-Cookie value (set-cookie, set-cookie#n) | green | crates/http/tests/set_cookie_rows.rs |
| C4 | R4 a volatile row path covers the rows keyed key#n | green | crates/model/tests/volatile_row_repeats.rs |
| C5 | R13 SDD states the phase's changes | test-unwritable | specs/sdd/v2.md (prose) |
| C6 | origin-when-cross-origin, strict-origin-when-cross-origin passes | green | crates/http/tests/referrer_policy_list.rs |
| C7 | no-referrer, unsafe-url warns Permissive policy: unsafe-url | green | crates/http/tests/referrer_policy_list.rs |
| C8 | strict-origin, made-up-policy passes | green | crates/http/tests/referrer_policy_list.rs |
| C9 | foo, bar warns Unrecognized value: foo, bar | green | crates/http/tests/referrer_policy_list.rs |
| C10 | padded mixed-case Strict-Origin passes | green | crates/http/tests/referrer_policy_list.rs |
| C11 | fixture http.server holds inspection_ms, no response_ms, validates | green | crates/http/tests/server_inspection_ms.rs |
| C12 | volatile list holds http.server/inspection_ms, not response_ms | green | crates/http/tests/server_inspection_ms.rs |
| C13 | lens response_duration_ms read from inspection_ms | green | crates/lens/src/modules.rs |
| C14 | three Set-Cookie values give rows set-cookie, set-cookie#2, set-cookie#3 | green | crates/http/tests/set_cookie_rows.rs |
| C15 | two Vary values stay one joined row | already_implemented | crates/http/tests/set_cookie_rows.rs |
| C16 | JSON replay with one set-cookie gives one row | already_implemented | crates/http/tests/set_cookie_rows.rs |
| C17 | differing Set-Cookie rows leave fingerprint and diff unchanged | green | crates/model/tests/volatile_row_repeats.rs |
| C18 | rows keyed set-cookiex or set-cookie#a stay compared | already_implemented | crates/model/tests/volatile_row_repeats.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 25611 | 18 |
| G2 | 1 | sonnet | 33267 | 70 |
| G3 | 1 | sonnet | 35901 | 339 |

### Review

One BLOCKER (Referrer-Policy read only the first header line) fixed in the phase: every line is graded joined with `, `
(`referrer_policy_lines_are_one_list`). Two AMENDMENTs repaired in the phase: a received `set-cookie#<n>` header keeps its
row (`set_cookie_rows_skip_a_received_set_cookie_hash_header`); the decision log now says when the `response_ms` removal
shows in a diff. NIT: the catalogue finding now names a value not readable as text. NIT: the trailers for the reformatted
and superseded protected tests are on the phase commit.

### Behavioural verification

skipped: the HTTP inspector resolves its target through public DNS, blocked locally (Little Snitch); the phase's paths
(`translate`, the canonical form, lens's http section) run offline in the tests above.
