---
class: neighbour-artefact-left-inconsistent
repeat-of: specs/solutions/2026-10-11-golden-fixture-contradicts-its-producer.md
---
# Narrowing a check dropped the hard fail it carried

**What failed.** v2-check-fixes R5 took the hostname out of `tls.chain_trusted` (trust via `verify_server_cert_signed_by_trust_anchor`, `crates/tls/src/validate/chain_trust.rs`). The check was right, but `chain_trusted` had been the only hard fail a name mismatch reached; `tls.hostname_match` was weighted 10 and not in `crates/engine/profiles/v3.toml` `hard_fail`. A publicly trusted certificate that does not cover the host went from grade F to about 90 % TLS. Spec, two phase readers and the gate passed it; the branch reader found it.
**What worked.** R8: `hard_fail = ["chain_trusted", "hostname_match", "not_expired", "tls_reachable"]`, the v3 lock hash refreshed (v3 unreleased), `tls.hostname_match` severity `critical`, and the guard test `every_hard_fail_check_is_critical_in_the_catalogue` in `crates/engine/tests/profile.rs`.
**How to notice next time.** A change that removes a Fail branch from a hard-fail check: list which conditions reached that branch and find each one's new path to the grade.
