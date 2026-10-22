## c3bc3980..2f41ed50

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/tls/src/module.rs:807 | `expired` is now worked out against `Utc::now()`, while the fixture leaf's `not_after` is the fixed 2030-04-01T00:00:00Z. So the committed goldens and the cert-detail test will flip from `expired: false` to `true` on that date and fail with no code change. | Run `cargo test -p lens --test lens_full_output` or `cargo test -p netray-tls --test evidence_cert_detail` on or after 2030-04-01: all 7 `lens-full-*.json` goldens and `evidence_cert_detail.rs:73` assert `false`.
MINOR | crates/tls/tests/translate_ports.rs:207 | `dane_and_a_tlsa_error_are_judged_on_each_port` still passes if the per-port DANE verdict in `port_wide` (module.rs:543) is replaced by the summary's `dane_valid`. Per-port CAA has no test at all, because `CaaInfo.lookup` is serde-skipped. | Change `port_wide` to return `resp.summary.checks.dane_valid`: the test sets the summary to `pass`, so 443 stays Pass and 465 stays Unmeasured; the test stays green.

```quote crates/tls/src/module.rs:807
            .is_ok_and(|t| t < Utc::now());
```

```quote tests/fixtures/contracts/tlsight-inspect.json:31
              "not_after": "2030-04-01T00:00:00Z",
```

```quote tests/fixtures/contracts/lens-full-healthy.json:2008
                "expired",
```

```quote crates/tls/tests/evidence_cert_detail.rs:73
    assert_eq!(get(&leaf, "expired"), Some(&Value::Bool(false)));
```

```quote crates/tls/tests/translate_ports.rs:207
    assert_eq!(status(&outcomes[0].1, "dane_valid"), Status::Pass);
```

```quote crates/tls/tests/translate_ports.rs:199
    doc["summary"]["checks"]["dane_valid"] = json!("pass");
```

```quote crates/tls/src/module.rs:543
    let dane = match port.tlsa.as_ref().and_then(|t| t.dane_valid) {
```

### Spec reader

COUNTS blockers=0 majors=0 minors=1
REQUIREMENTS met=12 partial=0 missed=0 contradicted=0
MINOR | crates/tls/src/inspector.rs:24 | Making `port` and `ports` `Option` fields with `#[serde(default)]` means an explicit JSON `null` now deserialises as absent. Before the range, `"port": null` was refused with 400. Now it inspects port 443, and a body that names both keys still gets 200 when one of them is `null`. No requirement asks for this. | Req 11: `{"host":"example.com","port":null,"ports":[465]}` gives `(None, Some([465]))` and inspects 465 with 200, although the body names both `port` and `ports`. `{"host":"example.com","port":null}` inspects 443 where the old `u16` field refused it.

```quote crates/tls/src/inspector.rs:21
    /// The port; 443 by default.
    #[serde(default)]
    #[schema(default = 443, value_type = u16)]
    pub port: Option<u16>,
    /// Several ports instead of `port`, at most `limits.max_ports`, each inspected once.
    #[serde(default)]
    #[schema(value_type = Vec<u16>)]
    pub ports: Option<Vec<u16>>,
```

```quote crates/tls/src/inspector.rs:105
        let ports = match (req.port, &req.ports) {
            (Some(_), Some(_)) => {
                return Err(
                    AppError::InvalidPort("give port or ports, not both".to_string()).into(),
                );
            }
```

```quote specs/features/v2-tls-backend/spec.md:102
    A body with both `port` and `ports`, an empty `ports`, more than `max_ports` ports or a port 0
    is refused with 400 and the API's error format. A body with neither inspects port 443 as today.
```

Not flagged because the spec's words allow it: the API deduplicates before its `max_ports` check while the host-string parser counts duplicates; a port with an empty TLSA answer stays unmeasured when another port's TLSA lookup failed.

### Refuted

None: no BLOCKER or MAJOR finding to refute.

### Calibration

None: the refuter was not called.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename in UI copy or API output; one code comment |
| P10 | convergence | live OCSP keys only when the responder answered; per-port OCSP, CAA, DANE keep the Skip and error paths |
| P12 | absence | no new outbound client or module entry |
| P13 | absence | limiter setup untouched |
| P18 | convergence | no new reqwest client; `test_config_strict.sh` passes; request keeps `deny_unknown_fields` |
| P26 | convergence | no new check id; new blocks emitted by the tls module only |
| P35 | absence | no rule change |
| P36 | convergence | OpenAPI and lens goldens change with their sources |
| P40 | convergence | engine and model untouched |

### Summary

- Before refutation: reader 0/0/2, spec reader 0/0/1 (REQUIREMENTS met=12 partial=0 missed=0 contradicted=0). After refutation: the same, 0 blockers, 0 majors, 3 minors.
- verified 0, held 0.
- Roll call: answered 9, convergence 6, divergence 0, absence 3.
