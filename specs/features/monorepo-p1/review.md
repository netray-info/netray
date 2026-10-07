## 980d1d2..8228821

### Reader

COUNTS blockers=2 majors=0 minors=1
LENSES Engineering, Security, Testing

BLOCKER | crates/beacon/src/dns/resolver.rs:57 | `netray email` now sends DNS lookups over DoH and DoT, which the separate beacon binary never did, and those lookups cannot pass TLS certificate checks. | Cargo feature unification: linking prism turns on mhost's `dot` and `doh` (crates/mhost-prism/Cargo.toml:9) for beacon too. `PredefinedProvider::Cloudflare.configs()` then returns HTTPS and TLS entries; beacon adds every IPv4 entry without a transport filter (prism filters). Observed: `target/debug/netray email beacon.toml` logged `DNS resolvers initialized count=8` (udp/tcp plus https:1.1.1.1:443, https:1.0.0.1:443, tls:1.1.1.1:853, tls:1.0.0.1:853); the old beacon built 4. `pick()` rotates through all 8. Traced, not observed: hickory-proto is built without `webpki-roots` and without `rustls-platform-verifier`, so the default TLS root store is empty and every DoH/DoT handshake fails; beacon reports those records as lookup failures.
BLOCKER | justfile:57 | `just check` exits 0 in any checkout without GeoIP data even though no ifconfig-rs integration test ran. Requirement 8 says `check` runs every Rust test. | Fresh clone, `just adlc-setup`, `just check`: `GeoLite2-City.mmdb` absent, `test-rust` takes the `else` branch, prints a notice and re-runs what `test-rust-offline` ran; ok_handlers.rs, error_handler.rs, admin.rs, rate_limit.rs, snapshots_test.rs never run; exit 0. tests/repo/test_verbs.sh greps `just -n check` for `cargo test --workspace`, which is in the `if` branch, so it matches either way.
MINOR | tests/repo/test_build_inputs.sh:23 | If bash cannot create the here-string temp file, the scan loop never runs and the test prints PASS. | Observed in the sandbox: "cannot create temp file for here document: Operation not permitted" then "PASS: build inputs clean".

```quote crates/beacon/src/dns/resolver.rs:57
                for ns_config in provider.configs() {
```

```quote crates/mhost-prism/Cargo.toml:9
mhost = { workspace = true, features = ["dot", "doh"] }
```

```quote justfile:57
        echo "notice: no GeoIP data; ifconfig-rs integration tests skipped (just ifconfig-data, then just test-ifconfig-data)" >&2
```

```quote tests/repo/test_build_inputs.sh:23
done <<< "$files"
```

### Refuted

- BLOCKER justfile:57 (`just check` exits 0 without GeoIP data) — REFUTED, confidence 7: the skip is the operator decision of 2026-10-07 (report.md, meta SDD); the spec's requirement 8 had not caught up — amended.

### Calibration

| Finding | Refuter | Conf. | Result | Check |
|---|---|---|---|---|
| BLOCKER beacon resolver.rs:57 DoH/DoT via feature unification | CONFIRMED | 8 | held | `netray email tests/fixtures/beacon.production.toml` logs `DNS resolvers initialized count=8 … https:1.1.1.1:443 … tls:1.0.0.1:853`; old beacon `default-features = false` (git show 980d1d2:crates/beacon/Cargo.toml) |
| BLOCKER justfile:57 check skips GeoIP tests | REFUTED | 7 | not held | `just check` on a clean clone prints the notice; matches the recorded operator decision |

### Summary

- Before refutation: blockers 2, majors 0, minors 1. After: blockers 1, majors 0, minors 1. verified 2, held 1. No principles declared.

### Follow-up: feature-unification sweep (orchestrator)

After the held blocker, every service's dependency features alone (`cargo tree -p <svc> -e features`) were compared with the unified binary (`-p netray`). Additions that could change behaviour, and their verdict:
- `mhost` `dot`/`doh` (+ hickory `tls-ring`/`https-ring`) reach beacon, tlsight, ifconfig-rs. Only beacon uses predefined-provider configs (`provider.configs()`), and it is fixed in f5cd503. tlsight and ifconfig-rs use system/explicit UDP resolvers.
- `rustls` `ring` next to `aws-lc-rs` for lens, spectra, ifconfig-rs, beacon. Safe: reqwest 0.13.1 uses the installed process provider, else aws-lc-rs explicitly (client.rs:705–708, 2453). Only tlsight calls `ClientConfig::builder()`, after installing ring first in `run()`. hickory/mhost do not use the crate-feature default.
- `netray-common` features: no `cfg!`-switched behaviour left after `EnrichmentMode`.

## 8228821..f5cd503

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result. The UDP/TCP filter keeps exactly the configs mhost builds without `dot`/`doh` for every predefined provider (cloudflare, google, quad9, mullvad, wikimedia, dns4eu all have IPv4 UDP+TCP), so no provider ends up empty; `system`, bare IP and IP:port only produce UDP; tlsight and ifconfig-rs build only from `.system()`. The test fails without the fix under the gate's unified `cargo test --workspace --exclude ifconfig-rs` and runs offline. The spec sentence on `check` and GeoIP matches the recipes.

### Summary

- Before and after refutation: 0/0/0. verified 0, held 0. Resolves the held blocker of 980d1d2..8228821 (beacon resolver.rs:57).
