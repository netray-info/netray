# Testing rules

Governs the repository's own checks: `tests/repo/*.sh`, Rust tests, and the acceptance suite.

- A check that could pass on empty or unrelated input must prove what it observed: a scan reports how many files it read and fails on zero; an error assertion greps for text only the intended failure path produces, never a key name an unrelated error also lists. Unenforced; precedents in `specs/solutions/` with class `check-passes-without-observing`.
- Goldens are produced from the producer's real types, constants and encoding path, never from hand-typed identifiers or invented values; a golden with invented beacon sub-check names let the MX bug through. Partly enforced: golden tests compare byte for byte, beacon exports `NO_MX`; the rest is unenforced.
