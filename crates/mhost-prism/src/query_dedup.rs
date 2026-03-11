//! Query deduplication: coalesce identical concurrent DNS queries into a single execution.
//!
//! When multiple SSE clients request the same query simultaneously, the first request
//! executes the DNS fan-out and broadcasts results; subsequent requests subscribe to
//! the broadcast and receive the same results without executing a duplicate query.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use ahash::AHasher;

use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::parser::{ParsedQuery, ServerSpec, Transport};

/// Capacity for the broadcast channel. Must be large enough that a slow receiver
/// doesn't miss events during a typical query (up to 10 batch + errors + 1 done).
/// If a receiver falls behind, it receives a `Lagged` error.
const BROADCAST_CAPACITY: usize = 64;

/// A deterministic hash of the query parameters that affect DNS execution.
///
/// Two queries with the same `QueryHash` will produce identical DNS results,
/// so one execution can serve both clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueryHash(u64);

impl QueryHash {
    /// Compute a deduplication hash from a parsed query.
    ///
    /// The hash is built from the normalized domain, sorted record types,
    /// sorted server specs, transport, and DNSSEC flag. Sorting ensures
    /// that token order in the query string does not affect the hash.
    pub fn from_query(parsed: &ParsedQuery) -> Self {
        let mut h = AHasher::default();

        // Domain is already lowercased by the parser.
        parsed.domain.hash(&mut h);

        // Sort record types by their string representation for determinism.
        let mut types: Vec<String> = parsed
            .record_types
            .iter()
            .map(|rt| rt.to_string())
            .collect();
        types.sort();
        types.hash(&mut h);

        // Sort servers by a stable string key.
        let mut servers: Vec<String> = parsed.servers.iter().map(server_key).collect();
        servers.sort();
        servers.hash(&mut h);

        // Transport and DNSSEC.
        transport_discriminant(parsed.transport).hash(&mut h);
        parsed.dnssec.hash(&mut h);

        Self(h.finish())
    }
}

/// Stable string key for a server spec, used for hashing.
fn server_key(spec: &ServerSpec) -> String {
    match spec {
        ServerSpec::Predefined(p) => format!("predefined:{}", p.to_string().to_ascii_lowercase()),
        ServerSpec::System => "system".to_string(),
        ServerSpec::Ip { addr, port } => format!("ip:{addr}:{port}"),
    }
}

/// Map transport to a stable discriminant for hashing.
fn transport_discriminant(t: Option<Transport>) -> u8 {
    match t {
        None => 0,
        Some(Transport::Udp) => 1,
        Some(Transport::Tcp) => 2,
        Some(Transport::Tls) => 3,
        Some(Transport::Https) => 4,
    }
}

/// SSE event data broadcast to deduplicated clients.
///
/// Contains pre-serialized JSON payloads so that subscribers can reconstruct
/// SSE events with their own unique request IDs without re-serializing DNS data.
#[derive(Debug, Clone)]
pub enum DedupEvent {
    /// A batch result for one record type. Contains the SSE event type name
    /// and pre-serialized JSON fields (excluding request_id).
    Batch {
        record_type: String,
        /// Pre-serialized `lookups` JSON value.
        lookups_json: serde_json::Value,
        completed: u32,
        total: u32,
    },
    /// Final done event.
    Done {
        total_queries: u32,
        duration_ms: u64,
        warnings: Vec<String>,
        transport: String,
        dnssec: bool,
    },
    /// An error event.
    Error { code: String, message: String },
}

/// Manages in-flight query deduplication.
///
/// Thread-safe and cheaply cloneable (wraps an `Arc`).
#[derive(Clone)]
pub struct QueryDedup {
    inner: Arc<DashMap<QueryHash, broadcast::Sender<DedupEvent>>>,
}

impl QueryDedup {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Try to join an in-flight query. Returns a receiver if an identical query
    /// is already executing, or `None` if this is a new query.
    pub fn try_join(&self, hash: QueryHash) -> Option<broadcast::Receiver<DedupEvent>> {
        self.inner.get(&hash).map(|entry| entry.subscribe())
    }

    /// Register a new query execution. Returns a sender for broadcasting events
    /// and an RAII guard that removes the entry on drop.
    pub fn register(&self, hash: QueryHash) -> (broadcast::Sender<DedupEvent>, DedupGuard) {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        self.inner.insert(hash, tx.clone());
        metrics::gauge!("prism_query_dedup_active").increment(1.0);

        let guard = DedupGuard {
            map: Arc::clone(&self.inner),
            hash,
        };
        (tx, guard)
    }
}

/// RAII guard that removes the dedup map entry when dropped.
///
/// This ensures cleanup even on error, timeout, or client disconnect.
pub struct DedupGuard {
    map: Arc<DashMap<QueryHash, broadcast::Sender<DedupEvent>>>,
    hash: QueryHash,
}

impl Drop for DedupGuard {
    fn drop(&mut self) {
        self.map.remove(&self.hash);
        metrics::gauge!("prism_query_dedup_active").decrement(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedQuery;
    use mhost::RecordType;
    use mhost::nameserver::predefined::PredefinedProvider;

    fn make_query(
        domain: &str,
        types: &[RecordType],
        servers: &[ServerSpec],
        transport: Option<Transport>,
        dnssec: bool,
    ) -> ParsedQuery {
        ParsedQuery {
            domain: domain.to_ascii_lowercase(),
            record_types: types.to_vec(),
            servers: servers.to_vec(),
            transport,
            dnssec,
            warnings: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // QueryHash determinism
    // -----------------------------------------------------------------------

    #[test]
    fn identical_queries_same_hash() {
        let q1 = make_query(
            "example.com",
            &[RecordType::A, RecordType::AAAA],
            &[ServerSpec::Predefined(PredefinedProvider::Cloudflare)],
            Some(Transport::Udp),
            false,
        );
        let q2 = make_query(
            "example.com",
            &[RecordType::A, RecordType::AAAA],
            &[ServerSpec::Predefined(PredefinedProvider::Cloudflare)],
            Some(Transport::Udp),
            false,
        );
        assert_eq!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_order_same_hash() {
        let q1 = make_query(
            "example.com",
            &[RecordType::A, RecordType::MX],
            &[
                ServerSpec::Predefined(PredefinedProvider::Cloudflare),
                ServerSpec::Predefined(PredefinedProvider::Google),
            ],
            None,
            false,
        );
        let q2 = make_query(
            "example.com",
            &[RecordType::MX, RecordType::A],
            &[
                ServerSpec::Predefined(PredefinedProvider::Google),
                ServerSpec::Predefined(PredefinedProvider::Cloudflare),
            ],
            None,
            false,
        );
        assert_eq!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_domain_different_hash() {
        let q1 = make_query("example.com", &[RecordType::A], &[], None, false);
        let q2 = make_query("example.org", &[RecordType::A], &[], None, false);
        assert_ne!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_types_different_hash() {
        let q1 = make_query("example.com", &[RecordType::A], &[], None, false);
        let q2 = make_query("example.com", &[RecordType::MX], &[], None, false);
        assert_ne!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_servers_different_hash() {
        let q1 = make_query(
            "example.com",
            &[RecordType::A],
            &[ServerSpec::Predefined(PredefinedProvider::Cloudflare)],
            None,
            false,
        );
        let q2 = make_query(
            "example.com",
            &[RecordType::A],
            &[ServerSpec::Predefined(PredefinedProvider::Google)],
            None,
            false,
        );
        assert_ne!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_transport_different_hash() {
        let q1 = make_query(
            "example.com",
            &[RecordType::A],
            &[],
            Some(Transport::Udp),
            false,
        );
        let q2 = make_query(
            "example.com",
            &[RecordType::A],
            &[],
            Some(Transport::Tls),
            false,
        );
        assert_ne!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn different_dnssec_different_hash() {
        let q1 = make_query("example.com", &[RecordType::A], &[], None, false);
        let q2 = make_query("example.com", &[RecordType::A], &[], None, true);
        assert_ne!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn warnings_do_not_affect_hash() {
        let mut q1 = make_query("example.com", &[RecordType::A], &[], None, false);
        let q2 = make_query("example.com", &[RecordType::A], &[], None, false);
        q1.warnings.push("some warning".to_string());
        assert_eq!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    #[test]
    fn case_insensitive_domain() {
        let q1 = make_query("example.com", &[RecordType::A], &[], None, false);
        let q2 = make_query("EXAMPLE.COM", &[RecordType::A], &[], None, false);
        assert_eq!(QueryHash::from_query(&q1), QueryHash::from_query(&q2));
    }

    // -----------------------------------------------------------------------
    // DedupGuard cleanup
    // -----------------------------------------------------------------------

    #[test]
    fn guard_removes_entry_on_drop() {
        let dedup = QueryDedup::new();
        let q = make_query("example.com", &[RecordType::A], &[], None, false);
        let hash = QueryHash::from_query(&q);

        let (_tx, guard) = dedup.register(hash);
        assert!(dedup.inner.contains_key(&hash));

        drop(guard);
        assert!(!dedup.inner.contains_key(&hash));
    }

    #[test]
    fn try_join_returns_none_for_new_query() {
        let dedup = QueryDedup::new();
        let q = make_query("example.com", &[RecordType::A], &[], None, false);
        let hash = QueryHash::from_query(&q);

        assert!(dedup.try_join(hash).is_none());
    }

    #[test]
    fn try_join_returns_receiver_for_in_flight_query() {
        let dedup = QueryDedup::new();
        let q = make_query("example.com", &[RecordType::A], &[], None, false);
        let hash = QueryHash::from_query(&q);

        let (_tx, _guard) = dedup.register(hash);
        assert!(dedup.try_join(hash).is_some());
    }

    #[tokio::test]
    async fn broadcast_reaches_subscriber() {
        let dedup = QueryDedup::new();
        let q = make_query("example.com", &[RecordType::A], &[], None, false);
        let hash = QueryHash::from_query(&q);

        let (tx, _guard) = dedup.register(hash);
        let mut rx = dedup.try_join(hash).unwrap();

        tx.send(DedupEvent::Batch {
            record_type: "A".to_string(),
            lookups_json: serde_json::Value::Null,
            completed: 1,
            total: 1,
        })
        .unwrap();
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DedupEvent::Batch { record_type, .. } if record_type == "A"));
    }
}
