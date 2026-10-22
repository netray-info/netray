## 3e8d7e14..675045c5

### Reader

COUNTS blockers=2 majors=0 minors=2
LENSES Engineering, Security, Testing
BLOCKER | crates/dns/src/module.rs:757 | A name below its zone apex whose own NS, DNSKEY or DS lookup failed still gets `dns.ns`, `dns.ns_lame` and `dns.ns_delegation` (or `dns.dnssec`, `dns.dnskey_algorithm` and `dns.dnssec_rollover`) as `unmeasured`, even though `run_check` found the zone and judged its records. The swap at check.rs:620 only reaches `all_lookups`. `translate` still collects the error of the phase-1 batch at the name next to the zone's good batch, which breaks requirements 4 and 5 ("replace those at the queried name in what the checks read"). | Take www.example.com: its SOA answers NODATA on both resolvers and its NS answers SERVFAIL on both. example.com owns an SOA and has NS ns1/ns2.example.net, answering AA. This is c9 in check_zone_tests.rs plus `.servfail("www.example.com","NS")`. The climb finds example.com and sends its NS batch, and the lints and the lame and consistency checks pass. But `batch_errors` holds ("NS", "NS lookup failed: SERVFAIL from …") from the phase-1 batch, so all three NS checks come out Unmeasured. `.servfail("www.example.com","DNSKEY")` does the same to the three DNSSEC checks, even though example.com's DNSKEY and DS answered.

```quote crates/dns/src/module.rs:757
                .filter(|(record_type, _)| inputs.contains(record_type))
```

```quote crates/dns/src/api/check.rs:620
        let others: Vec<_> = all_lookups
```

BLOCKER | crates/dns/src/api/check.rs:1338 | `with_ttl` writes one TTL, the highest across every authoritative RRSIG at the name, onto every RRSIG at that name. RFC 4034 §3 exempts RRSIGs from the RFC 2181 rule: each one carries the TTL of the RRset it covers. Result: the `dns.rrsig` evidence shows a TTL the record does not have, and a low-TTL RRSIG drops out of `dns.ttl`'s finding. | Signed apex example.com: A has TTL 30 (so RRSIG(A) is 30), MX has TTL 3600 (RRSIG(MX) 3600). The resolvers show A and RRSIG(A) at 29, so both (example.com, A) and (example.com, RRSIG) become pairs (line 1237). The AA answer to the RRSIG query reduces to 3600 through `.max()` (line 1287), and `with_ttl` sets RRSIG(A) to 3600. The RRSIG records block then shows RRSIG(A) at 3600, and `dns.ttl` lists only "example.com (30s, A)", although the server serves the RRSIG covering A at 30 s.

```quote crates/dns/src/api/check.rs:1338
                rec["ttl"] = Value::from(ttl);
```

```quote crates/dns/src/api/check.rs:1237
        let pair = (record.name().to_ascii(), record.record_type());
```

```quote crates/dns/src/api/check.rs:1287
                        .max()
```

MINOR | crates/email/tests/it/resolver_tcp_retry.rs:178 | The "no TCP query" assertion cannot fail. `Mode::Small` never binds a TCP listener, so the TCP counter stays 0 whatever the resolver sends over TCP. | `spawn(Mode::Small)` takes the `_ => break (udp, None)` arm (line 74), and only `Truncating { tcp: true }` binds a listener. A resolver that also queried TCP after an untruncated answer, but kept the UDP answer, passes the test.

```quote crates/email/tests/it/resolver_tcp_retry.rs:178
    assert_eq!(r.tcp.load(Ordering::SeqCst), 0, "no TCP query");
```

```quote crates/email/tests/it/resolver_tcp_retry.rs:74
            _ => break (udp, None),
```

MINOR | crates/dns/src/api/query_system_keys_tests.rs:3 | The requirement 9 tests read the host's `/etc/resolv.conf`. On a host with one system nameserver they pass with the pre-fix single "system" key, so they cannot catch a regression there. That includes this test and `a_system_query_sends_batches_and_done` in tests/it/query_reverse_system.rs. | mhost builds one config per resolv.conf nameserver. If resolv.conf lists only `nameserver 127.0.0.53` (the usual systemd-resolved stub, likely on CI's ubuntu-24.04-arm runner), the old code gives 1 resolver and 1 key, `keys.len() == resolvers().len()`, and the job indexes `breaker_keys[0]` without panicking. Both tests pass with the fix reverted, as the test's own comment admits.

```quote crates/dns/src/api/query_system_keys_tests.rs:3
// already; with two or more it fails until `@system` pushes one key per resolver.
```

### Spec reader

COUNTS blockers=0 majors=3 minors=1
REQUIREMENTS met=9 partial=3 missed=0 contradicted=0

MAJOR | crates/dns/src/api/check.rs:1018 | When the stream deadline cuts off the CAA climb, the code reads it as "no CAA anywhere" and `dns.caa` warns "No CAA records found". It is not marked unmeasured, although the zone climb right above it is. | R6. Trace: www.example.com has no CAA, phase 1 ends just before the deadline, and the b.example.com/example.com CAA lookup is still in flight. `timeout_at` fires, `unwrap_or(Ok(None))` adds no batch and no `Unanswered` event, and `check_caa` over the name's empty CAA set warns.

```quote crates/dns/src/api/check.rs:1018
    match caa.unwrap_or(Ok(None)) {
        Ok(None) => {}
```
```quote crates/dns/src/api/check.rs:1004
    match zone.unwrap_or_else(|_| Err(ZONE_DEADLINE.to_owned())) {
```
```quote specs/features/v2-correctness/spec.md:89
6. When the CAA lookup at the queried name answers no CAA record, `run_check` looks up CAA at each
```

MAJOR | crates/dns/src/module.rs:757 | `translate` still reads the queried name's own NS/DNSKEY/DS batches, which phase 1 already sent, beside the zone's replacement batches. A failed name batch therefore makes the zone checks unmeasured even when the zone's lookups answered. | R4 (also reaches R5's checks). Trace: www.example.com answers SOA with NODATA, NS gets SERVFAIL from every resolver, example.com owns its SOA and its NS answers. `batch_errors` holds ("NS", Some(err)) from phase 1 and ("NS", None) from the zone, so `dns.ns`, `dns.ns_lame` and `dns.ns_delegation` become `unmeasured` "NS lookup failed …". The report lists this as DEFERRED.

```quote crates/dns/src/module.rs:755
            batch_errors
                .iter()
                .filter(|(record_type, _)| inputs.contains(record_type))
                .filter_map(|(_, error)| error.clone())
```
```quote specs/features/v2-correctness/spec.md:81
   The zone's NS, DNSKEY and DS lookups replace those at the queried name in what the checks read
```

MAJOR | specs/sdd/v2.md:70 | The SDD rows for R3–R7 (S43 and the §3.9 DNS rows) landed in the phase 5 commit 032ebcf7. The commits that built R3–R6 (b941e658) and R7 (047dc68e) do not touch `specs/sdd/v2.md`. Only R10/R11 share a commit with their code. | R12, the "same commit as the code" clause, for requirements 3 to 7.

```quote specs/sdd/v2.md:70
| S43 | Verdicts as right as V1's, checked live (implementer under the operator's mandate, 2026-10-22; `v2-correctness`, inventory area 10). A name below its zone apex is judged by its zone for NS, DNSSEC and CAA: the zone is the closest ancestor owning its SOA, never above the registrable domain, at most 8 ancestors, within the stream deadline; the zone's NS, DNSKEY and DS replace the name's. `dns.ttl` re-reads records a resolver shows below 60 s at the zone's nameservers and judges the authoritative RRset TTL. The email resolver retries a truncated UDP answer once over TCP to the same server; an Info sub-check no longer suppresses an SPF pass. A measured section with every check unmeasured is labelled `incomplete`. A single-label name starts no domain run. No profile change; grades of subdomains and of domains with these records change. |
```
```quote specs/features/v2-correctness/spec.md:117
12. SDD `specs/sdd/v2.md` states requirements 3 to 7 in §3.9's DNS rows and requirement 10 and 11
```

MINOR | crates/web/src/api/reads.rs:241 | API v1 store reads that start no run (`GET /api/v1/domains/{d}`, `/diff`, `/history`) now refuse a single-label name with 400 "a domain needs at least two labels". A stored entry for `landing` can no longer be read by domain. The spec only asks this of the reads that start runs. | Unrequested behaviour beyond R11. Trace: `GET /api/v1/domains/landing/history` → `domain()` → `LensRuns::validate` → `validate_domain` refuses it before the store is read.

```quote crates/web/src/api/reads.rs:241
    let domain = domain(&state, raw)?;
```
```quote crates/lens/src/input.rs:18
    if !domain.contains('.') {
```
```quote specs/features/v2-correctness/spec.md:112
11. No domain run starts for a single-label name: the API (`POST /api/v1/domains/{domain}/runs`
```

### Merged

The reader's BLOCKER at crates/dns/src/module.rs:757 and the spec reader's MAJOR at the same line are one finding, counted once as BLOCKER.

### Refuted

None.

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| module.rs:757 failed name batch beside the zone's | CONFIRMED | 8 | held | traced `translate`: every Batch error enters `batch_errors`, filtered by type only |
| check.rs:1338 one TTL onto every RRSIG | CONFIRMED | 7 | held | traced `authoritative_ttls` (`.max()` per pair) and `with_ttl` (writes it on every record of the pair) |
| check.rs:1018 CAA climb cut by the deadline reads as no CAA | CONFIRMED | 9 | held | `caa.unwrap_or(Ok(None))` beside `zone.unwrap_or_else(.. ZONE_DEADLINE ..)` |
| sdd/v2.md:70 SDD rows not in the code commits | CONFIRMED | 8 | held | `git log --oneline 3e8d7e14..675045c5 -- specs/sdd/v2.md` lists 032ebcf7 only |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence |
| P10 | convergence |
| P12 | convergence |
| P13 | absence |
| P18 | convergence |
| P26 | convergence |
| P35 | absence |
| P36 | convergence |
| P40 | convergence |

### Summary

Before refutation: reader 2/0/2, spec reader 0/3/1 (REQUIREMENTS met=9 partial=3 missed=0 contradicted=0); merged: 2 blockers, 2 majors, 3 minors. After refutation: 2/2/3, verified 4, held 4. Roll call: answered 9, convergence 7, divergence 0, absence 2.

## 675045c5..c00a1c6e

### Reader

COUNTS blockers=1 majors=1 minors=0
LENSES Engineering, Testing

BLOCKER | crates/dns/src/api/check.rs:1270 | `dns.ttl` now warns on an RRSIG's cached TTL from the resolvers, even when the RRset it covers has a high authoritative TTL. This breaks requirement 7 for every signed name whose resolvers show the signature below 60 s. | Signed apex example.com: the nameservers serve A 192.0.2.10 at TTL 300; the resolvers return that A and its RRSIG(A) both at 29. The A pair is re-read at 300; the RRSIG pair is excluded, keeps 29, and mhost's `check_ttl` warns "example.com. (29s, RRSIG)". c10 only checks that no RRSIG query goes out.

```quote crates/dns/src/api/check.rs:1270
        if record.record_type() != RecordType::RRSIG
```

MAJOR | crates/dns/src/api/check.rs:916 | The zone marker is internal data, and it now goes out on prism's V1 wire (`"source":"zone"` on NS, DNSKEY, DS and climbed CAA batch frames and in the result cache), against the CLAUDE.md convention that internal data rides as `#[serde(skip)]`. | `POST /api/check` for www.example.com with example.com as its zone.

```quote crates/dns/src/api/check.rs:916
        source: source.map(str::to_owned),
```

### Spec reader

COUNTS blockers=0 majors=1 minors=1
REQUIREMENTS met=4 partial=1 missed=0 contradicted=0

MAJOR | crates/dns/src/api/check.rs:1270 | An RRSIG whose cached TTL is below 60 s is never looked up at the authoritative servers, so `dns.ttl` still judges it on its recursive TTL, even when the RRset it covers has an authoritative TTL of 60 s or more. | Req 7, traced through `c10_rrsig_is_not_reread_for_ttl`'s world with `dns.ttl` read: warn "example.com. (29s, RRSIG)".

MINOR | crates/dns/src/api/check.rs:916 | Zone and ancestor batches carry `"source":"zone"` on the V1 prism `+check` SSE stream and in its result cache; no requirement asks for this. | `stream_check_events` serialises `BatchEvent` as is.

### Merged

The reader's BLOCKER and the spec reader's MAJOR at check.rs:1270 are one finding (BLOCKER); the reader's MAJOR and the spec reader's MINOR at check.rs:916 are one finding (MAJOR).

### Refuted

None (no refuter run: both findings were reproduced directly, see Calibration).

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| check.rs:1270 RRSIG keeps its cached TTL | not run | n/a | held | new test c11 (same world as c10, reads `dns.ttl`): warn "example.com. (29s, RRSIG)" at c00a1c6e |
| check.rs:916 zone marker on the V1 wire | not run | n/a | held | traced `stream_check_events` and `send_batch`'s cache push |

Both fixed in bec9ee27: the RRSIGs covering a re-read RRset take its authoritative TTL in an RRSIG batch (c11 green); `wire_batch` leaves the zone marker out of the stream and the result cache.

### Summary

Before refutation: reader 1/1/0, spec reader 0/1/1 (REQUIREMENTS met=4 partial=1). Merged: 1 blocker, 1 major, 0 minors. Verified 2, held 2. Roll call: not repeated for the fix range.

## c00a1c6e..bec9ee27

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing
MAJOR | crates/dns/src/api/check.rs:1337 | An RRSIG only takes its covered RRset's TTL when that RRset is re-read, so an RRSIG whose covered RRset already has a recursive TTL of 60 s or more keeps its cached countdown and `dns.ttl` warns. | Two resolvers: 198.51.100.1 answers A at 29 s and RRSIG(A) at 29 s; 198.51.100.2 answers the A at 300 s and no RRSIG. The A is not below 60 after `unique_records`, so no pair exists and no RRSIG batch is made; `dns.ttl` warns "example.com. (29s, RRSIG)".

```quote crates/dns/src/api/check.rs:1337
            let rrsigs = with_ttl(unique, name, RecordType::RRSIG, Some(rt), ttl, Some(server));
```

### Spec reader

skipped: fix range of one requirement already read in the previous reading

### Refuted

None (no refuter run: reproduced directly).

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| check.rs:1337 RRSIG lifted only on re-read | not run | n/a | held | new test c12 (the two-resolver world): warn "example.com. (29s, RRSIG)" at bec9ee27 |

Fixed in 4cc9e6b7: `rrsig_lifts` gives every RRSIG the highest TTL its covered RRset shows in the unique records, without a lookup (c12 green).

### Summary

Reader 0/1/0, verified 1, held 1; fixed in the branch.

## bec9ee27..4cc9e6b7

### Reader

COUNTS blockers=0 majors=2 minors=0
LENSES Engineering, Testing

MAJOR | crates/dns/src/api/check.rs (rrsig_lifts, removed) | `rrsig_lifts` sent an RRSIG batch for every covered (name, type) on every signed name, labelled with the recursive resolver, so prism's V1 stream claimed TTLs that resolver never served and the DNS frontend reported RRSIG divergence where resolvers agree. | C12 world: a batch says 198.51.100.1 served RRSIG(A) at 300; it answered 29.

MAJOR | crates/dns/src/api/check.rs (rrsig_lifts, removed) | The lifts were returned only when the whole authoritative re-read finished; a deadline during another pair's re-read dropped them and the RRSIG was judged on its cached remainder again. | C12 world plus an in-zone MX at 30 s and a re-read cut by the stream deadline.

(Anchors are omitted: both passages were removed in 85764392.)

### Refuted

None.

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| lift batches on the V1 wire | not run | n/a | held | read `rrsig_lifts` and `stream_check_events` at 4cc9e6b7 |
| lifts lost at the deadline | not run | n/a | held | read `authoritative_ttls` and the `timeout_at(..).unwrap_or_default()` at 4cc9e6b7 |

Fixed in 85764392 by removing the lifts and the RRSIG re-read batches: the TTL lint reads the lookups without RRSIGs (`without_rrsigs`), which judges each signature by its covered RRset (RFC 4034 §3). No batch is added, so prism's wire and the deadline are untouched; c10, c11, c12 green.

### Summary

Reader 0/2/0, verified 2, held 2; both resolved by removal.
