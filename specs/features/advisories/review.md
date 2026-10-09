# Review: advisories

## 106a887..81b73f8

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing

MAJOR | crates/tlsight/src/dns/caa.rs:29 | After the mhost 0.12.0 bump a CAA `issue` record whose tag is not lowercase drops out of `issue_domains()`, so tlsight's `caa_compliant` returns Pass for a certificate from a CA the zone does not authorise. | Zone `example.com. CAA 0 ISSUE "digicert.com"` serving a Let's Encrypt leaf: on hickory 0.25/mhost 0.11 `Property::from(tag)` lowercased the tag and `check_caa_compliance` failed; on hickory 0.26.3 the wire case stays, mhost 0.12's `CAA::from_proto` copies "ISSUE", the filter yields nothing, and the "no issue tags → Pass" branch fires. `issuewild_present()` (caa.rs:44) misses ISSUEWILD the same way. RFC 8659 tags are case-insensitive; no test covers a non-lowercase tag.

```quote crates/tlsight/src/dns/caa.rs:29
            .filter(|r| r.tag == "issue")
```

MINOR | crates/mhost-prism/src/api/query.rs:770 | With `@system` in the group, the second-layer refusal is off for every server in it, so a caller-supplied non-global nameserver passes. | `prism.dev.toml` (`allow_system_resolvers = true`, `allow_arbitrary_servers = true`), `q=example.com A @system @198.18.0.1`: builds and queries 198.18.0.1:53; without `@system` → 422 `BLOCKED_TARGET_IP`. Production leaves `allow_arbitrary_servers = false`.

```quote crates/mhost-prism/src/api/query.rs:770
    if !servers.iter().any(|s| matches!(s, ServerSpec::System)) {
```

Traced sound: the hickory-proto 0.26 port (encoding, EDNS rcode merge, sections, RRSIG fields, `u16` round trip); every `build_resolver_group` caller propagates the refusal; PEM loading; lru 0.18; the let-chain; MSRV; bundled fonts on fontdb 0.24; `tests/repo/test_advisories.sh` passes, its fixture case can fail; every changed protected test carries `ADLC-Test-Change`.

### Refuted

None. The MAJOR was CONFIRMED (confidence 9): tlsight copies the tag verbatim (`crates/tlsight/src/dns/caa.rs:94`), mhost 0.12.0 `CAA::from_proto` copies it, hickory-proto 0.26.3 `read_tag` keeps `A`–`Z`, and 0.25.2 lowercased in `Property::from` (`caa.rs:259`).

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| tlsight `caa.rs:29` CAA tag case | CONFIRMED | 9 | held | sources read (mhost-0.12.0 `caa.rs:40`, hickory-proto-0.26.3 `caa.rs:565`, 0.25.2 `caa.rs:255`); `cargo test -p tlsight --lib caa_tags_match_case_insensitively` red at `a530bc7` |

Repair on this branch: `a530bc7` (test), the following `fix(advisories)` commit lowercases tags when the record is built and compares case-insensitively; tlsight's API output keeps lowercase tags as on 0.22. The MINOR (`@system` mixing, `query.rs:770`) stays as the spec decided (R2.6, `NameServerConfig::is_global()` noted for later).

Not covered here and handed to the operator: mhost 0.12.0's own CAA lint compares tags case-sensitively, so prism's `caa` lint reports an uppercase `ISSUE` as unknown (lens `caa` Pass → Warn); the fix belongs in mhost (Phase 2 report, DEFERRED).

### Roll call

| Principle | Answer | Evidence |
|---|---|---|
| P03 | absence | no enum, template or serializer added |
| P10 | absence | no scoring path changed; prism's DNSSEC/raw branches keep their logic |
| P12 | absence | no new outbound call; `send_udp`/`send_tcp` keep their timeout |
| P13 | absence | no serve, limiter or route change |
| P18 | convergence | `deny_non_global(true)` refuses non-global caller nameservers, mapped to `BlockedTargetIp`, propagated by compare; no new `reqwest::Client` |
| P26 | absence | no new check ID |
| P35 | absence | no `specs/rules/` change |
| P36 | convergence | `Cargo.lock` changes only with its manifests and `deny.toml` |
| P40 | absence | no module added or removed |

### Summary

Before refutation 0/1/1, after 0/1/1. verified 1, held 1 (repaired on the branch). Roll call: 9 answered, 2 convergence, 0 divergence, 7 absence.
