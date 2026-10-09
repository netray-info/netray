# Review: raw query policy

## e408d7e..9eff7bc

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result found. Traced: every send in `dns_raw::RawOutbound` and all five callers (NS lame, NS delegation, authcompare, the DNSSEC walk incl. DS queries to parent servers, the trace walk) use either the fixed root list or `raw.build_server_list`; no other socket or resolver in prism takes domain-derived addresses; the refused count cannot underflow; refused glue is not resolved again; the DNSSEC walk resets `refused` per hop and the warning level appears only when refusals empty the list; the NS checks and authcompare report as specified; public nameservers behave as before (the trace's old filter equals `is_allowed_target`); every new test fails without its code and needs no network.

### Refuted

None: no BLOCKER or MAJOR. Anchor check skipped (COUNTS all zero).

### Calibration

No refuter ran. verified 0, held 0.

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | absence | no enum, UI or API copy; one new neutral string |
| P10 | convergence | a refused address becomes a Warning finding or warning, never Pass or N/A; the trace drops refused glue silently, as before |
| P12 | convergence | every send carries the query timeout; glue resolution unchanged |
| P13 | absence | no limiter or route change |
| P18 | convergence | one policy (`RawOutbound.allow = is_allowed_target`) for every raw query; no new client |
| P26 | absence | no new check ID |
| P35 | divergence → repaired | the rule lived only in prism's CLAUDE.md checklist; `tests/repo/test_raw_query_outbound.sh` now fails when a DNS socket or resolver appears outside `dns_raw.rs` (commit after the reading) |
| P36 | absence | no generated artefact |
| P40 | absence | no module added or removed |

### Summary

0/0/0 before and after refutation. verified 0, held 0. Roll call: 9 answered, 3 convergence, 1 divergence (repaired), 5 absence.
