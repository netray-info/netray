# Intent: sdd-into-monorepo

## Problem

The V2 SDD lives in the private planning repository, so every code change that amends it needs a second repository and a second session. Its token source `v2-tokens.json` is private, though §8.1 says it moves into the monorepo.

## Proposed outcome

`specs/sdd/v2.md` and its token source are in this repository, the CLAUDE.md pointers name them, and from then on the SDD is amended in the same commit as the code.

## Affected users and systems

Agents and the operator working in this repository; no service, no wire, no release.

## Constraints

Step 2 of the repo-layout plan (operator, 2026-10-14). Lands only after argus-oci reports 0.26.0 deployed.

## Open questions

- The two real domains in citations (`v2.md:50`, `v2.md:633`): with netray-v2.
