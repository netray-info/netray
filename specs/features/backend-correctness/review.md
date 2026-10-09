# Review: backend correctness

## 8eb895a..5274e57

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Security, Testing

MAJOR | crates/lens/src/backends/ip.rs:178 | lens's enrichment calls now send `dns=false`, but ifconfig-rs's `?ip=` cache key is the bare IP and ignores `skip_dns`, so the hostname-less entry lens caches is served to public `/json?ip=` callers, which ask for the PTR lookup by default: `ip.hostname` null. | Production `[cache] enabled = true`, `ttl_secs = 300`: a lens check stores `/json?ip=93.184.216.34&dns=false` under key `93.184.216.34`; a visitor's `/json?ip=93.184.216.34` within 5 minutes hits it and the hostname card stays hidden. The cache-key flaw existed; this change triggers it on every lens IP check.

```quote crates/lens/src/backends/ip.rs:178
            let url = format!("{base}/json?ip={ip}&dns=false");
```

```quote crates/ifconfig-rs/src/routes.rs:337
            let cache_key = target_addr.ip();
```

Traced sound: prism `unique_records`/`lint_lookups`/`unique_lines`; lens IP filter, per-family sorted sample, "checked N of M", early error, Timeout precedence, NotApplicable shown as skip; ifconfig-rs one classifier, `exempt_cidrs` on the TCP peer, `/batch`/`/diff` still charged; prism `@system` consistent across `/api/config`, `/api/parse` and the UI; `inlineCode` fences; the raw-HTML sink test; trailers on every changed protected test.

### Refuted

None. The MAJOR was CONFIRMED (confidence 8): the `?ip=` cache is keyed on the bare IP, used whenever cache is enabled (production `enabled = true`), and a hit skips `make_ifconfig`, so no PTR lookup happens.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| `ip.rs:178` + ifconfig `routes.rs:337` hostname-less cache entry | CONFIRMED | 8 | held | `cargo test -p ifconfig-rs --lib ip_cache` red at `bbffac9` ("a skip_dns response must not be cached under the bare IP") |

Repaired on this branch: `bbffac9` (test), the following `fix(ifconfig-rs)` commit: a `skip_dns` response is not written to the cache.

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | divergence | `# lens report:` heading in lens's Markdown export (present before this range at `export.ts:34`; now in a code span); V2 V4.12 removes it — no change here |
| P10 | convergence | every IP error and timeout → Errored; missing flags → undecodable → Errored. The new "no public addresses" IP N/A is a by-design case the sentence doesn't name yet (Phase 2 AMENDMENT) — for the operator, like the email bucket N/A |
| P12 | convergence | each enrichment call under one timeout; no new client |
| P13 | absence | no limiter or route change (the ifconfig per-client exemption is a rate limit, not a concurrency limit) |
| P18 | convergence | no new client; `exempt_cidrs` under `deny_unknown_fields`; addresses filtered by `target_policy` |
| P26 | absence | no new check ID |
| P35 | absence | no `specs/rules/` change (new `test_no_raw_html_sinks.sh` enforces R5.5) |
| P36 | convergence | `package-lock.json` changes with its `package.json` edits |
| P40 | absence | no module added or removed |

### Summary

Before refutation 0/1/0, after 0/1/0. verified 1, held 1 (repaired on the branch). Roll call: 9 answered, 4 convergence, 1 divergence, 4 absence.
