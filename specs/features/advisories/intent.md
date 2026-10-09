# Intent: advisories

## Problem

`deny.toml` ignores seven RustSec advisories. Several have a fix today, one ignore is stale, and some comments state the wrong reachability or fix status. The mhost dependency pins hickory 0.25, whose advisories are fixed only in hickory ≥ 0.26.1, now available through mhost 0.12.0.

## Proposed outcome

`cargo deny check advisories` passes with only the three unmaintained-crate ignores that have no fix, each with a true reachability comment whose input bounds a test pins. mhost is 0.12.0, no hickory crate below 0.26.1 is in the tree, and prism's explicit-nameserver resolvers refuse non-global nameservers in mhost as a second layer.

## Affected users and systems

All six services through the dependency bumps (part of release 0.23.0); DNS results move where mhost 0.12's lint and resolver changes say so. No API shape changes.

## Constraints

Planning SDD security-correctness, Phase 2 (R2.1–R2.6), SC10, SC11, SC17. Ships in 0.23.0, never in a patch. Neutral wording in public artefacts.

## Open questions

None: the SDD decides them.
