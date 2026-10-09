# Intent: backend correctness

## Problem

prism lints signed zones with a false RRSIG question and double-counts records answered by several resolvers; lens's IP reputation hides blocklisted cloud addresses and passes failed enrichments; attacker-chosen DNS text reaches a Markdown export unescaped; prism's UI offers `@system` where the server refuses it.

## Proposed outcome

SDD security-correctness Phase 5 parts R5.1, R5.2, R5.5 and R5.9 hold (R5.8 is its own feature).

## Affected users and systems

prism `+check` users and lens's DNS section; lens's IP section (grades change in 0.23.0); lens's export; prism's query UI; argus's prism config (K5).

## Constraints

Ships in 0.23.0 (SC1, SC16); neutral wording (SC2, SC17); pinned rows move only with `ADLC-Test-Change`.

## Open questions

None.
