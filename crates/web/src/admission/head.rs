//! The domain's history head and whether it is inside the cache window. Copied from lens's
//! `cache.rs` without the V1 holder.

use super::state::ApexState;

/// The key of a domain in the run register, the throttle and the history: lowercased, trimmed.
pub fn cache_key(domain: &str) -> String {
    domain.trim().to_lowercase()
}

/// The domain's history head and whether it is still inside the cache window.
pub struct HeadLookup {
    pub head: netray_store::Head,
    pub fresh: bool,
}

/// The domain's head entry; `None` without a history store, for an unknown domain, or when the
/// lookup failed. `fresh` is never true with the cache disabled.
pub async fn head(state: &ApexState, domain: &str) -> Option<HeadLookup> {
    let store = state.history_store.as_ref()?;
    let head = match store.head(&cache_key(domain)).await {
        Ok(head) => head?,
        Err(e) => {
            tracing::warn!(error = %e, domain = %domain, "history head lookup failed");
            return None;
        }
    };
    let age = netray_store::now_unix() - head.last_seen_at;
    let fresh = state.config.cache_enabled
        && age < i64::try_from(state.config.cache_window_seconds).unwrap_or(i64::MAX);
    Some(HeadLookup { head, fresh })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_lowercases_and_trims() {
        assert_eq!(cache_key("  Example.COM  "), "example.com");
        assert_eq!(cache_key("DNS.NETRAY.INFO"), "dns.netray.info");
    }

    #[test]
    fn cache_key_empty() {
        assert_eq!(cache_key(""), "");
        assert_eq!(cache_key("   "), "");
    }
}
