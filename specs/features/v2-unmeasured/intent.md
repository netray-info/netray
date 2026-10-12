# Intent: v2-unmeasured

## Problem

Errors and timeouts reach the grade as `not_applicable`, `fail`, `warn` or even `pass`, so a run can get a letter it never measured, or lose one to a probe that never got an answer (SDD v2 S13, S32; R4 review defects). A mail-only domain never gets a letter (S31).

## Proposed outcome

Every check separates "the target is broken" from "could not check"; a weighted `unmeasured` check makes its run incomplete; a definite negative A/AAAA gives N/A IP, HTTP and TLS sections (SDD v2 V1.13, V1.15, V1.16).

## Affected users and systems

Lens users (grades, the new verdict `unmeasured`), all five check modules, the engine, the lens frontend.

## Constraints

Profile v3 stays byte-identical; the gate runs offline.

## Open questions

Settled in the spec interview (2026-10-12).
