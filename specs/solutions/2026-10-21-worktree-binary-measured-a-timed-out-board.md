---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-19-fixture-export-hid-the-blob-iframes.md
---
# The design check measured a timed-out TLS run and passed

**What failed.** `just design-check` builds `netray` in the worktree and starts lens from that path. On the operator's Mac a binary at a new path gets no outbound network (Little Snitch), so board G's live target `1.1.1.1:443` rendered "handshake timed out": no addresses table, no certificate, no protocols grid. The check still exited 0, because the manifest measured only the shell's elements, which render either way (v2-tls-ui, `target/design/d3-tls-inspector-light-page.png` before the fix).
**What worked.** `netray_bin` honours `NETRAY_BIN` (`tests/repo/lib/netray.sh`); the branch's build is copied to the main checkout's `target/debug/netray`, the one path allowed out, and the check runs with `NETRAY_BIN` pointing there, restoring main's binary afterwards. The run then completed and the new certificate, addresses and protocols rows measured real markup.
**How to notice next time.** A live inspector board's page image shows a timeout or an empty result while the check reports 0 open deviations.
**Check to build.** design-check refuses a board whose `wait` selector is reached with no measured element of the board present beyond the shell (a `no state` row), in `tests/acceptance/design/check.mjs`.
