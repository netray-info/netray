---
class: guard-misclassifies-lookalike-input
repeat-of: specs/solutions/2026-10-09-ehostunreach-is-not-local.md
---
# Inverting the gate's colour-property allow-list refused `transition: background`

**What failed.** The gate's rule 1 counted named colours only in a list of colour-bearing properties, so `mask-image: … black` passed (review 05e7d64..ee21507). The fix inverted it: named colours count everywhere except `IDENTIFIER_PROPERTIES` (c720bfc). The list held the longhands `transition-property` and `animation-name` but not their shorthands, and the deprecated system colours `Background` and `Highlight` read as colour words, so `transition: background 150ms ease` and `animation: highlight 1s` were refused (review ee21507..c720bfc).
**What worked.** `transition`, `animation` and `appearance` joined `IDENTIFIER_PROPERTIES` (528dd43), with a table row per shorthand and one that keeps `background: red` beside a `transition` refused (551a011).
**How to notice next time.** When a scope list flips from allow to deny, every shorthand of an excepted longhand needs its own passing row.
