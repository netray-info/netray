# Review: v2 modules

## 7d4b19a..898d12d

### Reader

COUNTS blockers=1 majors=1 minors=1
LENSES Engineering, Security, Testing
BLOCKER | crates/dns/src/module.rs:246 | `facts_from_lookups` takes A/AAAA from all six fact lookups merged, including NS glue mhost keeps from additional sections; the IP section then samples nameserver addresses. 0.23.x read only the A/AAAA batches. | A resolver answering `example.com NS` with `ns1.example.net A 1.0.0.53` in the additional section → `facts.a` holds 1.0.0.53.
MAJOR | crates/lens/src/config.rs:524 | The guard adds `resolve_timeout_ms` to the IP timeout as if IP started after the resolve; the engine counts the IP window from the run start with the resolve inside it. | resolve 5000 / ip 2000 passes yet IP times out at 2 s during a 3 s resolve; resolve 18500 / ip 2000 is refused although it fits.
MINOR | tests/repo/test_check_config.sh:204 | The "missing city db" row is refused by the both-keys rule, not by the missing file. | A loader that silently disabled a missing city DB file would keep the row green.

```quote crates/dns/src/module.rs:246
        a: first_seen(lookups.a().into_iter().copied()),
```

```quote crates/lens/src/config.rs:524
        let resolve_ms = b.resolve_timeout_ms.saturating_add(b.ip.timeout_ms);
```

```quote crates/engine/src/lib.rs:209
            let deadline = (run_start + section).min(base.deadline);
```

```quote tests/repo/test_check_config.sh:204
    "missing city db|geoip_city_db = \"/nonexistent.mmdb\""
```

```quote crates/netray/src/main.rs:211
    if ip_config.geoip_city_db.is_none() || ip_config.geoip_asn_db.is_none() {
```

### Refutation

| finding | refuter | confidence |
|---|---|---|
| F1 facts glue | CONFIRMED | 8 |
| F2 resolve guard | CONFIRMED | 8 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 8 | held | read `facts_from_lookups` (module.rs:246) against `collect_ips_from_batch` (module.rs:372) |
| F2 | CONFIRMED | 8 | held | read config.rs:524 against engine lib.rs:209 |

### Summary

1/1/1 before and after refutation; verified 2, held 2. Fixed on this branch.

## 898d12d..d5027f8

### Reader

COUNTS blockers=1 majors=1 minors=1
LENSES Engineering, Testing
BLOCKER | crates/dns/src/module.rs:308 | The new owner filter never matches on a live resolve (unqualified query name vs fully qualified wire names, hickory `Name::eq`), so facts.mx/caa/ns/https are empty for every domain. | lens strips the trailing dot; checked against mhost 0.12.0 in a scratch crate.
MAJOR | crates/dns/src/module.rs:308 | The filter also drops CNAME chain targets for MX, CAA and HTTPS. | `shop.example.com CNAME shop.example.net.` → facts.mx empty.
MINOR | crates/dns/src/module.rs:536 | The new test's query name is dotted, so it cannot see the mismatch. | live queries are undotted.

### Summary

1/1/1; repaired in the next commit: selection by query type only (mhost already scopes authority/additional records to the queried name; NS glue never reaches a/aaaa).

## d5027f8..2f6cccf

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | crates/dns/src/module.rs:531 | No test fails if the owner filter returns: the fixture's query name is qualified. | live queries are unqualified.

### Summary

0/0/1; the minor is repaired with `facts_keep_records_for_an_unqualified_query_name_and_cname_targets`. mhost's `select_records` and hickory's `Name::eq` confirm the query-type selection.
