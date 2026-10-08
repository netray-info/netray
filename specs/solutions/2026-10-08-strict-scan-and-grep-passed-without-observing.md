---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-text-checks-pass-without-observing.md
---
# A struct scan and an error grep passed without seeing their subject

**What failed.** `tests/repo/test_config_strict.sh` fed its Python scan through a heredoc; where the shell could not create the heredoc temp file, Python read empty stdin, found no offenders and the test printed PASS. Separately, the OTLP rows in `tests/repo/test_check_config.sh` grepped for `otlp_endpoint`, which serde's unknown-field message also lists, so a row could pass on the wrong error.
**What worked.** The scan prints `scanned <n>` first and the test fails unless `n > 0`; the grep demands `telemetry.otlp_endpoint`, which only the validate message contains (commits `85079c1`, the OTLP grep tightening after `d2c4218`).
**How to notice next time.** Ask what the check prints when its input is empty or its failure comes from somewhere else; if that is PASS, it is not observing.
