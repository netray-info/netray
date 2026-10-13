## 4e37fe0..3cd101b

### Reader

COUNTS blockers=1 majors=1 minors=0
LENSES Engineering, Testing
BLOCKER | crates/email/src/dns/mod.rs:21 | When a fetch host's A lookup answers but its AAAA lookup gets SERVFAIL or times out, the MTA-STS policy fetch and the BIMI logo fetch get no address at all. MTA-STS then reports a measured `fail` and BIMI a `warn`, so a lookup error becomes a defect, which Rule M forbids. | `mta-sts.example.com` A 203.0.113.10 answers and its AAAA times out. Since this change `lookup_ips` errors when either half fails (resolver.rs:109). `FetchResolver` turns that error into `[]`, the fetch returns `Blocked{NoAddress}`, and the result is sub-check `https_fetch_failed` Fail, so `email.mta_sts` is `fail`. Before the change the fetch used the A address and passed. BIMI reaches `logo_unreachable` (Warn).
MAJOR | crates/email/src/checks/fcrdns.rs:44 | An IPv4 MX address that the PTR name's A answer confirms is reported `unmeasured` when the AAAA half of the same forward lookup errors. The confirming A records are thrown away, and the run loses its letter. | 192.0.2.10 has PTR `out.example.net`; `out.example.net` A answers 192.0.2.10 and its AAAA gets SERVFAIL. `lookup_ips` returns Err, so `email.fcrdns` is `Unmeasured` and the grade is `incomplete`. Before the change this case gave `fcrdns_pass`.

```quote crates/email/src/dns/mod.rs:21
        Box::pin(async move { r.lookup_ips(&h).await.unwrap_or_default() })
```

```quote crates/email/src/dns/resolver.rs:109
            (Err(e), _) | (_, Err(e)) => {
```

```quote crates/email/src/checks/fcrdns.rs:44
                    let forward_ips = resolver.lookup_ips(&ptr_name).await;
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| BLOCKER `crates/email/src/dns/mod.rs:21` | CONFIRMED | 8 | held | read `resolver.rs:100-121`: `(Err(e), _) \| (_, Err(e)) => return Err`, and `FetchResolver` returns `unwrap_or_default()` |
| MAJOR `crates/email/src/checks/fcrdns.rs:44` | CONFIRMED | 8 | held | same `lookup_ips` path; an Err never confirms; one MX IP gives Unmeasured |

### Summary

- Before refutation: 1 blocker, 1 major, 0 minor. After refutation: unchanged.
- Verified 2 findings; both held.
- Root cause: `lookup_ips` discards a successful A answer when the AAAA half errors (Phase 5 reader round, C21). The fix belongs on this branch: `lookup_ips` returns the addresses it got together with the error of the failed half, and each caller decides under Rule M. A fetch uses the addresses. FCrDNS confirms from the addresses and is unmeasured only when the IP is unconfirmed and the error concerns that IP's address family.

## 3cd101b..772970f

### Reader

COUNTS blockers=0 majors=2 minors=2
LENSES Engineering, Testing
MAJOR | crates/email/src/checks/mx.rs:230 | When an MX host's AAAA half fails and its A half answers, `email.fcrdns` and `email.dnsbl` report Pass with no finding, although they never checked the IPv6 MX addresses. | `mx.example.com`: A 192.0.2.10 answers and AAAA SERVFAILs. Result: fcrdns Pass [], dnsbl Pass []. Neither names the unchecked addresses.
MAJOR | crates/email/src/checks/spf.rs:354 | The SPF `a`/`mx` mechanisms drop `AddressLookup.error`, so a failed AAAA half becomes a measured `spf_mx_coverage` Warn. | `v=spf1 a:out.example.com -all`, where `out.example.com` A answers and AAAA SERVFAILs, and the MX has 2001:db8::25. Result: `email.spf` Warn "MX IP(s) not in SPF: 2001:db8::25".
MINOR | crates/email/src/dns/mod.rs:47 | No test covers the family filter in `missed`. | Removing the filter leaves the suite green.
MINOR | crates/email/src/dns/resolver.rs:108 | The production code that combines the A and AAAA halves has no test. | Reverting the condition leaves the suite green.

```quote crates/email/src/checks/mx.rs:230
        (all_ips.is_empty() && !addr_errors.is_empty()).then(|| addr_errors.join("; "));
```

```quote crates/email/src/checks/spf.rs:354
                .map(|a| a.ips)
```

```quote crates/email/src/dns/mod.rs:47
        self.error.as_ref().filter(|e| e.record_type == family)
```

```quote crates/email/src/dns/resolver.rs:108
        if let (Err(e), Err(_)) = (&a_result, &aaaa_result) {
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| MAJOR `crates/email/src/checks/mx.rs:230` | CONFIRMED | 7 | held | `cargo test -p netray-email --features testing --test partial_address_findings`: C29 red at e567feb, findings [] |
| MAJOR `crates/email/src/checks/spf.rs:354` | CONFIRMED | 7 | held | same test, C30 red: `email.spf` Warn "MX IP(s) not in SPF: 2001:db8::25" |

### Summary

- Counts before and after refutation: 0 blockers, 2 majors, 2 minors. Verified 2, held 2.
- Both majors are fixed in dd082a8. FCrDNS and DNSBL now name each failed MX-host address lookup; SPF records a failed family, and the coverage check skips it.
- The two minors are covered by in-source tests, `combine_keeps_the_answered_half` and `missed_is_the_failed_family_only`.

## 772970f..dd082a8

### Reader

COUNTS blockers=0 majors=2 minors=2
LENSES Engineering, Testing
MAJOR | crates/email/src/checks/spf.rs:396 | A failed `mx:` lookup inside SPF is still swallowed into an empty list, so `spf_mx_coverage` warns from a SERVFAIL. | `v=spf1 mx:relay.example.net -all`, where the MX lookup of relay.example.net SERVFAILs and the MX is 192.0.2.10. Result: `email.spf` Warn "MX IP(s) not in SPF: 192.0.2.10".
MAJOR | crates/email/src/checks/spf.rs:267 | A failed `include:` TXT lookup counts as a void lookup and leaves the authorised set incomplete, and coverage still warns. | `v=spf1 include:_spf.example.net -all`, where the include's TXT SERVFAILs. Result: `email.spf` Warn.
MINOR | crates/email/src/checks/dnsbl.rs:156 | When only IPv6 MX IPs remain and the zones are IPv4-only, no query is sent and the result is `clean` Pass. | `zones = ["zen.spamhaus.org"]`, where the MX A half SERVFAILs and AAAA answers.
MINOR | crates/email/src/checks/mod.rs:236 | No test pins the family and "address" clauses in `family_unresolved`. | Replacing the predicate with `true` leaves the suite green.

```quote crates/email/src/checks/spf.rs:396
            let mx_records = resolver.lookup_mx(target).await.unwrap_or_default();
```

```quote crates/email/src/checks/spf.rs:267
        let txt_records = resolver.lookup_txt(domain).await.unwrap_or_default();
```

```quote crates/email/src/checks/dnsbl.rs:156
    if nothing_listed && !answered_any && !unanswered.is_empty() {
```

```quote crates/email/src/checks/mod.rs:236
        .any(|s| s.detail.starts_with(family) || s.detail.starts_with("address "))
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| MAJOR `crates/email/src/checks/spf.rs:396` | not run; I checked it myself | — | held | `cargo test -p netray-email --features testing --test spf_nested_errors`: C32 red at 0d5ece5 (`email.spf` Warn) |
| MAJOR `crates/email/src/checks/spf.rs:267` | not run; I checked it myself | — | held | same test: C33 red (Warn) and C34 red (void-lookup Fail) |

### Summary

- Counts before refutation: 0 blockers, 2 majors, 2 minors. After: unchanged. Verified 2, held 2.
- Both majors and the DNSBL minor are fixed. Every nested SPF lookup error is recorded as `lookup_failed`. A failed lookup is never a void lookup. The coverage check skips an incomplete authorised set.
- The `family_unresolved` minor is pinned by C35 and C36.

## dd082a8..c5fdb6a

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | crates/email/src/checks/spf.rs:429 | A failed `exists:` lookup is stored as an `A`-family `lookup_failed`, and `family_unresolved` reads it as the A half of an address lookup. MX coverage is then skipped for IPv4 but still judged for IPv6, so it warns from an incomplete authorised set. | `v=spf1 exists:relay.example.net -all` with relay.example.net's A SERVFAILing, or a macro `exists:%{i}._spf.example.net` that hickory rejects before sending a query. MX 192.0.2.25 and 2001:db8::25. Result: `email.spf` Warn "MX IP(s) not in SPF: 2001:db8::25".

```quote crates/email/src/checks/spf.rs:429
                    Err(e) => sub_checks.push(lookup_failed(&e)),
```

```quote crates/email/src/checks/mod.rs:237
        .any(|s| !s.detail.starts_with(other_family))
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| BLOCKER `crates/email/src/checks/spf.rs:429` | not run; I checked it myself | — | held | `cargo test -p netray-email --features testing --test spf_exists`: C38 and C39 red at de238e1 (`email.spf` Warn) |

### Summary

- Counts before refutation: 1 blocker, 0 majors, 0 minors. After: unchanged. Verified 1, held 1.
- The blocker is fixed in 3d6f9a5. The `spf_incomplete` marker carries the unresolved families as a structured token. The coverage check reads only that marker. Macro targets are not queried.

## c5fdb6a..3d6f9a5

### Reader

COUNTS blockers=0 majors=2 minors=1
LENSES Engineering, Testing
MAJOR | crates/email/src/checks/spf.rs:544 | When an `exists:` target with no macro resolves, every client IP passes SPF, but nothing is added to the authorised set, so MX coverage warns. This predates the branch. | `v=spf1 exists:relay.example.net -all` with relay.example.net resolving and MX 192.0.2.25. Result: `email.spf` Warn "MX IP(s) not in SPF: 192.0.2.25".
MAJOR | crates/email/src/checks/spf.rs:440 | A macro mechanism always marks both families unresolved, even with a `-`, `~` or `?` qualifier that can never authorise an IP. A certain coverage gap is then suppressed. | `v=spf1 -exists:%{i}._deny.example.net ip4:192.0.2.0/24 -all` with MX 198.51.100.25. Result: `email.spf` Pass, with no coverage finding.
MINOR | crates/email/tests/spf_exists.rs:82 | C39's `"void"` assertion cannot fail with only one `exists:`. | The void threshold is greater than 2 (spf.rs:263).

```quote crates/email/src/checks/spf.rs:544
                    Ok(true) => {}
```

```quote crates/email/src/checks/spf.rs:440
            sub_checks.extend(Unresolved::BOTH.marker());
```

```quote crates/email/tests/spf_exists.rs:82
                finding_absent: &[COVERAGE, "void"],
```

```quote crates/email/src/checks/spf.rs:263
    if void_count > 2 {
```

### Calibration

| Finding | Refuter | Confidence | Result | Check |
|---|---|---|---|---|
| MAJOR `crates/email/src/checks/spf.rs:544` | not run; I checked it myself | — | held | `cargo test -p netray-email --features testing --test spf_exists_qualifier`: C41 red at 12cbd98 (`email.spf` Warn) |
| MAJOR `crates/email/src/checks/spf.rs:440` | not run; I checked it myself | — | held | same test: C42 red (Pass instead of Warn) |

### Summary

- Counts before refutation: 0 blockers, 2 majors, 1 minor. After: unchanged. Verified 2, held 2.
- Both majors are fixed in the last fix commit. A matching positive `exists:` gives an `any` marker. Markers come only from `+`-qualified mechanisms.
- The minor is covered by C43 (three macro targets, no void).
- Operator (2026-10-13): no further re-review after this fix unless a blocker appears.
- DEFERRED, from before the branch: `-a`, `-mx`, `-ip4` and `-ip6` still add their addresses to `authorized_prefixes`, whatever the qualifier.
