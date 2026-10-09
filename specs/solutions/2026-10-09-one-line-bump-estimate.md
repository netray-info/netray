---
class: unverified-claim-treated-as-fact
repeat-of: none
---
# "Exactly one line changes" held for mhost, not for prism's own hickory use

**What failed.** The SDD estimated the mhost 0.12 bump at one changed line, verified with `cargo check` against mhost alone. prism also depends on `hickory-proto` directly, and 0.26 rewrote the `Message` API its raw DNS paths use (`dns_raw.rs`, `dns_dnssec.rs`, `dns_trace.rs`). The spec reading found it before any code was written.
**What worked.** Pinning tests of prism's wire bytes and of a recorded referral's decode, written against 0.25 through prism's own functions (`pinned_query_wire_format_dnskey_do`, `pinned_decode_recorded_referral`), then the port; both stayed green unchanged across the bump.
**How to notice next time.** An effort estimate for a dependency bump that names one consumer, while `cargo tree -i <dep>` shows another crate depending on it directly.
