# Intent: v2-evidence

## Problem

Checks carry no evidence: `CheckResult.evidence` is never set, and what a module measured reaches lens only as the transitional `presentation` JSON (SDD v2 §3.6). History, diff and the evidence views of V2 need typed, stable evidence blocks (§3.7, V1.14).

## Proposed outcome

Every module projects its measurements into the six evidence shapes with registered, typed keys and volatile paths; checks reference their blocks; `presentation` is gone and lens adds the blocks to its wire.

## Affected users and systems

All five modules, the engine, beacon's V1 SSE (one additive message), lens's wire (additive), MCP consumers.

## Constraints

The lens wire's existing fields stay byte-identical; the gate runs offline.

## Open questions

Settled in the spec interview (2026-10-13).
