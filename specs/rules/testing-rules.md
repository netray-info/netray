# Testing rules

Governs the repository's own checks: `tests/repo/*.sh`, Rust tests, and the acceptance suite.

- A check that could pass on empty or unrelated input must prove what it observed: a scan reports how many files it read and fails on zero; an error assertion greps for text only the intended failure path produces, never a key name an unrelated error also lists. Unenforced; precedents in `specs/solutions/` with class `check-passes-without-observing`.
- Goldens are produced from the producer's real types, constants and encoding path, never from hand-typed identifiers or invented values; a golden with invented beacon sub-check names let the MX bug through. Partly enforced: golden tests compare byte for byte, beacon exports `NO_MX`; the rest is unenforced.
- A change to a grade, verdict or backend check result moves a pinned row (lens golden or results table) in a commit carrying `ADLC-Test-Change` naming the requirement. Enforced by the gate's `ADLC-Baseline` trailer, which refuses a weakened pinned test without it.
- mhost's test constructors (`Lookup::new_for_test` and friends) are `#[cfg(test)]`; an integration test builds `Lookups` through mhost's serde form. Unenforced.
