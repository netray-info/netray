# Report: fetch.sh fails closed

## Phase 1 — Fail closed

### Criteria

| Id | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: every download uses `curl -fsS`; an HTTP or transport failure exits non-zero | green | `tests/repo/test_fetch_fail_closed.sh` |
| C2 | R2: every data file is written atomically; a failure leaves no new or partial file and an existing file unchanged | green | `tests/repo/test_fetch_fail_closed.sh` |
| C3 | R3: successful runs produce the same files as before | green | `tests/repo/test_fetch_fail_closed.sh` |
| C4 | Scenario: 404 for the Tor list → non-zero exit, no `tor_exit_nodes.txt` | green | `tests/repo/test_fetch_fail_closed.sh` |
| C5 | Scenario: IPv6 VPN list fails → no `vpn_ranges.txt` | green | `tests/repo/test_fetch_fail_closed.sh` |
| C6 | Scenario: one Spamhaus list fails → no `spamhaus_drop.txt` | green | `tests/repo/test_fetch_fail_closed.sh` |
| C7 | Scenario: existing `regexes.yaml` unchanged when another download fails | green | `tests/repo/test_fetch_fail_closed.sh` |
| C8 | Scenario: all URLs served → exit 0, every data file present and non-empty | already_implemented | `tests/repo/test_fetch_fail_closed.sh` |
| C9 | Scenario: every `curl` invocation carries `-f` | green | `tests/repo/test_fetch_fail_closed.sh` |

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 33,893 | 33 |

### Review

| Class | Finding | Outcome |
|---|---|---|
| AMENDMENT | the EXIT trap removed every run's `*.tmp.*`, so a concurrent run in the same directory could have its half-built `vpn_ranges.txt` deleted and a partial file renamed into place | repaired in phase: cleanup removes only `*.tmp.$$`; test asserts the glob (`1b10b88`) |
| DEFERRED | `-f` refuses only HTTP >= 400; a 3xx or 2xx HTML body still lands in the plain-copy files (no `-L`, no content check) | follow-up |
| DEFERRED | `while read` drops a last CIDR without trailing newline in the Cloudflare, Azure and GPTBot lists (pre-existing) | follow-up |

The reader confirmed against stub binaries: each of the 26 URLs failing in turn exits non-zero and leaves no file or temp behind; success output is byte-identical to before; `clean` and usage paths unchanged.

### Behavioural verification

```
$ bash tests/repo/test_fetch_fail_closed.sh                       # Homebrew bash
PASS: test_fetch_fail_closed
$ PATH=/bin:/usr/bin:<jq> /bin/bash tests/repo/test_fetch_fail_closed.sh   # macOS bash 3.2.57
PASS: test_fetch_fail_closed
```

Live upstream fetch not run: GeoIP needs the deployment's licence key.
