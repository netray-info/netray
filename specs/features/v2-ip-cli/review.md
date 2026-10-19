# Review: v2-ip-cli

## 5ae774d2..8abe47e0

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing

MAJOR | crates/web/src/api/ip.rs:614 | A request that fails because the field has no form in the asked format still costs a per-IP token and a per-target token, and it runs the full lookup (reverse DNS on by default) before it gets its 400. The decision log says "validation runs before any charge" (spec.md:271), so this ordering contradicts it. | Send `GET /api/v1/ip/198.51.100.1?field=country&format=toml` from 192.0.2.10 with the default `[api.ip]`. In `answer_address`, `Params::new` accepts `toml`, then `charge_caller` (line 614) and the per-target charge both run, and `ip.lookup_with` (line 621) issues the PTR lookup. Only then does `respond` find that a bare string has no TOML form (lines 362-363) and answer `400 INVALID_INPUT`, carrying `X-RateLimit-Remaining: 9`. Ten such requests use up the per-IP burst of 10, so the caller's next valid lookup gets 429. The same ordering applies to `/api/v1/ip?field=country&format=toml` (answer_caller) and to `ip_host`'s `/country/toml`. For scalar fields (`country`, `city`, `region`, `timezone`, `latitude`, `longitude`, `asn`) the answer is known without a lookup, so the check could run before the charge.

```quote crates/web/src/api/ip.rs:614
    charge_caller(state, headers, peer, 1)?;
```

```quote crates/web/src/api/ip.rs:621
    let ifconfig = ip.lookup_with(target, params.options.clone()).await?;
```

```quote crates/web/src/api/ip.rs:362
            let body = format.serialize_body(&value).ok_or_else(|| {
```

```quote crates/web/src/api/ip.rs:363
                V1Error::InvalidInput(format!("{} has no {name} form", params.label))
```

```quote specs/features/v2-ip-cli/spec.md:271
- Phase 1 review: a 503 charges no limiter, validation runs before any charge, and the answers that
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=15 partial=0 missed=0 contradicted=0

No findings. Requirements 1-15 traced through the code at 8abe47e0. Two candidates did not hold:
Accept with several types under `/api/v1` answers the first recognised type (consistent with the
decision log), and the OpenAPI fixture's removed enum lines are a reorder, no value lost.

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| MAJOR ip.rs:614 field-form check after charge and lookup | CONFIRMED | 8 | held | read `answer_address`/`respond` at 8abe47e0: `Params::new` accepts `toml` for a scalar field, `charge_caller`, the per-target charge and `lookup_with` run before `serialize_body` refuses |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no new Status/Verdict enum; no codename in API output |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no new outbound client; reverse DNS toggles the existing enrich path |
| P13 | convergence | the ip routes and ip_host charge the separate IpLimiters set |
| P18 | absence | no new fetch, no new config struct |
| P26 | absence | no new check ID |
| P35 | absence | no rule changed |
| P36 | convergence | the OpenAPI fixture changes with its utoipa source |
| P40 | convergence | engine untouched; the ip change is a trait extension in its own crate |

### Summary

Before refutation: reader 0/1/0, spec reader 0/0/0 (`REQUIREMENTS met=15 partial=0 missed=0
contradicted=0`). After refutation: reader 0/1/0, spec reader 0/0/0. verified 1, held 1. Roll call:
9 answered, 4 convergence, 0 divergence, 5 absence. The MAJOR is fixed on this branch after the
reading (validation of field against format before any charge), with its own test and reading.

## 8abe47e0..1ae2ed46

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing

MAJOR | crates/web/src/api/ip.rs:344 | The new check that runs before any charge misses `user_agent`. When the caller sends no usable User-Agent header, that field has no TOML form either, so the request is still charged on both limiters and looked up before it gets its 400. | `GET /api/v1/ip?field=user_agent&format=toml` with no User-Agent header, or `/user_agent/toml` on the `ip.` host: `handlers::user_agent::to_json` gives `Some(Value::Null)`, TOML refuses a root null after the per-IP and per-target charges; from the 21st such request the caller gets 429 where it should get 400.

```quote crates/web/src/api/ip.rs:344
            && TOML_LESS_FIELDS.contains(&params.label.as_str())
```

```quote crates/web/src/api/ip.rs:708
    charge_caller(state, headers, peer, 1)?;
```

```quote crates/ip/src/handlers.rs:239
        serde_json::to_value(&ifconfig.user_agent).ok()
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=1 partial=0 missed=0 contradicted=0

No findings: requirement 5's scalar-as-TOML clause and the decision log's "validation runs before
any charge" hold on all three entry points for the seven listed fields.

```quote specs/features/v2-ip-cli/spec.md:89
   An unknown format answers 400. A field with no form in the asked format (a scalar as TOML) answers
```

```quote crates/web/src/api/ip.rs:342
        // A scalar field has no TOML form; refused here, before any charge or lookup.
```

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| MAJOR ip.rs:344 user_agent without a User-Agent header as TOML charged before its 400 | CONFIRMED | 8 | held | traced at 1ae2ed46: `TOML_LESS_FIELDS` lacks `user_agent`; `handlers::user_agent::to_json` gives `Some(Null)`; `serialize_body(Toml)` refuses a root null after `charge_caller` |

### Roll call

Not repeated for a two-file fix range; the reading of 5ae774d2..8abe47e0 holds it.

### Summary

Before and after refutation: reader 0/1/0, spec reader 0/0/0 (`REQUIREMENTS met=1 partial=0
missed=0 contradicted=0`). verified 1, held 1. The MAJOR is fixed after this reading with its own
test and reading.

## 1ae2ed46..bb570ec6

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result. Traced `GET /api/v1/ip?field=user_agent&format=toml` and `/user_agent/toml` on
`ip_host` with no User-Agent, an empty one, a non-ASCII one, two headers, and an Accept-negotiated
TOML: the guard uses the same header expression `answer_caller` passes to the lookup, so it
refuses exactly the requests whose answer was already a 400 after both charges; `?ip=` and
`/ip/{address}` refuse `user_agent` earlier; the test fails at 1ae2ed46 (429 on request 11).

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=1 partial=0 missed=0 contradicted=0

No findings: requirement 5 and "validation runs before any charge" hold for `user_agent` without an
agent.

### Summary

Before and after refutation: reader 0/0/0, spec reader 0/0/0 (`REQUIREMENTS met=1`). verified 0,
held 0. No anchors to check (all counts zero).
