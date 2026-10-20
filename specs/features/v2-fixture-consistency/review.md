
## f22cbe6e..06750cd2

### Reader

COUNTS blockers=2 majors=0 minors=2
LENSES Engineering, Testing
BLOCKER | crates/email/src/testing.rs:615 | The email rebuild passes each `email.dkim` row's selector to the module as a user-supplied selector. As a result all seven committed fixtures carry a `mail` DKIM key and an `email.dkim` status that lens cannot produce, because history only records selector-less runs (crates/lens/src/history.rs:115) and those take an empty selector list (crates/email/src/module.rs:218), so they probe only `default` and provider selectors. | Executed: `check_dkim_measured("shop.example.com", ["mx1.shop.example.com"], [], 5, resolver with a key at mail._domainkey.shop.example.com)` returns `found=false keys=[]`. The same call with `["mail"]`, which is what from_evidence does, finds the key. The shop story's DKIM fail-to-pass step and every fixture's `email.dkim` pass rest on a selector a recorded run never queries. The guard itself calls lens with `dkim_selectors: None`, which contradicts the outcome it feeds in.
BLOCKER | crates/tls/src/testing.rs:441 | `validation_of` replaces the `ends_at_anchor` that `validate_chain` worked out from the chain with `anchored`, and `anchored` requires at least two certificates. A lone self-signed leaf therefore gets `tls.chain_complete` fail with "an intermediate is missing", where the module returns pass. Requirement 2 promises the module's own assessment, and the spec puts this case in scope. | Executed: new.example.com's `tls.*` blocks with every chain cut to one item (`position` leaf_self_signed, `self_signed` true, subject = issuer). Rebuild: `tls.chain_complete Fail ["192.0.2.10: chain does not reach a trust anchor; an intermediate is missing", ...]` and `tls.aia_reachability Pass`. Production `validate_chain` on the same CertInfo gives `ends_at_anchor=true` (chain_trust.rs:90), so `check_chain_complete` returns `Pass "chain ends at a self-signed certificate"` and AIA becomes not applicable. Any future self-signed fixture regenerated with `UPDATE_FIXTURES=1` would store the wrong status, and the guard would then protect it.
MINOR | crates/lens/tests/it/history_fixtures_consistent.rs:163 | The guard does not pass an incomplete section's recorded reason through as requirement 7 says. It feeds the stored string back in as the module's reason, lens wraps it a second time, and a consistent incomplete fixture fails the guard (`UPDATE_FIXTURES=1` would nest it again on every run). | Executed: a module answering `Incomplete { reason: "busy" }` is stored as `BackendError("busy")` (history.rs:82, modules.rs:497). The guard's recompute gives `BackendError("BackendError(\"busy\")")`, so `compare` fails on `sections`. Fixtures with incomplete sections are declared out of scope in the spec.
MINOR | specs/features/v2-fixture-consistency/spec.md:50 | No committed fixture carries the `dns.rrsig` block the constraint requires. The rebuild therefore treats every fixture's RRSIG lookup as failed rather than a definite NODATA, and the DNS tests insert the empty block themselves (fixture_evidence_dns.rs:44), so they test an input no fixture contains. | shop.example.com-3.json has no `dns.rrsig` block. `evidence_lookups` returns `Lookups::empty()`, which gives the batch error "RRSIG lookup failed: no resolver answered". The stored `dns.dnssec` evidence is `["dns.dnskey","dns.ds","dns.nsec","dns.nsec3"]`; with the spec's empty block the run would carry `dns.rrsig` and `dns.dnssec` would reference it. No status or grade changes.

```quote crates/email/src/testing.rs:615
        .take(config.dkim.max_user_selectors)
```

```quote crates/email/src/module.rs:218
            let selectors = ctx.options.dkim_selectors.clone().unwrap_or_default();
```

```quote crates/lens/src/history.rs:115
    if dkim_selectors {
```

```quote crates/tls/src/testing.rs:441
    v.ends_at_anchor = anchored;
```

```quote crates/tls/src/validate/chain_trust.rs:90
    if chain.iter().any(|c| c.is_self_signed) {
```

```quote crates/lens/tests/it/history_fixtures_consistent.rs:163
            SectionKind::Incomplete => SectionOutcome::Incomplete { reason },
```

```quote crates/lens/src/history.rs:82
                (SectionKind::Incomplete, Some(format!("{e:?}")))
```

```quote crates/lens/src/modules.rs:497
                Err(SectionError::BackendError(reason))
```

```quote specs/features/v2-fixture-consistency/spec.md:50
  RRSIG records (an empty `dns.rrsig` block, a definite NODATA).
```

```quote crates/lens/tests/it/fixture_evidence_dns.rs:44
    if !blocks.iter().any(|b| b["id"] == "dns.rrsig") {
```

### Spec reader

COUNTS blockers=0 majors=0 minors=2
REQUIREMENTS met=10 partial=0 missed=0 contradicted=0
MINOR | crates/dns/src/api/check.rs:58 | Production files get three visibility changes (`CHECK_RECORD_TYPES`, `unique_records`, `tls::routes::compute_consistency` to `pub(crate)`) beyond the single TLS clock seam the Non-goals call "the one production edit". Behaviour does not change. | Non-goals, spec lines 21-24. The diff also touches crates/dns/src/api/check.rs:845 and crates/tls/src/routes.rs:1162 so the testing code can reuse them. Rated MINOR, not MAJOR, because nothing observable changes: no behaviour, no public API.
MINOR | tests/acceptance/fixtures/history/new.example.com.json:57 | No fixture holds a `dns.rrsig` block. Under R3 a missing block is a failed lookup, so every fixture records its RRSIG batch as unanswered, not as the definite NODATA the constraint asks for. | Constraint at spec lines 47-50, not a numbered requirement. new.example.com's `dns.dnssec` passes with evidence `dns.dnskey, dns.ds, dns.nsec, dns.nsec3` and no `dns.rrsig`, the same in all seven fixtures. No status moves, because RRSIG is an evidence-only input (`EVIDENCE_ONLY_INPUTS`, crates/dns/src/module.rs:704). The fixtures already lacked this block before the range.

```quote specs/features/v2-fixture-consistency/spec.md:21
- No production behaviour change: no check rule, register, block key, API or contract golden
```

```quote crates/dns/src/api/check.rs:58
pub(crate) const CHECK_RECORD_TYPES: [RecordType; 15] = [
```

```quote crates/dns/src/api/check.rs:845
pub(crate) fn unique_records(lookups: &Lookups) -> Lookups {
```

```quote crates/tls/src/routes.rs:1162
pub(crate) fn compute_consistency(
```

```quote specs/features/v2-fixture-consistency/spec.md:47
- Two checks read the wall clock: TLS (`expired` in `tls.chain`) and DNS (`dns.dnssec` over
```

```quote tests/acceptance/fixtures/history/new.example.com.json:57
        "id": "dns.dnssec",
```

### Refuted

None: the refuter confirmed both blockers (confidence 7 and 8).

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| email testing.rs:615 DKIM selectors as user selectors | CONFIRMED | 7 | held | read dkim.rs:92-121 candidate list (no `mail`), history.rs:115, module.rs:218 |
| tls testing.rs:441 lone self-signed ends_at_anchor | CONFIRMED | 8 | held | read chain_trust.rs:90 and check_chain_complete branches |

### Roll call

| id | answer | evidence |
|---|---|---|
| P03 | absence | no enum, template or API serializer change |
| P10 | absence | the only production edit (TLS clock seam) leaves verdict logic unchanged |
| P12 | absence | no client builder added; optional deps under `testing` only |
| P13 | absence | no limiter or route change |
| P18 | absence | no new Client, no config struct change |
| P26 | absence | no new check id; visibility widenings only |
| P35 | absence | no rule change |
| P36 | convergence | the fixtures change with their generator (the guard under UPDATE_FIXTURES=1) |
| P40 | absence | no module added or removed; engine untouched |

### Summary

Before refutation: reader 2/0/2, spec reader 0/0/2 (REQUIREMENTS met=10 partial=0 missed=0 contradicted=0). After refutation: blockers 2, majors 0, minors 4; verified 2, held 2. Roll call 9 answered, 1 convergence, 0 divergence, 8 absence. Both blockers are fixed on this branch (implementer under the operator's autonomy mandate) and the fix range is read again.

## 06750cd2..a7e4685

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing
MAJOR | tests/acceptance/fixtures/history/example.com.json:892 | The new empty `dns.rrsig` block gives the seeded example.com DNS evidence view an extra explore link, so the committed acceptance test `inspectors.spec.ts` "evidence view example.com/dns" now fails in all 4 width/theme variants. | `block_links` gives every `dns.<type>` records block one link, an empty block included (it falls back to the domain as owner), so the page now renders `/inspect/dns?q=example.com%20RRSIG`. Before the change the fixture had exactly 14 dns records blocks, matching the 14 entries of `BLOCK_LINKS.dns`. It now has 15, so `toHaveLength(1 + 0 + 14)` fails. The fix is to add `/inspect/dns?q=example.com%20RRSIG` to `BLOCK_LINKS.dns` between NSEC3 and SOA.

```quote tests/acceptance/fixtures/history/example.com.json:892
        "id": "dns.rrsig",
```

```quote crates/web/src/explore.rs:73
            vec![link(owner, "dns", "q", &query)]
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=5 partial=0 missed=0 contradicted=0
No findings for R4 and R7–R10.

### Refuted

None: the refuter confirmed the major (confidence 8).

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| example.com.json:892 extra RRSIG explore link | CONFIRMED | 8 | held | `just acceptance-local v2/inspectors.spec.ts --grep "1280 px light"`: with the RRSIG link added to BLOCK_LINKS.dns every evidence view and inspector test passes (22 passed, run with the pre-existing check-row code assertion set aside locally) |

### Summary

Reader 0/1/0, spec reader 0/0/0 (REQUIREMENTS met=5 partial=0 missed=0 contradicted=0), unchanged by refutation; verified 1, held 1. The major is fixed in the next commit (`BLOCK_LINKS.dns` lists the RRSIG link). Not caused by this branch and left as found: the same spec's check-row assertion expects catalogue shorts ("DNSSEC") where the page now shows line codes ("DNS·01"), failing on main's templates. No roll call for this follow-up range (no principle-relevant change beyond the first reading's).
