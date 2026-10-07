---
class: environment-decides-the-outcome
repeat-of: none
---
# Linking every service into one binary changed two services' behaviour through Cargo feature unification

**What failed.** After the merge into one workspace and one `netray` binary, each crate's own tests were green, but the binary behaved differently from the separate binaries in two places. `netray-common`'s enrichment client picked transport, cache and User-Agent by `#[cfg(feature = "backend")]` / `cfg!(feature = "enrichment-cache")`, so spectra, beacon and tlsight inherited prism's choices. prism's `mhost = { features = ["dot", "doh"] }` made `PredefinedProvider::Cloudflare.configs()` return DoH/DoT entries for beacon too: `netray email` with `resolvers = ["cloudflare"]` logged `DNS resolvers initialized count=8` (old beacon: 4, UDP/TCP), and hickory had no TLS roots for those.
**What worked.** Runtime choice instead of feature choice: `EnrichmentMode::{Plain, Backend { cache_ttl_secs }}` passed by each service, and beacon filtering provider configs to `Protocol::Udp | Protocol::Tcp` (crates/beacon/src/dns/resolver.rs). Tests that only fail in the unified build (`cargo test --workspace`), then a sweep comparing `cargo tree -p <svc> -e features` with `cargo tree -p netray -e features` per service.
**How to notice next time.** A service's dependency gains a feature in `cargo tree -p netray -e features` that it lacks in `cargo tree -p <svc> -e features`, and the crate branches on that feature.
