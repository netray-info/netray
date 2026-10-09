# Intent: grade integrity

## Problem

lens grades can be better than what was measured: errored or timed-out sections drop out of a grade that is then cached, snapshotted and badged; an HTTPS-less site gets no TLS penalty; the hard deadline throws away finished sections; unknown verdicts read as pass; four blocklists disagree.

## Proposed outcome

SDD security-correctness Phase 3 (R3.1–R3.7) plus R5.3 hold: incomplete results have no letter and are never cached, one deadline per backend call, `tls_reachable` hard-fails unreachable TLS, one blocklist.

## Affected users and systems

Everyone checking a domain on lens (grades change visibly in 0.23.0); tlsight's API (`tls_reachable`, `NOT_TESTED_FROM_HERE`); argus's lens timeouts (K2).

## Constraints

Ships in 0.23.0 (SC1); snapshot DB untouched (SC15); status words keep one spelling (SC16); pinned rows move only with `ADLC-Test-Change`.

## Open questions

None; decided in the spec's interview (2026-10-09).
