# Review: changelog 0.23.0

## 71f239c..e314332

### Reader

COUNTS blockers=2 majors=0 minors=1
LENSES Engineering
BLOCKER | CHANGELOG.md:17 | Truncated streams are not counted in `lens_unknown_verdict_total`; only `unknown_verdict()` increments it. | prism SSE body without `done`: `sse::drain` returns `Err("stream ended without done")`, `check_dns_inner` maps it to a plain `BackendError`; counter stays 0.
BLOCKER | CHANGELOG.md:22 | A domain with no A/AAAA at all gets an errored IP section (`NoDnsResults`), not "not applicable"; N/A only when addresses exist and all are refused. | Mail-only apex: `IpBackend::run` returns `Err(SectionError::NoDnsResults)`, mapped to `Errored`, result incomplete.
MINOR | specs/features/changelog-0-23-0/report.md:37 | The amendment says `netray lens --check-config` does not exist, but it does and runs the same validation. | `netray lens --check-config cfg.toml` with dns 19000 + ip 2000 exits 1.

```quote CHANGELOG.md:17
- **Unknown verdicts and truncated streams** make a section errored and are counted in `lens_unknown_verdict_total{section}`. (R3.4, R3.5)
```

```quote crates/lens/src/backends/sse.rs:87
    Err(format!("stream ended without {terminal}"))
```

```quote crates/lens/src/backends/dns.rs:156
                message: format!("failed to read prism stream: {e}"),
```

```quote crates/lens/src/backends/mod.rs:90
    metrics::counter!("lens_unknown_verdict_total", "section" => section).increment(1);
```

```quote CHANGELOG.md:22
- **IP reputation** comes from ifconfig-rs's flags (`is_spamhaus`, `is_c2`, `is_tor` → fail, `is_vpn` → warn); lens enriches up to four IPv4 and four IPv6 public addresses, sorted, and says "checked N of M addresses" when it sampled; a failed enrichment makes the IP section incomplete; a domain with no public address has the IP section not applicable. ifconfig-rs classifies `/json`, `/network` and `/range` with one function. (R5.2)
```

```quote crates/lens/src/backends/ip.rs:85
            return Box::pin(async { Err(SectionError::NoDnsResults) });
```

```quote crates/lens/src/check.rs:222
        Err(_) => SectionInput {
```

```quote specs/features/changelog-0-23-0/report.md:37
- The scratchpad draft said `netray lens --check-config` refuses a timeout budget; no such flag exists, and lens refuses the configuration when it loads. Corrected in the entry.
```

```quote crates/netray/src/main.rs:100
        } => check_config(&path, lens::config::Config::load),
```

### Refutation

| finding | refuter | confidence |
|---|---|---|
| F1 CHANGELOG.md:17 truncated streams not counted | CONFIRMED | 8 |
| F2 CHANGELOG.md:22 no-address domain errors, not N/A | CONFIRMED | 8 |

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 | CONFIRMED | 8 | held | `grep -rn "unknown_verdict(" crates/lens/src`: no caller in `sse.rs` or the stream error mapping |
| F2 | CONFIRMED | 8 | held | `sed -n 84,85p crates/lens/src/backends/ip.rs; sed -n 222,225p crates/lens/src/check.rs` |

### Summary

Before refutation 2/0/1, after 2/0/1; verified 2, held 2. Both blockers are wrong claims in the changelog text; fixed on this branch.

## e314332..2639f8a

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

Every changed claim (CHANGELOG.md:17, :22; report.md:37-39) checked against crates/ at 2639f8a and found carried out.

### Summary

0/0/0; nothing to refute or verify.
