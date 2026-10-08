---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-strict-scan-and-grep-passed-without-observing.md
---
# The `/docs` CSP check matched the host as text, not as a CSP source

**What failed.** `tests/repo/test_header_parity.sh` accepted any `/docs` CSP containing `https://cdn.jsdelivr.net`. beacon's source `https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.44.25/` contains that text, but a source ending in `/` is a path prefix and does not match the page's script `…/api-reference@1.44.25`; the reader showed in headless Chromium that Scalar would render blank once Traefik stops overriding the CSP.
**What worked.** The test extracts every jsDelivr `<script src>` from the docs page and checks it against `script-src` with the browser's source-matching rule (host-only, `/`-terminated prefix, or exact URL); beacon's source lost its slash (commits `16dee7f`, `6915288`).
**How to notice next time.** A check on a policy that greps for a substring instead of evaluating the policy the way its consumer does passes values the consumer rejects.
