# Review: email scoring

## 2c3bb09..5025def

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing, Security

MAJOR | crates/lens/src/backends/email.rs:391 | For a Null MX domain the three N/A email buckets tell the user "No MX records — email receiving not configured", which is false: the domain publishes `MX 0 .`. Verdicts and scores are right; `bucket_na` says "null MX". | Serve `beacon-null-mx.sse`: `no_mx_reason` returns "null MX", `map_buckets` writes the fixed no-MX string into the messages of infrastructure, transport and brand, and the API and `CheckList.tsx` show it.

```quote crates/lens/src/backends/email.rs:391
                let na_msg = "No MX records — email receiving not configured".to_string();
```

```quote crates/lens/src/backends/email.rs:367
    if mx.sub_checks.iter().any(|s| s.name == BEACON_NULL_MX) {
```

Traced sound: beacon's verdict reorder, skip_result events, `sends_no_mail`, `only_dash_all`/`is_modifier`, the DKIM revoked-only override incl. cname_loop rows; lens's timeout guard, the not-run and unrouted errors, the counter, bucket aggregation, the `sends_no_mail` exclusions, Null MX detection, `bucket_na`; authentication always has something to score; every new test fails when its code is reverted; trailers cover every moved pinned file.

### Refuted

None. The MAJOR was CONFIRMED (confidence 9): `map_buckets` wrote the fixed no-MX message for every no-MX case; `build_check_items` (routes.rs:1238) sends bucket messages to the API and `CheckList.tsx` renders them.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| `email.rs:391` Null MX buckets say "No MX records" | CONFIRMED | 9 | held | `cargo test -p lens --test contract_beacon beacon_null_mx` red at `cea9c8e` ("email_infrastructure: Null MX message expected, got [\"No MX records — email receiving not configured\"]") |

Repaired on this branch: `cea9c8e` (test), the following `fix(lens)` commit: `map_buckets` takes the no-MX reason; Null MX buckets read "Null MX (RFC 7505) — domain does not accept mail".

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | convergence | the new beacon-named error strings stay in `SectionError::BackendError`, which the API shows as "backend error"; no new status enum |
| P10 | divergence | errors, timeouts, not-run and unknown names lead to Timeout/Errored → incomplete; but a bucket with only Info or absent categories becomes Skip "not applicable" and leaves earned and possible points, which the principle's sentence (N/A only for no MX and Null MX) does not name. SDD R4.3 decides it (absent BIMI is not a failure); the sentence of P10 should name bucket-level N/A, or R4.3 should be revisited — for the operator |
| P12 | absence | no new outbound call |
| P13 | absence | no route or limiter change |
| P18 | absence | no new client or config struct |
| P26 | convergence | `sends_no_mail` computed once in beacon, read by lens; lens's copies of beacon's names are pinned against beacon's exports |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | goldens change only with their generators |
| P40 | convergence | lens depends on beacon only as a dev-dependency |

### Summary

Before refutation 0/1/0, after 0/1/0. verified 1, held 1 (repaired on the branch). Roll call: 9 answered, 4 convergence, 1 divergence, 4 absence.
