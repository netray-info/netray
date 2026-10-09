# Intent: email scoring

## Problem

lens's email tests run on invented fixtures; a beacon timeout, a Null-MX domain and all-info buckets score Pass; two beacon categories are silently dropped; a parked domain's revoked DKIM key grades it D.

## Proposed outcome

SDD security-correctness Phase 4 (R4.1–R4.5) holds, proven against beacon's own goldens.

## Affected users and systems

Everyone checking a domain's email posture on lens (grades change in 0.23.0); beacon's DKIM result and cross-validation output.

## Constraints

Ships in 0.23.0 (SC1); status words keep one spelling (SC16); pinned rows move only with `ADLC-Test-Change`.

## Open questions

None; decided in the spec's interview (2026-10-09).
