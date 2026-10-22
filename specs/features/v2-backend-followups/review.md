# Review: v2-backend-followups

## 3e8d7e14..5a56ae5f

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result found in 3e8d7e14..5a56ae5f (read only, no build). Traced: the directive splitter
over the spec cases and edge inputs; `unreadable_headers` reads the same first value as
`analyze_header_presence`; `http.isolation` writes exactly one of value, `unreadable` or `missing`;
`coop_verdict` keeps warn; the JSON replay falls back to `missing`; live and shared exports derive
the same port set, names and protocol; one-port TLS runs and other inspectors get no per-port part;
`pending_export_html` equals the old `ready=false` path. Test gap: c7 and c8 in
`crates/web/tests/it/export_tls_ports.rs` check absence of the per-port parts, not byte equality;
byte equality traced in code.

### Spec reader

COUNTS blockers=0 majors=0 minors=1
REQUIREMENTS met=8 partial=0 missed=0 contradicted=0
MINOR | crates/web/src/pages/views/http.rs:300 | The isolation view's new claim rule counts repeats of a single key, so it also stops claiming a block that has the same value key twice (or one header listed twice under `missing`). Before this range the view claimed such a block, and no requirement asks for the change. | Fields `coop=same-origin`, `coop=unsafe-none`, `missing=cross-origin-embedder-policy`, `missing=cross-origin-resource-policy`: claimed at 3e8d7e14, generic block at END. `isolation_block` never writes such a block, so live data cannot reach this path.

```quote crates/web/src/pages/views/http.rs:300
    let contradicts = HEADERS.iter().any(|(key, wire)| {
```

```quote specs/features/v2-backend-followups/spec.md:87
   `missing`, `unreadable` or a value, or an unknown name under `unreadable`, is not claimed.
```

### Refuted

None: no BLOCKER or MAJOR to refute.

### Calibration

None: the refuter was not called.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no new Status/Verdict enums, no codenames; coop_verdict uses the engine Status |
| P10 | convergence | an unreadable COOP yields warn, never pass or N/A |
| P12 | absence | no outbound call or module entry added |
| P13 | absence | no route or limiter change |
| P18 | absence | no reqwest client, no config struct |
| P26 | convergence | no check id added; `http.isolation/unreadable` emitted by http only |
| P35 | absence | no rules spec changed |
| P36 | absence | no generated artefact changed |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/0/0, spec reader 0/0/1 (REQUIREMENTS met=8 partial=0 missed=0
contradicted=0). After refutation: the same, 0 blockers, 0 majors, 1 minor (deferred).
verified 0, held 0. Roll call: answered 9, convergence 3, divergence 0, absence 6.
