
## 5ae774d2..be7f0eca

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/web/tests/api_dns_summary.rs:251 | Test C16 builds its expected `soa_serial_age_days` from a second wall-clock read taken after the response, so it fails whenever the run crosses UTC midnight. CLAUDE.md asks for a wall-clock golden to be normalised, and this one is not. | The job reaches `done` at 23:59:59.995 UTC, so `summarise` uses day D. The test then reads `SystemTime::now()` at 00:00:00.010 and expects day D+1 minus 20741. The assertion fails by one, though the code under test is correct.
MINOR | specs/sdd/v2.md:950 | The SDD was amended in a later commit (fafad52b) than the code that changed what it says. CLAUDE.md:24 says to amend it in the same commit, and the spec's own Context says the same. | At commit 87b956df, which adds `GET /api/v1/dns/results/{key}`, and at 591839b0 (GET forms) and ee6d68f9 (`/dns/check`), §7.3 of specs/sdd/v2.md still says "`/api/results/{key}` is cut", and §7.1 lists none of the new endpoints.

```quote crates/web/tests/api_dns_summary.rs:251
    let secs = std::time::SystemTime::now()
```

```quote crates/dns/src/jobs/summary.rs:128
    summarise_on(batches, Utc::now().date_naive())
```

```quote CLAUDE.md:24
| `specs/sdd/v2.md` | the V2 SDD, this repository's own: amend it in the same commit as the code that changes what it says. Its token source is `crates/web/tokens.json`. PRD, reviews and baseline stay private; cite them as "(private)" |
```

### Spec reader

COUNTS blockers=1 majors=1 minors=1
REQUIREMENTS met=15 partial=1 missed=0 contradicted=1

BLOCKER | crates/web/src/api/dns.rs:548 | The check endpoint never calls the query parser. It validates the name with its own rule instead: `netray_common::domain::validate` plus an ASCII allowlist. The spec reads otherwise, and neither the spec nor its decision log was amended (the change is recorded only in report.md's Phase 5 review) | Req 15, case: `POST /api/v1/dns/check {"domain":"example.com other"}`. `parser::parse` would take the first token `example.com` and run the check. The code answers 400 "`example.com other` is not a domain: it holds ' '". `münchen.de` is accepted by the parser (and by lens's `validate`) but refused here

```quote specs/features/v2-dns-api/spec.md:141
    - The domain is validated by the query parser; an IP address, an empty or invalid name answers
```

```quote crates/web/src/api/dns.rs:548
fn check_domain(raw: &str) -> Result<String, V1Error> {
```

```quote crates/web/src/api/dns.rs:563
    netray_common::domain::validate(name).map_err(V1Error::InvalidInput)
```

MAJOR | crates/dns/src/jobs/results.rs:151 | The cache key is the last 48 bits of `Uuid::now_v7()`. In uuid 1.26.1 (Cargo.lock), those bits are 16 low bits of the shared 42-bit monotonic counter plus 32 random bits, so a third of every key comes from a counter rather than a random draw | Req 13, case: two jobs finish in the same millisecond. Their keys share the counter state: the second key's first 4 hex digits are the first key's plus 1, and only the last 8 hex digits are drawn fresh. The comment "from the random bits" is also wrong

```quote specs/features/v2-dns-api/spec.md:128
    `done` (the check: when it answers) under a fresh key of 12 lowercase hex characters drawn from
```

```quote crates/dns/src/jobs/results.rs:150
    /// A key from the random bits of a UUID v7: its last 12 hex characters.
```

MINOR | crates/web/src/api/dns.rs:704 | `GET /dns/servers`, `GET /dns/record-types` and `POST /dns/parse` answer `503 UNAVAILABLE` when no DNS part is built. The spec names a 503 only for the check, and the first two read only static catalogues | Reqs 11, 12 and 10, case: lens built with the DNS module off, then `GET /api/v1/dns/record-types`. The spec says it answers `{record_types}`; the code answers 503 "the DNS inspector is off"

```quote crates/web/src/api/dns.rs:701
pub(super) async fn dns_servers(
```

```quote specs/features/v2-dns-api/spec.md:145
    - With no DNS module built it answers `503 UNAVAILABLE`.
```

### Refutation

| finding | refuter | confidence |
|---|---|---|
| BLOCKER dns.rs:548 check validation differs from spec R15 (spec not amended) | CONFIRMED (the defect is the unamended spec) | 8 |
| MAJOR results.rs:151 key holds 16 counter bits | CONFIRMED | 8 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| BLOCKER dns.rs:548 | CONFIRMED | 8 | held | read `check_domain` against spec.md:141 and plan.md:70 |
| MAJOR results.rs:151 | CONFIRMED | 8 | held | read uuid-1.26.1/src/v7.rs:78-110 (counter below the version nibble, 32 random bits left) |

### Summary

Before refutation: reader 0/0/2, spec reader 1/1/1 (REQUIREMENTS met=15 partial=1 missed=0
contradicted=1). After refutation: unchanged, 1/1/3 in all; verified 2, held 2. No roll call
(`adlc.toml` declares no principles with sentences). Outcome: blocker; fixed on this branch under
the operator's autonomy mandate (spec amended, keys from UUID v4, the reference reads independent
of the DNS part), then read again.

## be7f0eca..af2f84d0

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Security, Testing
MINOR | specs/features/v2-dns-api/spec.md:143 | The new name-rule text says a name with whitespace answers `400 INVALID_INPUT`, but `check_domain` trims the body's whole `domain` first. A padded name therefore answers 200, and its replay stores the untrimmed string as `query`. | `POST /dns/check {"domain":" example.com "}` runs on `example.com` and answers 200; the replay shows `query: " example.com "`. MINOR because lens's own rule trims and normalises padded input.

```quote specs/features/v2-dns-api/spec.md:143
      `.`; an IP address (a trailing dot included), an empty name, a name with whitespace or any
```

```quote crates/web/src/api/dns.rs:549
    let name = raw.trim();
```

### Spec reader

COUNTS blockers=0 majors=1 minors=0
REQUIREMENTS met=5 partial=1 missed=0 contradicted=0
MAJOR | crates/web/src/api/dns.rs:549 | `check_domain` trims the POST body's `domain` before it applies the character allowlist, so a name with leading or trailing whitespace runs the check instead of getting a 400 | R15: `POST /dns/check {"domain": " example.com\n"}` passes and answers 200, while the replay stores `query` as the untrimmed `" example.com\n"`. The requirement says a name with whitespace answers 400.

```quote specs/features/v2-dns-api/spec.md:141
    - The name is the POST body's whole `domain`, or `q`'s first token. It must pass lens's domain
```

```quote crates/web/src/api/dns.rs:548
fn check_domain(raw: &str) -> Result<String, V1Error> {
```

```quote crates/web/src/api/dns.rs:555
    if let Some(c) = name
```

### Refutation

The reader's MINOR (spec.md:143) and the spec reader's MAJOR (dns.rs:549) are one defect, merged at
MAJOR.

| finding | refuter | confidence |
|---|---|---|
| MAJOR dns.rs:549 check trims a padded name instead of refusing it | CONFIRMED | 8 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| MAJOR dns.rs:549 | CONFIRMED | 8 | held | read `check_domain`: `raw.trim()` precedes the allowlist |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence: the new enums are `AnswerOutcome` and `Agreement`, no status type; check entries carry the engine's `Status` |
| P10 | convergence: incomplete and timed-out sections keep their kinds; a failed lookup stays `unmeasured` |
| P12 | convergence: the check runs the module under a 30 s deadline; no new reqwest client |
| P13 | convergence: jobs and check charge `[api.dns]`; the read routes sit behind `[api]` per-IP |
| P18 | convergence: no new outbound client, no new config struct |
| P26 | absence: no check ID added |
| P35 | absence: no rule changed |
| P36 | convergence: the contract golden changes with its generator |
| P40 | absence: no module added or removed |

### Summary

Before and after refutation: reader 0/0/1, spec reader 0/1/0 (REQUIREMENTS met=5 partial=1
missed=0 contradicted=0); merged 0/1/0; verified 1, held 1. Roll call: answered 9, convergence 6,
divergence 0, absence 3. The major is fixed in the next commit (the check refuses a padded name).

## af2f84d0..8b3a1084

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result: `check_domain` no longer trims, so padded names (`" example.com"`, `"example.com\n"`,
U+00A0 at either end, `" 192.0.2.1"`) answer 400 INVALID_INPUT; the empty string keeps validate's
400; the GET form is unchanged; the new test rows fail against the old trim.

### Spec reader

skipped: a one-line fix of the previous reading's major, read against requirement 15 by the reader.

### Summary

0/0/0 before and after refutation; verified 0, held 0. Nothing to refute.
