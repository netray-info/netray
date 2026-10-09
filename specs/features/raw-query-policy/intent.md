# Intent: raw query policy

## Problem

prism sends raw DNS queries to addresses taken from a checked domain's data without the outbound policy.

## Proposed outcome

One outbound policy in `dns_raw` for every such query.

## Affected users and systems

prism `+check` NS checks, authcompare, the DNSSEC chain walk, the trace walk.

## Constraints

Ships in 0.23.0; neutral wording.

## Open questions

None.
