## a757312..d57d7a1

### Reader

COUNTS blockers=2 majors=2 minors=0
LENSES Engineering, Testing
BLOCKER | CHANGELOG.md:36 | The DNSBL known issue says N/A only when every zone refuses; one policy answer or timeout without a listing is enough. | Zen returns 127.255.255.254, the other zones NXDOMAIN: sub_checks hold one Info, the `clean` Pass is never added, the check is N/A.
BLOCKER | CHANGELOG.md:25 | The CORS line says only an echo or `null` is flagged; `*` still warns and an echo without credentials passes. | ACAO `*` without credentials: Warn; ACAO the probe origin without ACAC: Pass.
MAJOR | CHANGELOG.md:15 | "`hygiene`, which 0.24.0 never scored" is false: it counted when a part warned or failed, and was N/A only when all passed. | No Cache-Control at 0.24.0: hygiene Warn, earned 1 of 2.
MAJOR | CHANGELOG.md:18 | "no MX" is listed as a TLS not_applicable reason; the TLS module never emits it. | crates/tls/src/module.rs falls back to "no TLS handshake" or "not reported" only.

```quote CHANGELOG.md:36
- **`email.dnsbl` is not applicable when every blocklist zone refuses the query**, instead of being reported as not measured.
```

```quote crates/email/src/checks/dnsbl.rs:144
    if sub_checks.is_empty() {
```

```quote CHANGELOG.md:25
- **`http.cors`** flags only an `Access-Control-Allow-Origin` that echoes the probe's `Origin` exactly, or `null`; a site that names its own trusted origin passes.
```

```quote crates/http/src/inspect/cors.rs:35
    } else if allows_any_origin {
```

```quote crates/http/src/inspect/cors.rs:24
    let (status, message) = if reflects_origin && allows_credentials {
```

```quote CHANGELOG.md:15
  - HTTP: `csp`, `x_frame_options`, `x_content_type_options`, `referrer_policy` and `permissions_policy` replace `security_headers`; `deprecated_headers`, `info_leakage` and `caching` replace `hygiene`, which 0.24.0 never scored.
```

```quote CHANGELOG.md:18
  - TLS reports all 21 checks in every run, measured over every address that completed a handshake; the worst result counts and names the address. A check that does not apply is `not_applicable` with its reason ("no TLS handshake", "no MX", "not reported").
```

```quote crates/tls/src/module.rs:260
        "no TLS handshake"
```

### Calibration

No refuter ran: the four findings are prose claims against code the orchestrator opened directly (`crates/http/src/inspect/cors.rs:24-40`, `crates/email/src/checks/dnsbl.rs:120-150`, `crates/tls/src/module.rs` holds no "MX", and v2-catalogue's context records that `hygiene` read `skip` only on a healthy site). All four held.

### Summary

2/2/0, verified 4, held 4. Fixed on this branch: the notes are prose, the fix is the wording.

## a757312..eeb9eae

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | CHANGELOG.md:18 | The notes say all 21 TLS checks are measured over every handshake address, worst counting; `tls.ocsp_stapled` and `tls.caa_compliant` (and `tls.dane_valid`) come once from the summary. | Two addresses, only the first staples OCSP: lens reports pass, not the worst.

```quote CHANGELOG.md:18
  - TLS reports all 21 checks in every run, measured over every address that completed a handshake; the worst result counts and names the address. A check that does not apply is `not_applicable` with its reason, such as "no TLS handshake" in TLS, "no MX" in email or "not reported" where a module had no result.
```

```quote crates/tls/src/module.rs:199
/// worst of them; CAA, DANE and OCSP come once from the summary.
```

### Summary

1/0/0, verified 1, held 1 (module.rs:199 states the exception). No refuter: the orchestrator opened the passage. Fixed on this branch.

## eeb9eae..79f4060

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | CHANGELOG.md:18 | Only three checks are named as once per run; `tls_reachable`, `consistency` and `alpn_consistency` are also judged once per run, and not by worst-of. | One of two addresses completes the handshake: `tls_reachable` passes.

```quote CHANGELOG.md:18
  - TLS reports all 21 checks in every run. The per-address checks are measured on every address that completed a handshake; the worst result counts and names the address. `caa_compliant`, `dane_valid` and `ocsp_stapled` are measured once per run. A check that does not apply is `not_applicable` with its reason, such as "no TLS handshake" in TLS, "no MX" in email or "not reported" where a module had no result.
```

```quote crates/tls/src/module.rs:210
    let reachable = checks::check_tls_reachable(ips);
```

```quote crates/tls/src/module.rs:249
            checks::check_consistency(port.consistency.as_ref()),
```

### Summary

1/0/0, verified 1, held 1 (module.rs:210-251). No refuter: the orchestrator opened the passage. Fixed on this branch by naming the six once-per-run checks from the code.

## 79f4060..d8b8f80

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering

The rewritten TLS line matches `translate`: 15 per-address checks, 6 judged once per run, 21 in all.

### Summary

0/0/0.
