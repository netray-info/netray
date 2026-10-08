
## d134679..dffd586

### Reader

COUNTS blockers=1 majors=0 minors=2
LENSES Engineering, Security, Testing
BLOCKER | crates/beacon/src/checks/bimi.rs:52 | A BIMI redirect hop whose name does not resolve is reported as a redirect to a private address (`logo_redirect_ssrf_blocked` Fail, grade D), so the detail text is false. Spec R4 says "any other refused or unresolvable target gives `logo_unreachable` Warn". | `l=https://logo.example.com/l.svg`. logo.example.com is public and answers `302 Location: https://cdn.gone.example/l.svg`. Beacon's `lookup_ips("cdn.gone.example")` returns `[]` (NXDOMAIN, or a SERVFAIL/timeout on both A and AAAA). `Checked` returns `Refused(NoAddress)`, `map_error` returns `Blocked{hop:1, reason:NoAddress}`, and the first arm emits Fail with "BIMI logo URL redirects to private address". Before this change the same chain gave `logo_unreachable` Warn (grade B). No results-table row covers a redirect hop with no address.
MINOR | crates/beacon/src/checks/mta_sts.rs:383 | `body_exceeds_cap_warns` now calls `evaluate_policy` with `truncated=true` hard-coded, so it no longer tests the fetch-side cap. It stays green if `truncate_body: true` (mta_sts.rs:165) is deleted. | Delete line 165. A 65,537-byte policy then fails with `FetchError::BodyTooLarge` and gets `https_fetch_failed` Fail "failed to fetch policy" instead of `policy_body_too_large` Warn. This test and `mta_sts_results_table` (no oversized-body row) both still pass.
MINOR | crates/common/Cargo.toml:99 | The change departs from the CLAUDE.md convention "Shared dependencies go in `[workspace.dependencies]` once two crates use them": `rcgen`, `rustls` and `tokio-rustls` are declared separately in four crates. | `rcgen = "0.14"`, `rustls = "0.23"` and `tokio-rustls = "0.26"` were tlsight-only before this range. They are now also added per crate in crates/common, crates/beacon (Cargo.toml:72) and crates/mhost-prism (Cargo.toml:66), and none is in the root `[workspace.dependencies]`.

```quote crates/beacon/src/checks/bimi.rs:52
        FetchError::Blocked { hop, .. } if *hop >= 1 => (
```

```quote crates/beacon/src/checks/bimi.rs:55
            "BIMI logo URL redirects to private address",
```

```quote crates/beacon/src/checks/mta_sts.rs:383
    async fn body_exceeds_cap_warns() {
```

```quote crates/beacon/src/checks/mta_sts.rs:165
        truncate_body: true,
```

```quote crates/common/Cargo.toml:99
rcgen = "0.14"
```

```quote crates/beacon/Cargo.toml:72
rcgen = "0.14"
```

### Refuted

None.

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| B1 unresolvable BIMI redirect hop → `logo_redirect_ssrf_blocked` Fail | CONFIRMED | 9 | held | `cargo test -p beacon --lib bimi_redirect_to_unresolvable` (new test, fails: left `logo_redirect_ssrf_blocked` Fail, right `logo_unreachable` Warn) |

### Roll call

Not run: the principles were declared on `main` (b3d6334) after this branch forked, so this branch's `adlc.toml` declares none.

### Summary

Before refutation: 1 blocker, 0 majors, 2 minors. After: 1 / 0 / 2. Verified 1, held 1. Fix on this branch: a redirect hop with no address maps to `logo_unreachable` Warn, as before.

## dffd586..a05aa44

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | specs/features/outbound-fetch/plan.md:142 | The plan's BIMI mapping table still maps every refused redirect hop to `logo_redirect_ssrf_blocked` Fail, while decision 10, spec item 4 and `bimi.rs:53-57` give a `NoAddress` hop `logo_unreachable` Warn. | `Blocked{hop: 1, reason: NoAddress}` (test `bimi_redirect_to_unresolvable_name_warns`): code Warn, table Fail. Repaired: the row is split by reason.

### Summary

0 blockers, 0 majors, 1 minor (repaired in the plan). No refutation needed.

Note: the review row for dffd586..a05aa44 was recorded with estimated usage (60000 tokens, 300 s); the reader reported 76378 tokens, 142 s.
