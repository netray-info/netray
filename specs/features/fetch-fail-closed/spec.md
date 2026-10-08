# Spec: fetch.sh fails closed

Status: Active
Created: 2026-10-08

`crates/ifconfig-rs/data/fetch.sh` fetches ifconfig-rs's data files; the deployment vendors it at the release tag and runs it from a host timer. It calls `curl -s` without `-f`, so an HTTP error page is saved as a data file, and `> file` redirects truncate the target before the download, so a failed second or third download leaves a half file. Because the script skips files that already exist, a bad file stays until someone deletes it.

## Requirements

1. Every download uses `curl -fsS`; an HTTP error or transport failure makes `fetch.sh` exit non-zero.
2. Every data file is written atomically: produced under a temporary name in the same directory and renamed into place only when every input of that file succeeded. A failure leaves no new or partial file behind and leaves an existing file unchanged.
3. Successful runs produce the same files as before.

## Phase 1 — Fail closed

**Depends on:** none
**Requirements:** 1, 2, 3

### Test Scenarios

- GIVEN a `curl` that answers HTTP 404 for the Tor exit list WHEN `fetch.sh get_all` runs THEN it exits non-zero and `tor_exit_nodes.txt` does not exist.
- GIVEN a `curl` that fails only the IPv6 VPN list WHEN `fetch.sh` runs THEN `vpn_ranges.txt` does not exist (no half file).
- GIVEN a `curl` that fails one of the three Spamhaus lists WHEN `fetch.sh` runs THEN `spamhaus_drop.txt` does not exist.
- GIVEN an existing `regexes.yaml` and a failing download for another file WHEN `fetch.sh` runs THEN `regexes.yaml` is unchanged.
- GIVEN a `curl` that serves every URL WHEN `fetch.sh get_all` runs THEN it exits 0 and every data file exists and is non-empty.
- GIVEN the script WHEN read THEN every `curl` invocation carries `-f`.

## Open decisions

None.
