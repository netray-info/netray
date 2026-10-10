---
class: instrument-cannot-vary-with-its-subject
repeat-of: none
---
# ifconfig-rs's ?ip= cache could not tell a dns=false answer from a full one

**What failed.** ifconfig-rs caches `/json?ip=X` responses under the bare IP (`crates/ip/src/routes.rs`, `cache_key = target_addr.ip()`), while `&dns=false` changes the response (no PTR lookup, `hostname` null). Once lens sent `dns=false` for every enrichment, a public `/json?ip=X` within the cache TTL (300 s) got the hostname-less entry. The phase reader missed it; the feature review found it.
**What worked.** A test asserting the cache stays empty after a `dns=false` request (`ip_cache_tests.rs`), then the insert guarded by `!skip_dns`; reading a cached full entry for a `dns=false` request stays allowed.
**How to notice next time.** A new caller sends a request parameter that changes the response, to an endpoint whose cache key does not include it.
