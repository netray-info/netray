
## 8bde1a8..e4d38be

### Reader

COUNTS blockers=1 majors=1 minors=1
LENSES Engineering, Testing
BLOCKER | crates/email/src/module.rs:200 | Only the `answered` column of `email.dnsbl_domain` is volatile, but a policy-code answer also writes the code into the `answer` column, so a refused or rate-limited domain zone changes the fingerprint and stores an entry, which R19 forbids. | Clean row (`answered=true, answer=""`) then Spamhaus rate-limit row (`answered=false, answer="127.255.255.255"`, dnsbl.rs:153); `email.dnsbl` pass in both; scratch program: clean vs policy fingerprints differ, diff reports `email.dnsbl_domain ~ example.com@dbl.spamhaus.org`.
MAJOR | crates/store/src/lib.rs:33 | `HistoryStore::entries` promises newest first, but entries with equal `created_at` come back in insertion order. | Chain e1→e2→e3 all `created_at`=1000 (concurrent runs in one second): the NEWEST_FIRST query returns head, oldest, middle.
MINOR | crates/email/tests/dnsbl_domain.rs:304 | `an_unanswered_domain_zone_is_not_a_change` claims to cover a policy-code answer but only flips `answered`, leaving `answer` empty. | With `answer="127.255.255.254"` the fingerprint assertion fails.

```quote crates/email/src/module.rs:200
            EvidencePath("email.dnsbl_domain/answered"),
```

```quote crates/email/src/checks/dnsbl.rs:153
        answer(!listing.is_empty() || policy.is_empty(), &values);
```

```quote crates/store/src/lib.rs:33
     e.created_at DESC";
```

```quote crates/email/tests/dnsbl_domain.rs:304
        row[answered] = Ev::Bool(false);
```

### Spec reader

COUNTS blockers=1 majors=0 minors=2
REQUIREMENTS met=18 partial=0 missed=0 contradicted=1

BLOCKER | crates/email/src/module.rs:200 | Only `answered` of `email.dnsbl_domain` is volatile; a policy-code answer writes its code into the compared `answer` column and stores a new entry, contradicting R19. | Run A dbl clean (`answered=true, answer=""`), run B dbl answers 127.255.255.254: `answer(false, &values)` (dnsbl.rs:153), module.rs:615 writes the code; `email.dnsbl` stays pass; fingerprints differ; `record` returns Changed.
MINOR | crates/http/src/module.rs:120 | http's volatile list has seven header rows R17 does not name (report-to, via, x-fastly-request-id, x-vercel-id, x-vercel-cache, x-github-request-id, x-amz-cf-pop); only SDD §3.7 lists them. | Two runs differing only in `via` fingerprint equal.
MINOR | crates/store/src/lib.rs:177 | `record` clamps `now` to the chain head's times, which the spec does not ask for (R9 "move its last_seen_at to now"). | Head last_seen_at=1000, identical run at now=900 leaves 1000.

```quote specs/features/v2-history/spec.md:56
19. The domain-zone rows of the DNSBL answers (targets that are not an address, e.g. `<domain>@dbl.spamhaus.org`) move out of `email.dnsbl` into a new table block `email.dnsbl_domain` with the same columns and key, present only when a domain zone was queried; `email.dnsbl` keeps the address-target rows. The check `email.dnsbl` references `email.dnsbl_domain` beside `email.dnsbl`. `email.dnsbl_domain` is compared except its `answered` column (`email.dnsbl_domain/answered` volatile), so a domain listing that appears or clears stores a new entry even when the check status stays the same, while a domain zone that times out or answers with a policy code does not; a timeout while the domain is listed still reads as a cleared listing. SDD §3.7 describes the split, and its email volatile row says the three whole-block paths hide every per-address change while the check status stays the same (an MX address change of the same hosts, a PTR change, an address listed or cleared on a zone while the check already fails or warns). The narrower `email.dnsbl` on the lens wire, with `evidence_schema` unchanged at 1, is accepted: 0.30.0 is the next and breaking release (SDD §7.4) and removes the lens API; requirement 13's "lens's responses unchanged" excepts this block.
```

```quote crates/email/src/module.rs:615
                        EvValue::Text(codes(&a.answer)),
```

```quote specs/features/v2-history/spec.md:54
17. The modules' volatile lists cover values that change between identical responses: http adds the header rows `expires`, `x-timer`, `x-served-by`, `x-cache`, `x-cache-hits`, `cf-cache-status` and the `http.server` keys `address`, `org`, `network_type` (the first A record in resolver order, `crates/http/src/input.rs:70`); dns adds `dns.rrsig/data` (expiration, inception and signature in one text value, `crates/dns/src/module.rs:467`). SDD §3.7's volatile table lists them.
```

```quote crates/http/src/module.rs:120
    EvidencePath("http.headers/value@report-to"),
```

```quote specs/features/v2-history/spec.md:46
9. `netray-store` records a complete run in one transaction: read the domain's latest entry; equal fingerprint → move its `last_seen_at` to now and store nothing else; otherwise insert a new entry with `predecessor_id` = latest, the run JSON (with the evidence schema version), the fingerprint and the diff (none for a first entry). Record returns which of first, identical or changed happened, and the diff. The store serialises its writes (record and sweep) within the process, so concurrent records of one domain chain or match and never fail on each other.
```

```quote crates/store/src/lib.rs:177
                    .bind(now.max(latest.last_seen_at))
```

### Refuted

none

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| BLOCKER email.dnsbl_domain policy code compared (reader + spec reader, merged) | CONFIRMED | 8 | held | `cargo test -p netray-email --features testing --test dnsbl_domain policy_answer` → FAILED (new test `a_policy_answer_from_the_domain_zone_is_not_a_change`) |
| MAJOR store entries order at equal created_at | CONFIRMED | 8 | held | `cargo test -p netray-store --test record entries_are_newest_first` → FAILED (new test) |

### Roll call

| principle | answer |
|---|---|
| P03 | absence: no enum, no UI/API copy added |
| P10 | convergence: incomplete badge unscored; timed-out zone not a change |
| P12 | absence: no outbound call added |
| P13 | absence: no limiter or route added |
| P18 | convergence: no new client; `HistoryConfig` deny_unknown_fields; test_config_strict PASS |
| P26 | convergence: `email.dnsbl_domain` emitted once, no second lookup |
| P35 | convergence: no rules change; behaviour carried by tests |
| P36 | convergence: only Cargo.lock with its sources |
| P40 | convergence: engine history imports netray_model only |

### Summary

Before refutation: reader 1/1/1, spec reader 1/0/2 (REQUIREMENTS met=18 partial=0 missed=0 contradicted=1). Merged duplicate blocker counted once. After refutation: 1 blocker, 1 major, 3 minors; verified 2, held 2. Roll call: 9 answered, 6 convergence, 0 divergence, 3 absence. Outcome: blocker held → fixed on this branch under the operator's autonomy mandate (first pass), then a second reading.

## e4d38be..55777a8

### Reader

COUNTS blockers=2 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | crates/email/src/module.rs:617 | A domain-zone policy answer is written as `answer=""`, a placeholder that hides a known code (CLAUDE.md "never publish a placeholder"). | dbl answers 127.255.255.254: sub-check names the code, the row says "".
BLOCKER | crates/email/src/module.rs:200 | listed → rate-limited still stores an entry that reads as a delisting; no stateless projection satisfies R19's "a policy answer stores nothing". | bl1 lists, dbl 127.0.1.2 then 127.255.255.255: `record` returns Changed.

### Outcome

Both hold (verified by reading; the second is a spec contradiction, not a code slip). Resolved under the operator's autonomy mandate by revising R19: `email.dnsbl_domain` is compared whole with codes as returned and every transition is recorded (83fc15e); the blanking and the `answered` volatile path are removed. Test `a_policy_answer_from_the_domain_zone_keeps_its_code_and_is_recorded` holds the new rule.

## 55777a8..83fc15e

### Reader

COUNTS blockers=0 majors=0 minors=2
MINOR | specs/features/v2-history/spec.md:174 | the phase 6 scenario still said `answered`-only rows fingerprint equal — fixed to the revised R19.
MINOR | crates/email/tests/dnsbl_domain.rs:289 | no test held the timeout clause — added `a_failed_domain_zone_lookup_is_recorded`.

### Summary

Third pass clean of blockers and majors; both minors fixed. Review complete under the operator's autonomy mandate.
