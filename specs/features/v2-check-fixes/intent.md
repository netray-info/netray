# Intent: v2-check-fixes

## Problem

Writing catalogue v3 (feature v2-catalogue) found five checks whose code contradicts their catalogue text: http.cors, tls.caa_compliant, tls.strong_signature, tls.chain_complete/aia_reachability, tls.chain_trusted.

## Proposed outcome

Each check does what its `checks.toml` entry says, before 0.25.0 ships.

## Affected users and systems

Everyone whose grade involves these checks; crates/http and crates/tls only.

## Constraints

No catalogue, weight or profile change; offline gate; neutral commit wording.

## Open questions

Answered in the spec interview (2026-10-11).
