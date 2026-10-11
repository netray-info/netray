---
class: neighbour-artefact-left-inconsistent
repeat-of: none
---
# A golden fixture contradicted its producer's own rules, and a recomputation read as a grade change

**What failed.** `tests/fixtures/contracts/tlsight-inspect.json` carries a hand-written port `quality` list that tlsight's rules would not produce: `expiry_window` warn for a certificate valid 60 more days (tlsight warns at 30, `crates/tls/src/quality/checks.rs`) and an `ocsp_stapled` fail in the summary (tlsight's `check_ocsp_stapled` never fails). V1 lens read that list verbatim, so every lens golden's TLS section was B. When v2-catalogue phase 3 computed the TLS checks per address from the raw data, every golden moved to TLS A+, which first looked like a scoring shift of v3.

**What worked.** Reading the diff of each regenerated golden check by check against the producer's rule, and recording in `specs/features/v2-catalogue/report.md` (phase 3, grade changes) which shift is a fixture artefact and which is v3. The real shift is measured on the V1 baseline snapshots before the release, not on the goldens.

**How to notice next time.** A golden grade jumps the moment code recomputes a field it used to read from the fixture: check the fixture field against the producer's rule before explaining the jump as a scoring change.
