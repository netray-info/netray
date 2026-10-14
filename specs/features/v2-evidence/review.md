
## 13fa7dc..99172c7

### Reader

COUNTS blockers=1 majors=1 minors=2
LENSES Engineering, Testing
BLOCKER | crates/email/src/module.rs:287 | `mta_sts_without_tls_rpt` makes `email.tls_rpt` warn, but the check does not reference `email.mta_sts`, the block that decided the warning. | An MX and `v=STSv1` with no `_smtp._tls`: tls_rpt Warn, with evidence `["email.tls_rpt","email.mx"]`.
MAJOR | crates/tls/src/module.rs:388 | TLS gives references to unmeasured checks: `caa_compliant` and `ech_advertised` point at `dns.caa`/`dns.https` after their own lookup failed. | The facts' CAA answers NODATA and TLS's own climb times out: `caa_compliant` is unmeasured with evidence `["dns.caa"]`.
MINOR | crates/lens/src/modules.rs:243 | The DNS headline symbol is chosen from finding text ("Not found"). | A constructed lint `[NotFound, Failed]` shows `–` where the old code showed `✗`.
MINOR | CHANGELOG.md:26 | The CHANGELOG and README say `headline` is derived from the evidence; the DNS, HTTP and email headlines come from the check verdicts. | `dns_headline(&checks)` reads no blocks.

```quote crates/email/src/module.rs:287
    ("mta_sts_without_tls_rpt", &["email.tls_rpt"]),
```

```quote crates/tls/src/module.rs:388
            if let Some(refs) = shared_references(name, &evidence, caa_from_facts) {
```

```quote crates/lens/src/modules.rs:243
                    if c.messages.first().map(String::as_str) == Some("Not found") =>
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| BLOCKER `crates/email/src/module.rs:287` | CONFIRMED | 8 | held | read `CROSS_VALIDATION_INPUTS` and `cross_validation.rs:106` |
| MAJOR `crates/tls/src/module.rs:388` | CONFIRMED | 7 | held | read `translate_with_facts`: `shared_references` overwrites the evidence of every status |

### Summary

- Counts before and after refutation: 1 blocker, 1 major, 2 minors. Verified 2, held 2.
- Both are small. In `CROSS_VALIDATION_INPUTS`, `mta_sts_without_tls_rpt` must name `email.mta_sts`. TLS must leave unmeasured checks without references, as DNS and HTTP do.

## 99172c7..9e5d29b

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

All three fixes traced; no wrong result. The new TLS test fails on the parent commit, and the DNS headline gives the same symbols as before for every reachable mhost 0.12.1 lint outcome.

### Summary

- 0/0/0. The branch reading's blocker and major are fixed (1430a05); both minors are fixed (DNS headline from evidence; prose wording).
