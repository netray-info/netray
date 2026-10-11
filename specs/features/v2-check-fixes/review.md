## f68ccb2..466ac57

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Testing
MAJOR | crates/tls/src/validate/chain_trust.rs:25 | Trust no longer depends on the hostname, so a certificate that does not cover the hostname stops triggering lens's hard-fail F: `tls.chain_trusted` was the only hard-fail check that caught a name mismatch, and `hostname_match` is not on the hard-fail list (crates/engine/profiles/v3.toml:26). | Leaf SAN `*.example.com` inspected as `a.b.example.com`: before, chain_trusted Fail and Grade F; after, only hostname_match fails (0/10) and TLS scores about 90% with no hard fail.
MINOR | crates/http/tests/cors_probe.rs:29 | The C6 test only compares the constant to a literal, so it cannot fail if the probe request stops using the constant. | Change crates/http/src/inspect/mod.rs:97 back to a literal other origin: every test still passes.

```quote crates/tls/src/validate/chain_trust.rs:25
    let (chain_trusted, chain_trust_reason) = verify_chain_trust(roots, raw_certs);
```

```quote crates/engine/profiles/v3.toml:26
hard_fail = ["chain_trusted", "not_expired", "tls_reachable"]
```

```quote crates/http/tests/cors_probe.rs:29
    assert_eq!(PROBE_ORIGIN, "https://evil.example.com");
```

```quote crates/http/src/inspect/mod.rs:97
        Some(cors::PROBE_ORIGIN),
```

### Calibration

| finding | refuter | confidence | held | command |
|---|---|---|---|---|
| MAJOR chain_trust.rs:25 name mismatch no longer hard-fails | CONFIRMED | 8 | held | `sed -n 24,27p crates/engine/profiles/v3.toml`: hard_fail = chain_trusted, not_expired, tls_reachable; `verify_chain_trust(roots, raw_certs)` takes no name |

### Roll call

| id | answer | evidence |
|---|---|---|
| P03 | convergence | new field `ends_at_anchor` and UI copy use product terms; no new *Status enum |
| P10 | convergence | trust, completeness, signature and CAA error branches reach Fail or Warn, never Pass or N/A |
| P12 | absence | no outbound call added |
| P13 | absence | no limiter touched |
| P18 | absence | no client or fetch added |
| P26 | absence | no new check ID |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | `tlsight-inspect.json` regenerated with its source; `checkMeta.ts` guarded by `test_checkmeta_catalogue.sh` |
| P40 | absence | no module added or removed |

### Summary

Before refutation 0/1/1, after 0/1/1; verified 1, held 1. Roll call: 9 answered, 3 convergence, 0 divergence, 6 absence. The MAJOR is repaired as R8 (operator, 2026-10-11): `tls.hostname_match` becomes a hard fail of profile v3.

## 466ac57..e9efa87

### Reader

COUNTS blockers=0 majors=0 minors=4
LENSES Engineering, Testing
MINOR | specs/features/v2-check-fixes/report.md:118 | C38/C39 sit outside the Phase 2 Criteria table, so `adlc next` never reads them; C39's status is not a closed status. | The rows follow `### Behavioural verification`; Phase 2 counts as done from C26–C37 alone.
MINOR | crates/lens/README.md:300 | The hard-failure table omits the hostname mismatch that now forces an F. | A leaf SAN not covering the name: hostname_match Fail, in v3 hard_fail, Grade F; the README names three conditions.
MINOR | site/guide/lens.html:129 | The guide says two TLS conditions force an F; the profile has four. | tls_reachable (already missing) and hostname_match are not listed.
MINOR | specs/features/v2-check-fixes/spec.md:22 | The out-of-scope line says only finding texts change in checks.toml; R8 also changes a severity. | checks.toml tls.hostname_match severity high → critical.

```quote specs/features/v2-check-fixes/report.md:118
| C39 | R8: every hard-fail check is `critical` in the catalogue | already_implemented (guard; held for the three before R8) | crates/engine/tests/profile.rs |
```

```quote crates/lens/README.md:300
These conditions force an **F** regardless of the numeric score:
```

```quote site/guide/lens.html:129
      Two TLS conditions force the grade to <strong>F</strong> regardless of the
```

```quote specs/features/v2-check-fixes/spec.md:22
- Changing weights. Profile v3 changes only by R8 (one hard-fail entry); v3 is unreleased, so its lock hash moves with it. In `checks.toml` only the `finding` texts of `tls.strong_signature` (R4) and `http.cors` (R1) change.
```

### Summary

0/0/4, no BLOCKER or MAJOR to refute or verify; the four MINORs are fixed after this reading.
