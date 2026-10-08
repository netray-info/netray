# Review: fetch.sh fails closed

## main..e2f539c

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing
MAJOR | crates/ifconfig-rs/data/fetch.sh:12 | On every exit, including a usage error, the new EXIT-trap `cleanup` deletes the shared cloud and bot intermediates (`.aws.json` … `.as_metadata.json`), not just this run's `*.tmp.$$` files as the comment at lines 8-9 says. So a run sharing the same directory loses its inputs: it exits non-zero, and the six data files that come after are not written, even though all of its own downloads succeeded. | Reproduced with the test's stub curl in a temp dir. Run A `fetch.sh get_all` is held in the `.azure.txt` download for 3 s. One second in, a second `fetch.sh get-all` (a typo, so exit 2 from usage) or a `get_all` whose geoipupdate fails (exit 1) runs in the same directory. A then fails with `jq: error: Could not open file .aws.json` (rc=2), leaving no `cloud_provider_ranges.jsonl`, `datacenter_ranges.txt`, `bot_ranges.jsonl`, `spamhaus_drop.txt`, `cins_army_ips.txt` or `as_metadata.jsonl`. The same sequence with main's fetch.sh gives A rc=0 and every file. The concurrency check in the test (lines 144-147) only greps the glob text, so it passes despite this.
MINOR | tests/repo/test_fetch_fail_closed.sh:103 | The test, which report.md cites for C2 ("every data file is written atomically"), still passes when atomic writes are removed from 5 of the 7 writers. | Each of these mutations, applied alone, still gives `PASS: test_fetch_fail_closed`: (1) `get` changed to `curl -fsS "$1" -o "$2"`; (2)-(5) `cins_army_ips`, `cloud_provider_ranges`, `bot_ranges` and `as_metadata` writing straight to the final name. With `BAD_URLS=oracle.com` (HTTP 200 with an HTML body), the `cloud_provider_ranges` mutant leaves a 7-line partial `cloud_provider_ranges.jsonl` while the END version leaves none.

```quote crates/ifconfig-rs/data/fetch.sh:12
        .aws.json .gcp.json .cloudflare-v4.txt .cloudflare-v6.txt .oracle.json .fastly.json \
```

```quote tests/repo/test_fetch_fail_closed.sh:103
l=$(leftovers "$d"); [ -z "$l" ] || fail "C2: leftovers after Tor failure: $(echo $l)"
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| F1 cleanup deletes shared intermediates of a concurrent run | CONFIRMED (mechanism; severity arguable: overlap needs a manual run during the weekly cron or argus's timer; main had a narrower form) | 7 | held | the reader's stub repro; `tests/repo/test_fetch_fail_closed.sh` C12 now fails on it (`.aws.json` removed) |

### Summary

Before refutation: 0 / 1 / 1. After: 0 / 1 / 1. Verified 1, held 1. No roll call. Both repaired before finish: intermediates move to a per-run directory; the test gains partial-write (C10, C11) and concurrency (C12) scenarios, each checked against the reader's mutants.

## e2f539c..ffbf03c

### Reader

COUNTS blockers=0 majors=0 minors=4
LENSES Engineering, Security, Testing

- MINOR: a `.fetch.<pid>` left by a killed run blocked a later run with the same PID (`mkdir` fails) — repaired: the run replaces its own-PID directory; test C13.
- MINOR: `.fetch.<pid>/` was not git-ignored — repaired: `.gitignore` gains `.fetch.*/` and `*.tmp.*`; test C13.
- MINOR: C12 plants another run's files only under `.fetch.99999`, so a mutant sharing one `.fetch` directory passes — listed, not repaired.
- MINOR: the "foreign `.aws.json` consumed" assertion cannot fail (the planted file is not JSON) — listed, not repaired.

Success path verified byte-identical across main, e2f539c and ffbf03c (12 data files, `cmp`); three concurrent runs in one directory: the good runs exit 0 with reference output, the failing one exits 1.

### Summary

0 / 0 / 4. Verified 0, held 0. No roll call. Two minors repaired (`1379981`, `84e1702`).
