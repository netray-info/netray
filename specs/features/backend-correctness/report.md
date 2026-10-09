# Report: backend correctness

## Phase 1 — prism lints

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C2 | Requirement 2: prism lints through one function over the unique records of all resolvers (one per name, type, data; highest TTL kept); identical lint lines within a category emitted once | green | crates/mhost-prism/tests/lint_results_table.rs |
| C4 | GIVEN a signed zone whose DNSKEY answer carries its RRSIG (expiring in 3 days) WHEN prism lints it THEN mhost's near-expiry line appears | green | crates/mhost-prism/tests/lint_results_table.rs |
| C5 | GIVEN a signed zone answered identically by two resolvers WHEN prism lints it THEN "Found 1 KSK(s) and 1 ZSK(s)" | green | crates/mhost-prism/tests/lint_results_table.rs |
| C6 | GIVEN two resolvers returning different records for one name WHEN prism lints THEN both records count | green | crates/mhost-prism/tests/lint_results_table.rs |
| C7 | GIVEN one A record with TTL 45 from one resolver and 300 from another WHEN prism lints THEN no low-TTL warning | green | crates/mhost-prism/tests/lint_results_table.rs |
| C8 | GIVEN two identical lint lines in one category WHEN prism emits the category THEN the line appears once | green | crates/mhost-prism/tests/lint_results_table.rs |
| C9 | GIVEN one resolver WHEN prism lints THEN the lines equal today's, except identical lines collapse (two RSASHA1 keys give one deprecation line) | green | crates/mhost-prism/tests/lint_results_table.rs |

The table first carried requirement 1 (DO bit) and its scenario as C1 and C3. The operator dropped them on 2026-10-09: prism keeps the explicit RRSIG question for 0.23.0, because mhost 0.12.0 offers no DO bit (see `### Halt`); the spec is amended accordingly. The ids C2 and C4–C9 are kept as they appear in the commits.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 30176 | 62 |
| G1 (BLOCKER fix, 8.1) | 1 | sonnet | 34892 | 29 |

### Review

- BLOCKER (fixed in the second pass) | `crates/mhost-prism/src/api/check.rs` `unique_records` | the first version round-tripped the whole `Lookups` through serde and fell back to the raw input on any error, so one name that does not round-trip (a `\000` label, as in a compact-denial NSEC next name) undid the deduplication for every record type | now per `Lookup`: an untouched lookup is kept as is, a fully duplicated one is dropped without deserialising, only a partially duplicated one is round-tripped, and on error that one lookup alone is kept. Not covered by a test: such a name cannot be built through the serde fixtures the tests use, and mhost's own constructors are private.
- Sound per the reader: category order (`ns_lame`, `ns_delegation` after `ns`), highest TTL kept at the first copy's position, `done` counts taken after collapsing, the prism contract golden unchanged (literal events).

### Behavioural verification

skipped: `POST /api/check` against a locally started `netray dns crates/mhost-prism/prism.dev.toml` returned `{"events":[],"truncated":true}`; the log shows every lookup to 8.8.8.8 and 8.8.4.4 ending in `Received Timeout error` (no outbound DNS from this sandbox).

### Halt (resolved)

Resolved 2026-10-09 by the operator: requirement 1 is amended to keep the RRSIG question. Reason given: open-decisions. Requirement 1 cannot be built as specified on mhost 0.12.0: mhost's `Resolver` gives no way to set the DO bit. `ResolverOpts::to_proto` (`mhost-0.12.0/src/resolver/mod.rs:427-439`) never sets hickory's `validate`, and hickory-resolver 0.26.3 sets `edns_set_dnssec_ok` only from `validate` (`hickory-resolver-0.26.3/src/resolver.rs:358-361`). mhost's public `raw_dnssec_query` is non-recursive (RD=0, `resolver/raw.rs:236-246`), meant for authoritative servers, and the constructors that would turn raw hickory records into mhost `Lookup`s (`Lookup::from_records`, `Record::from_proto`) are `pub(crate)`.
