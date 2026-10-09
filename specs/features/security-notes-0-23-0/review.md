# Review: security notes 0.23.0

## 5baf476..75ed398

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security
MAJOR | CHANGELOG.md:45 | The note says only the DNSSEC walk echoed answers; at e408d7e authcompare also streamed the answer records from a non-public address and named the resolved container address in `done.auth_servers`. | Domain with NS `lens.` queried with `+auth`: glue resolved via the system resolver to 172.30.0.x, label `lens. (172.30.0.x)`, records in `batch` events.
MINOR | CHANGELOG.md:45 | "the checks report it" is false for the DNSSEC walk when a refused address sits beside a public one: it is dropped without a finding. | Referral glue `10.0.0.1` plus one public address: no refusal finding (dns_dnssec.rs:94 runs only when the level is empty).

```quote CHANGELOG.md:45
  Missing glue was resolved through the system resolver, which inside a container answers service names with container addresses. A domain could therefore make prism send queries to a non-public address on port 53, and the DNSSEC walk echoed the answers. Every such query now goes through one outbound policy; a refused address is never queried, and the checks report it as "nameserver address not public, not queried". Found in an internal review. Every earlier release is affected; upgrade to 0.23.0.
```

```quote CHANGELOG.md:43
  - the DNSSEC chain walk, which follows referral glue and returned the records it received.
```

```quote crates/mhost-prism/src/api/authcompare.rs:72
            labels.push(format!("{ns_name} ({})", addr.ip()));
```

```quote crates/mhost-prism/src/api/authcompare.rs:466
                                                    "records": records_json,
```

```quote crates/mhost-prism/src/dns_dnssec.rs:94
            if refused > 0 {
```

### Refutation

| finding | refuter | confidence |
|---|---|---|
| F1 CHANGELOG.md:45 authcompare also echoed answers and named the address | CONFIRMED | 8 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 8 | held | `git show e408d7e:crates/mhost-prism/src/api/authcompare.rs \| grep -n 'format!("{ns_name} ({'` → line 251, unfiltered |

### Summary

Before refutation 0/1/1, after 0/1/1; verified 1, held 1. The R5.8 entry is rewritten: authcompare and the NS check findings named in the impact, the DNSSEC walk's silent drop beside a public address stated.

## 75ed398..4f376df

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering
BLOCKER | CHANGELOG.md:41 | The delegation-consistency findings never named the address at e408d7e; they name NS hostnames, in the mismatch case taken from the answer. Only the lame check named the address. | NS `ns1.example.com` resolving to 172.18.0.5, answering NS `internal.example.`: finding `NS 'internal.example.' is in child zone but not in parent delegation`, no address.

```quote CHANGELOG.md:41
  - the lame-delegation and delegation-consistency checks, whose findings named the address;
```

```quote crates/mhost-prism/src/api/check.rs:1124
                "NS '{ns}' is in child zone but not in parent delegation"
```

### Refutation

| finding | refuter | confidence |
|---|---|---|
| F1 CHANGELOG.md:41 consistency findings named no address | CONFIRMED | 9 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 9 | held | `git show e408d7e:crates/mhost-prism/src/api/check.rs \| sed -n '/fn check_ns_delegation_consistency/,/^}/p'`: messages carry NS names only |

### Summary

Before refutation 1/0/0, after 1/0/0; verified 1, held 1. Line 41 rewritten on this branch.

## 4f376df..8e81376

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering

The changed line holds at e408d7e: the lame check's `Failed` text carries `qr.server.ip()`; the consistency check returns the received NS names that differ from the recursive set.

### Summary

0/0/0; nothing to refute or verify.
