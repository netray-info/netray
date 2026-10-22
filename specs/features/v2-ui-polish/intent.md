# Intent: v2-ui-polish

## Problem

Features 2 to 7 of the V2 build left visual findings outside their scope: the design gate can pass
a live board that never reached its state, the DNS query builder shows native controls, a note
meant for `+check` queries sits on every DNS page, the run actions and the evidence view's
tools wrap at 390 px, a refused live run replaces the result region with a bare line, the email
evidence view leaves loose links, clipped tables and an empty card, guide prose runs the full
container width, the shell's check list has no card frame, spectrum label leaders cross
neighbouring labels, long facts wrap in a third-width cell, and the phone header crowds the Theme
link.

## Proposed outcome

Each finding is rebuilt to the nearest board (F, G, M) with existing tokens, verified at 1280
and 390 px in both themes, and `just design-check` stays at 0 open deviations.

## Affected users and systems

Visitors of the inspector pages, evidence views, guides and result pages; `crates/web`
(templates, static, islands, spectrum layout, explore links) and the design-check tooling in
`tests/acceptance/design/`.

## Constraints

No backend crate changes (lane B owns `crates/http`, `crates/tls`). No new token without need.
No board edits (the canvas artifact is unreachable): pages without a board follow the nearest
board, listed as canvas proposals.

## Open questions

None.
