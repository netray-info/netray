# Intent: v2-design-system

## Problem

V2's pages (Phase 4) need one token set, one implementation per component and a gate that keeps it so; none exists, and the identity marks (logo W5b, favicon, badge mark) exist only as a design canvas.

## Proposed outcome

SDD Phase 4a (S38): V4.1 tokens and the contrast test, V4.2 the gate, V4.3 the R44 components as askama macros with their stylesheets, V4.14 the logo, favicon, badge mark and the tagline partial, all in a new library crate `netray-web` that nothing serves yet.

## Affected users and systems

No user sees a change: the crate has no route and the binary does not depend on it. The gate becomes part of `just adlc-verify`.

## Constraints

No release of its own (S38); ships with the next one, expected 0.27.0. The result page (V4.4–V4.13) is out.

## Open questions

None; interviewed 2026-10-14.
