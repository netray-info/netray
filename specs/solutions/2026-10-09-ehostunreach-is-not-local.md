---
class: guard-misclassifies-lookalike-input
repeat-of: specs/solutions/2026-10-09-caa-tag-case-lost-in-hickory-bump.md
---
# EHOSTUNREACH read as "not tested from here" hid a target-side refusal

**What failed.** SDD R5.3 and the spec listed `ENETUNREACH`, `EHOSTUNREACH` and `EADDRNOTAVAIL` as local errors raised before any packet reaches the target. Linux also returns `EHOSTUNREACH` in SYN_SENT for an incoming ICMP host-unreachable, host-prohibited or packet-filtered (`icmp_err_convert`, `tcp_v4_err`). With it mapped to `NOT_TESTED_FROM_HERE`, an HTTPS-less host behind firewalld's default reject got `tls_reachable` Skip, so lens graded it `incomplete` instead of F.
**What worked.** The phase reader traced the kernel path; `EHOSTUNREACH` now stays `HANDSHAKE_FAILED` (`crates/tlsight/src/tls/mod.rs` `error_code`, test `target_side_errors_stay_handshake_failed`); only `ENETUNREACH` and `EADDRNOTAVAIL` are not tested from here.
**How to notice next time.** An errno named "local" that the kernel also produces from an ICMP message sent by the remote side.
