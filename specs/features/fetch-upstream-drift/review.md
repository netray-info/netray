# Review: fetch.sh upstream drift

Context: argus-oci reported that `fetch.sh` at v0.22.0 fails on its host (2026-10-08): `openai.com/gptbot-ranges.txt` answers 403 (replaced by `gptbot.json`), Google's `googlebot.json` answers 301 to `common-crawlers.json`. Every other upstream URL answered 200 when probed. Fix: `c3fb929` (new URLs, jq for gptbot, `-L` in `get()`), `<this range>` (`-L` on every curl).

## main..c3fb929

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing
MAJOR | crates/ifconfig-rs/data/fetch.sh:107 | Only `get()` followed redirects; the five direct curl pipelines (vpn 47-48, spamhaus 107-109, cins 117) still wrote a 3xx body into the data file with exit 0. | Stub with a 301-without-`-L` rule for spamhaus drop.txt, vpn ipv4.txt and cinsscore: `spamhaus_drop.txt` started with `<html><head><title>301 Moved Permanently`, the IPv4 VPN list was lost, `cins_army_ips.txt` held only the HTML line.
MINOR | tests/repo/test_fetch_fail_closed.sh:74 | No test failed when `-L` was removed (the stub's redirect branch matched only the old googlebot URL). | `curl -fsS` in `get()` still gave PASS.

Traced and sound: both new URLs return 200 JSON with `.prefixes[]` (gptbot 18 ipv4, common-crawlers 317 mixed); the jq filters label providers correctly and every CIDR is canonical for `bot.rs`'s strict parser.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 direct curl pipelines without `-L` | not run (orchestrator reproduced via the new C14 scenario) | — | held | `tests/repo/test_fetch_fail_closed.sh` C14 failed for vpn, spamhaus, cins before the fix (`8cdb97b`), passes after |

### Summary

0 / 1 / 1. Verified 1, held 1. Both repaired: C14 (every URL redirected, output byte-identical to the plain run) and C9 (`-L` on every curl), and `-L --max-redirs 5` on every curl.
