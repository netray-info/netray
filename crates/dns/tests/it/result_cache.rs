// v2-dns-api Phase 4 review: a full v1 result cache drops its oldest result, never the one whose
// key a job just handed out (moka's default TinyLFU would refuse an unread newcomer).

use netray_dns::{DnsCachedJob, DnsCachedResult, DnsQueryResult, DnsResultCache};

fn entry(key: String) -> DnsCachedResult {
    DnsCachedResult {
        key,
        mode: "query".into(),
        query: "example.com".into(),
        result: DnsCachedJob::from(DnsQueryResult::default()),
    }
}

#[tokio::test]
async fn a_full_cache_keeps_the_newest_result() {
    let cache = DnsResultCache::new();
    for n in 0..1_000 {
        cache.insert(entry(format!("{n:012x}"))).await;
    }
    cache.run_pending_tasks().await;
    let newest = DnsResultCache::fresh_key();
    cache.insert(entry(newest.clone())).await;
    cache.run_pending_tasks().await;
    assert!(
        cache.get(&newest).await.is_some(),
        "the newest result was dropped"
    );
}
