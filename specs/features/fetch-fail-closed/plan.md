# Plan: fetch.sh fails closed

## Phase 1 — Fail closed

### Groups

| Group | Criteria | Depends on |
|---|---|---|
| G1 | C1–C9 | — |

### Plan

**G1.** `crates/ifconfig-rs/data/fetch.sh`:
- `get` uses `curl -fsS` and writes to `<file>.tmp.$$`, then `mv` on success.
- `vpn_ranges`, `spamhaus_drop` and `cins_army_ips` build their file in a temporary file from `curl -fsS` calls (the pipelines already run under `pipefail`) and `mv` it into place.
- The jq/awk-built `cloud_provider_ranges.jsonl`, `bot_ranges.jsonl` and `as_metadata.jsonl` are assembled in a temporary file and renamed into place.
- An `EXIT` trap removes temporary files left by a failure.
