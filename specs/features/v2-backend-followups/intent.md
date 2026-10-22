# Intent: v2-backend-followups

## Problem

Features v2-http-checks, v2-http-ui and v2-tls-ui left three backend proposals: `http.caching`
drops Cache-Control directives outside V1's six flags, `http.isolation` cannot tell an unreadable
COOP/COEP/CORP from a missing one, and multi-port TLS exports carry only the band port's checks.

## Proposed outcome

The three hold as stated in spec.md.

## Affected users and systems

Readers of the HTTP evidence and of TLS exports; crates `netray-http` and `netray-web`.

## Constraints

No grade change, no V1 wire change, no template change (lane A).

## Open questions

None.
