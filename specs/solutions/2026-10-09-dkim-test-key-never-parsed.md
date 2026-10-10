---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-09-literal-goldens-cannot-see-verdict-changes.md
---
# beacon's "valid" DKIM test key never parsed as a key

**What failed.** `make_valid_rsa_p_value` in `crates/email/src/checks/dkim.rs`'s tests yields a `p=` whose SPKI bytes do not parse: `check_dkim` reports `key_parse_error` Info for it. The tests that use it assert only that a key was `found`, so they pass whether the key parses or not, and no test ever exercised `rsa_key_ok`.
**What worked.** The DKIM results table (`crates/email/src/checks/dkim_results_table.rs`) asserts every sub-check name and verdict per row; its first run of the valid-key row showed `key_parse_error`. The row now uses a real 2048-bit key from `openssl genrsa 2048`, recording `rsa_key_ok` Pass.
**How to notice next time.** A fixture named "valid" whose test asserts presence, not the verdict the fixture is meant to produce.
