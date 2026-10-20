# Intent: v2-tls-probe-close

## Problem

The ClientHello probes of v2-tls-versions-coop read a server that closes on an old version
(Windows SChannel) as unanswered, run uncharged by the rate limiters, and hold the handshake permit
while a hanging server lets them time out.

## Proposed outcome

Operator decision 2026-10-20: a close after the ClientHello is a refusal; probe connections are
charged; the handshake permit is released after the handshake and probes have their own bounded
concurrency and deadline.

## Affected users and systems

`netray-tls` (probes, inspection, module), `netray-web` (inspector job admission), SDD S41.

## Constraints

Rule M, the v2-input-edges refusal shape, no new config keys, no V1 wire change.

## Open questions

None.
