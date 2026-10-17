use std::collections::HashMap;
use std::sync::Arc;

use netray_store::Recorded;

use crate::admission::Published;
use crate::check::SectionError;
use crate::modules::BackendResult;
use crate::score::OverallScore;
use crate::state::AppState;

/// The V1 rendering of a history entry, held in the moka cache by entry id.
pub struct CachedResult {
    pub sections: HashMap<String, Result<BackendResult, SectionError>>,
    pub score: OverallScore,
    pub duration_ms: u64,
    /// Snapshot id the run created. Reused on cache hits so the snapshot URL stays
    /// available for the lifetime of the cache entry.
    pub snapshot_id: Option<String>,
}

/// Build the cache key for a domain: lowercased + trimmed.
pub fn cache_key(domain: &str) -> String {
    domain.trim().to_lowercase()
}

/// The domain's history head and whether it is still inside the cache window.
pub(crate) struct HeadLookup {
    pub head: netray_store::Head,
    pub fresh: bool,
}

/// The domain's head entry; `None` without a history store, for an unknown domain, or when
/// the lookup failed. `fresh` is never true with `[cache] enabled = false`.
pub(crate) async fn head(state: &AppState, domain: &str) -> Option<HeadLookup> {
    let store = state.history_store.as_ref()?;
    let head = match store.head(&cache_key(domain)).await {
        Ok(head) => head?,
        Err(e) => {
            tracing::warn!(error = %e, domain = %domain, "history head lookup failed");
            return None;
        }
    };
    let age = crate::history::now_unix() - head.last_seen_at;
    let fresh = state.config.cache.enabled
        && age < i64::try_from(state.config.cache.ttl_seconds).unwrap_or(i64::MAX);
    Some(HeadLookup { head, fresh })
}

/// Holds the V1 rendering of the run the history just recorded, keyed by its entry id.
pub(crate) async fn hold(state: &AppState, recorded: Option<&Recorded>, published: &Published) {
    let (Some(cache), Some(recorded)) = (&state.cache, recorded) else {
        return;
    };
    let (Recorded::First { id } | Recorded::Identical { id } | Recorded::Changed { id, .. }) =
        recorded
    else {
        return;
    };
    let entry = Arc::new(CachedResult {
        sections: published.output.sections.clone(),
        score: published.output.score.clone(),
        duration_ms: published.output.duration_ms,
        snapshot_id: published.snapshot_id.clone(),
    });
    cache.insert(id.clone(), entry).await;
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
