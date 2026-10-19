//! The DNS jobs: prism's five fan-outs (query, compare, authcompare, trace, dnssec) as
//! typed event streams, callable without a route, and the seam through which they reach DNS.

pub mod blocks;
pub mod events;
pub mod reference;
pub mod results;
pub mod schema;
pub mod summary;

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::Stream;
use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use hickory_proto::rr::{Name, RecordType as HickoryRecordType};
use mhost::RecordType;
use mhost::resolver::{Lookups, MultiQuery, Resolver, ResolverResult};
use netray_common::enrichment::EnrichmentClient;
use tokio::sync::{Semaphore, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::STREAM_TIMEOUT_SECS;
use crate::api::dnssec::DnssecRequest;
use crate::api::query::{
    PostQueryRequest, build_resolver_group, convert_post_body, effective_server_specs,
    extract_ips_from_cached_events, record_breaker_outcomes, target_keys_from_servers,
};
use crate::api::trace::TraceRequest;
use crate::circuit_breaker::{BreakerState, CircuitBreakerRegistry};
use crate::config::Config;
use crate::dns_dnssec;
use crate::dns_raw::{self, RawOutbound};
use crate::dns_trace;
use crate::error::ApiError;
use crate::parser::{ParsedQuery, Transport};
use crate::result_cache::CachedEvent;
use crate::security::{QueryPolicy, RateLimitState};

use events::{
    AuthLookup, AuthLookups, AuthQuery, AuthRecord, AuthResult, BatchLookups, WireDuration,
};
pub use events::{
    DnsAuthCompareDone, DnsAuthCompareEvent, DnsAuthCompareResult, DnsBatch, DnsChainEvent,
    DnsCompareDone, DnsCompareEvent, DnsCompareResult, DnsDnssecDone, DnsDnssecEvent,
    DnsDnssecResult, DnsEnrichmentEvent, DnsErrorEvent, DnsHopEvent, DnsJobEvent, DnsQueryDone,
    DnsQueryEvent, DnsQueryResult, DnsTraceDone, DnsTraceEvent, DnsTraceResult,
};
pub use reference::{
    DnsParseAnswer, DnsParseRequest, DnsParsedQuery, DnsRecordTypes, DnsServerCatalogue,
    DnsServerGroup, parse_answer, record_types, server_catalogue,
};
pub use results::{DnsCachedJob, DnsCachedResult, DnsResultCache, is_key};
pub use summary::{
    Agreement, AnswerOutcome, DnsJobSummary, DnsRecordDetail, DnsServerAnswer, DnsServerSummary,
    DnsTypeComparison,
};

// Cost charged against the per-IP and global limiters per trace or dnssec job: trace queries
// root/TLD/auth servers (public infrastructure), so there is no per-target charge and a flat
// 16 tokens, as check.
const TRACE_COST: u32 = 16;
const DNSSEC_COST: u32 = 16;

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/// How the jobs reach DNS. Production: mhost's resolvers and `RawOutbound::production()`.
#[derive(Clone)]
pub struct DnsBackend {
    pub(crate) lookup: Arc<dyn ResolverLookup>,
    pub(crate) raw: RawOutbound,
}

impl DnsBackend {
    pub fn production() -> Self {
        Self {
            lookup: Arc::new(MhostLookup),
            raw: RawOutbound::production(),
        }
    }
}

/// Runs one multi-query against one resolver.
pub(crate) trait ResolverLookup: Send + Sync + 'static {
    fn lookup(
        &self,
        resolver: &Resolver,
        query: MultiQuery,
    ) -> BoxFuture<'static, ResolverResult<Lookups>>;
}

struct MhostLookup;

impl ResolverLookup for MhostLookup {
    fn lookup(
        &self,
        resolver: &Resolver,
        query: MultiQuery,
    ) -> BoxFuture<'static, ResolverResult<Lookups>> {
        let resolver = resolver.clone();
        Box::pin(async move { resolver.lookup(query).await })
    }
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// Why a job did not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DnsRefusal {
    /// A domain, record type, server or query the policy refuses.
    #[error("{0}")]
    InvalidInput(String),
    /// A server address in a blocked range.
    #[error("{0}")]
    Blocked(String),
    #[error("rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },
    /// No resolver could be built.
    #[error("{0}")]
    Unavailable(String),
}

impl From<ApiError> for DnsRefusal {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::InvalidDomain(_)
            | ApiError::InvalidRecordType(_)
            | ApiError::InvalidServer(_)
            | ApiError::ParseError(_)
            | ApiError::AmbiguousInput
            | ApiError::SystemResolversDisabled
            | ApiError::ArbitraryServersDisabled
            | ApiError::TooManyRecordTypes { .. }
            | ApiError::TooManyServers { .. } => Self::InvalidInput(e.to_string()),
            ApiError::BlockedTargetIp { .. } => Self::Blocked(e.to_string()),
            ApiError::RateLimited {
                retry_after_secs, ..
            } => Self::RateLimited { retry_after_secs },
            ApiError::ResolverError(_) => Self::Unavailable(e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// The job handle
// ---------------------------------------------------------------------------

/// A running job: its events as a stream. Dropping it does not abort the job; the job ends on
/// its next send.
pub struct DnsJob<E> {
    events: ReceiverStream<E>,
}

impl<E> DnsJob<E> {
    fn new(rx: mpsc::Receiver<E>) -> Self {
        Self {
            events: ReceiverStream::new(rx),
        }
    }
}

impl<E> Stream for DnsJob<E> {
    type Item = E;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<E>> {
        Pin::new(&mut self.get_mut().events).poll_next(cx)
    }
}

impl<E: DnsJobEvent> DnsJob<E> {
    /// Reads the job to its end. `done` stays `None` when the stream ends without one.
    pub async fn collect(mut self) -> E::Collected {
        let mut collected = E::Collected::default();
        while let Some(event) = self.next().await {
            event.collect_into(&mut collected);
        }
        collected
    }
}

// ---------------------------------------------------------------------------
// prism's metrics
// ---------------------------------------------------------------------------

/// prism's `prism_*` series; every call is a no-op unless the jobs run for prism.
#[derive(Clone, Copy)]
struct PrismMetrics(bool);

impl PrismMetrics {
    fn active(self, gauge: &'static str, delta: f64) {
        if self.0 {
            let g = metrics::gauge!(gauge);
            if delta >= 0.0 {
                g.increment(delta);
            } else {
                g.decrement(-delta);
            }
        }
    }

    fn query(self, status: &'static str, record_type: String) {
        if self.0 {
            metrics::counter!(
                "prism_queries_total",
                "status" => status,
                "record_type" => record_type
            )
            .increment(1);
        }
    }

    fn endpoint(self, endpoint: &'static str, status: &'static str) {
        if self.0 {
            metrics::counter!("prism_queries_total", "endpoint" => endpoint, "status" => status)
                .increment(1);
        }
    }

    fn duration(self, histogram: &'static str, elapsed: Duration) {
        if self.0 {
            metrics::histogram!(histogram).record(elapsed.as_secs_f64());
        }
    }
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

/// prism's fan-outs as jobs over a configuration, circuit breakers, the query semaphore and a
/// backend.
#[derive(Clone)]
pub struct DnsJobs {
    config: Arc<Config>,
    circuit_breakers: Arc<CircuitBreakerRegistry>,
    query_semaphore: Arc<Semaphore>,
    ip_enrichment: Option<Arc<EnrichmentClient>>,
    backend: DnsBackend,
    prism_metrics: bool,
}

async fn send_error<E: From<DnsErrorEvent>>(tx: &mpsc::Sender<E>, code: &str, message: &str) {
    let _ = tx.send(DnsErrorEvent::new(code, message).into()).await;
}

/// The enrichment of `ips`, when there is any.
async fn enrichment_event(
    svc: &EnrichmentClient,
    ips: &[IpAddr],
    request_id: &str,
) -> Option<DnsEnrichmentEvent> {
    if ips.is_empty() {
        return None;
    }
    let enrichments = svc.lookup_batch(ips, Some(request_id)).await;
    if enrichments.is_empty() {
        return None;
    }
    Some(DnsEnrichmentEvent {
        request_id: request_id.to_owned(),
        enrichments: enrichments
            .into_iter()
            .map(|(ip, info)| (ip.to_string(), info))
            .collect(),
    })
}

/// The mnemonic of a response code (`SERVFAIL`, `REFUSED`), as mhost prints the recursive ones.
fn rcode_mnemonic(rcode: hickory_proto::op::ResponseCode) -> String {
    match u16::from(rcode) {
        0 => "NOERROR".to_owned(),
        1 => "FORMERR".to_owned(),
        2 => "SERVFAIL".to_owned(),
        3 => "NXDOMAIN".to_owned(),
        4 => "NOTIMP".to_owned(),
        5 => "REFUSED".to_owned(),
        6 => "YXDOMAIN".to_owned(),
        7 => "YXRRSET".to_owned(),
        8 => "NXRRSET".to_owned(),
        9 => "NOTAUTH".to_owned(),
        10 => "NOTZONE".to_owned(),
        n => format!("RCODE{n}"),
    }
}

fn transport_label(t: Transport) -> &'static str {
    match t {
        Transport::Udp => "udp",
        Transport::Tcp => "tcp",
        Transport::Tls => "tls",
        Transport::Https => "https",
    }
}

// All four transports to compare.
const ALL_TRANSPORTS: [Transport; 4] = [
    Transport::Udp,
    Transport::Tcp,
    Transport::Tls,
    Transport::Https,
];

impl DnsJobs {
    pub fn new(
        config: Arc<Config>,
        circuit_breakers: Arc<CircuitBreakerRegistry>,
        query_semaphore: Arc<Semaphore>,
        ip_enrichment: Option<Arc<EnrichmentClient>>,
        backend: DnsBackend,
    ) -> Self {
        Self {
            config,
            circuit_breakers,
            query_semaphore,
            ip_enrichment,
            backend,
            prism_metrics: false,
        }
    }

    /// Whether `@system` resolvers are allowed by the configuration.
    pub fn allows_system_resolvers(&self) -> bool {
        self.config.dns.allow_system_resolvers
    }

    /// Emit prism's `prism_*` metrics from the jobs.
    pub fn with_prism_metrics(mut self) -> Self {
        self.prism_metrics = true;
        self
    }

    /// A query job over a structured body.
    pub async fn query(
        &self,
        req: &PostQueryRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsQueryEvent>, ApiError> {
        let parsed = convert_post_body(req.clone())?;
        self.query_parsed(parsed, client, limits, request_id).await
    }

    /// A query job over a parsed query (prism's `GET /api/query` parses a query string).
    pub async fn query_parsed(
        &self,
        mut parsed: ParsedQuery,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsQueryEvent>, ApiError> {
        // Validate against query policy.
        let policy = QueryPolicy::new(&self.config);
        policy.validate(&parsed)?;

        let timeout_secs = self.config.limits.max_timeout_secs;
        let timeout = Duration::from_secs(timeout_secs);

        // When DNSSEC mode is requested, ensure DNSKEY and DS are queried so
        // DNSSEC records are visible in results. This is additive — explicit
        // record types from the user are preserved.
        if parsed.dnssec {
            for rt in [RecordType::DNSKEY, RecordType::DS] {
                if !parsed.record_types.contains(&rt) {
                    parsed.record_types.push(rt);
                }
            }
        }

        // Rate limiting: compute cost and check budget before building resolvers.
        // Total cost = record_types × servers (SDD §8.2 query cost model).
        // Per-target cost = record_types (each target is only charged its share).
        let effective_servers = effective_server_specs(&parsed, &self.config);
        let target_keys = target_keys_from_servers(&effective_servers);
        let num_types = parsed.record_types.len() as u32;
        let num_servers = effective_servers.len().max(1) as u32;
        let total_cost = num_types * num_servers;
        let stream_guard = limits.check_query_cost(client, &target_keys, total_cost, num_types)?;

        // Build resolver group and parallel circuit breaker keys.
        let (resolver_group, breaker_keys) =
            build_resolver_group(&parsed, &self.config, timeout).await?;

        // Extract individual resolvers from the group. We call Resolver::lookup()
        // per record type (which is Send — it uses tokio::task::spawn internally)
        // rather than ResolverGroup::lookup() (which is !Send due to ThreadRng in
        // the Uni mode path).
        let resolvers: Vec<Resolver> = resolver_group.resolvers().to_vec();

        let (tx, rx) = mpsc::channel::<DnsQueryEvent>(32);

        let warnings = parsed.warnings.clone();
        let record_types = parsed.record_types.clone();
        let domain = parsed.domain.clone();
        let rid = request_id;
        let circuit_breakers = self.circuit_breakers.clone();
        let query_semaphore = self.query_semaphore.clone();
        let backend_lookup = Arc::clone(&self.backend.lookup);
        let transport_name = match parsed.transport {
            Some(Transport::Udp) => "udp",
            Some(Transport::Tcp) => "tcp",
            Some(Transport::Tls) => "tls",
            Some(Transport::Https) => "https",
            None => "udp",
        }
        .to_owned();
        let dnssec_requested = parsed.dnssec;
        let enrichment_svc = self.ip_enrichment.clone();
        let prism = PrismMetrics(self.prism_metrics);

        tokio::spawn(async move {
            // Hold stream guard for the lifetime of this task so the active stream
            // count is decremented when the job ends.
            let _stream_guard = stream_guard;
            prism.active("prism_active_queries", 1.0);
            let start = Instant::now();
            let mut batch_events: Vec<CachedEvent> = Vec::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
            let total = record_types.len() as u32;
            // std::sync::Mutex is safe here: the lock is never held across an .await point.
            let degraded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

            // Build one future per record type. Each future fans out across all
            // resolvers concurrently and returns (RecordType, Lookups, had_error).
            // FuturesUnordered drives them all in parallel and yields each result
            // as it completes, so the total wall-clock time is bounded by the
            // slowest single-type query rather than the sum.
            let futs: FuturesUnordered<_> = record_types
                .iter()
                .map(|rt| {
                    let rt = *rt;
                    let domain = domain.clone();
                    let resolvers = resolvers.clone();
                    let breaker_keys = breaker_keys.clone();
                    let circuit_breakers = Arc::clone(&circuit_breakers);
                    let tx_err = tx.clone();
                    let degraded = Arc::clone(&degraded);
                    let semaphore = Arc::clone(&query_semaphore);
                    let backend_lookup = Arc::clone(&backend_lookup);
                    async move {
                        let query = match MultiQuery::single(domain.as_str(), rt) {
                            Ok(q) => q,
                            Err(e) => {
                                tracing::warn!(domain = %domain, record_type = %rt, error = %e, "query build failed");
                                send_error(&tx_err, "RESOLVER_ERROR", &e.to_string()).await;
                                return (rt, Lookups::empty(), true);
                            }
                        };

                        // Circuit breaker pre-check and per-resolver spawn.
                        let mut handles = Vec::with_capacity(resolvers.len());
                        for (idx, resolver) in resolvers.iter().enumerate() {
                            let breaker_key = &breaker_keys[idx];
                            if let Err(BreakerState::Open) = circuit_breakers.check(breaker_key) {
                                tracing::warn!(domain = %domain, record_type = %rt, provider = %breaker_key, "circuit breaker open, skipping provider");
                                send_error(
                                    &tx_err,
                                    "PROVIDER_DEGRADED",
                                    &format!(
                                        "circuit breaker open for {breaker_key} ({rt}), skipping"
                                    ),
                                )
                                .await;
                                if let Ok(mut d) = degraded.lock()
                                    && !d.contains(breaker_key)
                                {
                                    d.push(breaker_key.clone());
                                }
                                continue;
                            }
                            let r = resolver.clone();
                            let q = query.clone();
                            let sem = Arc::clone(&semaphore);
                            let lookup = Arc::clone(&backend_lookup);
                            handles.push(tokio::spawn(async move {
                                let _permit = sem.acquire().await;
                                lookup.lookup(&r, q).await
                            }));
                        }

                        let mut merged = Lookups::empty();
                        let mut had_error = false;
                        for handle in handles {
                            match handle.await {
                                Ok(Ok(lookups)) => {
                                    record_breaker_outcomes(&circuit_breakers, &lookups);
                                    merged = merged.merge(lookups);
                                }
                                Ok(Err(e)) => {
                                    had_error = true;
                                    tracing::warn!(domain = %domain, record_type = %rt, error = %e, "resolver lookup failed");
                                    send_error(&tx_err, "RESOLVER_ERROR", &e.to_string()).await;
                                }
                                Err(e) => {
                                    had_error = true;
                                    tracing::error!(domain = %domain, record_type = %rt, error = %e, "resolver task panicked");
                                    send_error(&tx_err, "INTERNAL_ERROR", &e.to_string()).await;
                                }
                            }
                        }

                        (rt, merged, had_error)
                    }
                })
                .collect();

            tokio::pin!(futs);
            let mut completed: u32 = 0;
            let mut timed_out = false;

            loop {
                tokio::select! {
                    maybe = futs.next() => {
                        match maybe {
                            None => break, // all record types completed
                            Some((rt, merged, had_error)) => {
                                completed += 1;
                                let status = if had_error { "error" } else { "ok" };
                                prism.query(status, rt.to_string());

                                let batch = DnsBatch {
                                    request_id: rid.clone(),
                                    record_type: rt.to_string(),
                                    lookups: BatchLookups::Resolved(merged),
                                    completed,
                                    total,
                                    transport: None,
                                    source: None,
                                };
                                if enrichment_svc.is_some() {
                                    batch_events.push(CachedEvent {
                                        event_type: "batch".to_owned(),
                                        data: batch.cache_value(),
                                    });
                                }
                                if tx.send(DnsQueryEvent::Batch(batch)).await.is_err() {
                                    // Receiver dropped.
                                    prism.active("prism_active_queries", -1.0);
                                    return;
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        send_error(&tx, "STREAM_TIMEOUT", "stream deadline exceeded").await;
                        break;
                    }
                }
            }

            if !timed_out {
                // Send enrichment event before done (non-blocking, graceful).
                if let Some(ref svc) = enrichment_svc {
                    let ips = extract_ips_from_cached_events(&batch_events);
                    if let Some(e) = enrichment_event(svc, &ips, &rid).await {
                        let _ = tx.send(DnsQueryEvent::Enrichment(e)).await;
                    }
                }

                let elapsed = start.elapsed();
                tracing::info!(
                    request_id = %rid,
                    domain = %domain,
                    duration_ms = elapsed.as_millis(),
                    "query completed"
                );
                let degraded_providers = degraded.lock().map(|d| d.clone()).unwrap_or_default();

                let done = DnsQueryDone {
                    request_id: rid,
                    total_queries: total,
                    duration_ms: elapsed.as_millis() as u64,
                    warnings,
                    transport: transport_name,
                    dnssec: dnssec_requested,
                    degraded_providers,
                    cache_key: None,
                };
                let _ = tx.send(DnsQueryEvent::Done(done)).await;

                prism.duration("prism_query_duration_seconds", start.elapsed());
            }

            prism.active("prism_active_queries", -1.0);
        });

        Ok(DnsJob::new(rx))
    }

    /// A compare job: the query over all four transports.
    pub async fn compare(
        &self,
        req: &PostQueryRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsCompareEvent>, ApiError> {
        let parsed = convert_post_body(PostQueryRequest {
            transport: None,
            ..req.clone()
        })?;
        self.compare_parsed(parsed, client, limits, request_id)
            .await
    }

    /// A compare job over a parsed query; any transport override is cleared, the job iterates
    /// all transports itself.
    pub async fn compare_parsed(
        &self,
        mut parsed: ParsedQuery,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsCompareEvent>, ApiError> {
        parsed.transport = None;

        let policy = QueryPolicy::new(&self.config);
        policy.validate(&parsed)?;

        let timeout = Duration::from_secs(self.config.limits.max_timeout_secs);

        // Probe which transports are available (have at least one resolver).
        let mut available_transports: Vec<Transport> = Vec::new();
        for t in ALL_TRANSPORTS {
            let mut probe = parsed.clone();
            probe.transport = Some(t);
            match build_resolver_group(&probe, &self.config, timeout).await {
                Ok((rg, _)) if !rg.resolvers().is_empty() => {
                    available_transports.push(t);
                }
                Err(e @ ApiError::BlockedTargetIp { .. }) => return Err(e),
                _ => {}
            }
        }

        if available_transports.is_empty() {
            return Err(ApiError::ResolverError(
                "no transports available for the specified servers".into(),
            ));
        }

        // Rate limiting: cost = record_types * servers * available_transports.
        let effective_servers = effective_server_specs(&parsed, &self.config);
        let target_keys = target_keys_from_servers(&effective_servers);
        let num_types = parsed.record_types.len() as u32;
        let num_servers = effective_servers.len().max(1) as u32;
        let num_transports = available_transports.len() as u32;
        let total_cost = num_types * num_servers * num_transports;
        let stream_guard = limits.check_query_cost(client, &target_keys, total_cost, num_types)?;

        // DNSSEC augmentation.
        if parsed.dnssec {
            for rt in [RecordType::DNSKEY, RecordType::DS] {
                if !parsed.record_types.contains(&rt) {
                    parsed.record_types.push(rt);
                }
            }
        }

        let (tx, rx) = mpsc::channel::<DnsCompareEvent>(64);
        let rid = request_id;
        let warnings = parsed.warnings.clone();
        let record_types = parsed.record_types.clone();
        let domain = parsed.domain.clone();
        let circuit_breakers = self.circuit_breakers.clone();
        let enrichment_svc = self.ip_enrichment.clone();
        let config = self.config.clone();
        let query_semaphore = self.query_semaphore.clone();
        let backend_lookup = Arc::clone(&self.backend.lookup);
        let total_batches = (record_types.len() as u32) * num_transports;
        let prism = PrismMetrics(self.prism_metrics);

        tokio::spawn(async move {
            let _stream_guard = stream_guard;
            prism.active("prism_active_compares", 1.0);
            let start = Instant::now();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
            let mut batch_events: Vec<CachedEvent> = Vec::new();
            let mut completed: u32 = 0;
            let mut actual_transports: Vec<String> = Vec::new();
            let mut all_warnings = warnings;

            // For each available transport, build resolvers and run all record types.
            // Use FuturesUnordered to run transports concurrently.
            type TransportResult = (Transport, Vec<(RecordType, Lookups, bool)>);
            let transport_futs: FuturesUnordered<
                Pin<Box<dyn std::future::Future<Output = TransportResult> + Send>>,
            > = FuturesUnordered::new();

            for t in &available_transports {
                let t = *t;
                let mut probe = parsed.clone();
                probe.transport = Some(t);
                let config = config.clone();
                let circuit_breakers = Arc::clone(&circuit_breakers);
                let tx_err = tx.clone();
                let record_types = record_types.clone();
                let domain = domain.clone();
                let query_semaphore = Arc::clone(&query_semaphore);
                let backend_lookup = Arc::clone(&backend_lookup);

                transport_futs.push(Box::pin(async move {
                    let (resolver_group, breaker_keys) =
                        match build_resolver_group(&probe, &config, timeout).await {
                            Ok(rg) => rg,
                            Err(e) => {
                                send_error(&tx_err, "RESOLVER_ERROR", &e.to_string()).await;
                                return (t, Vec::new());
                            }
                        };
                    let resolvers: Vec<Resolver> = resolver_group.resolvers().to_vec();
                    if resolvers.is_empty() {
                        send_error(
                            &tx_err,
                            "TRANSPORT_UNAVAILABLE",
                            &format!("no resolvers support {}", transport_label(t)),
                        )
                        .await;
                        return (t, Vec::new());
                    }

                    // Fan out per record type.
                    let rt_futs: FuturesUnordered<_> = record_types
                        .iter()
                        .map(|rt| {
                            let rt = *rt;
                            let domain = domain.clone();
                            let resolvers = resolvers.clone();
                            let breaker_keys = breaker_keys.clone();
                            let circuit_breakers = Arc::clone(&circuit_breakers);
                            let tx_err = tx_err.clone();
                            let semaphore = Arc::clone(&query_semaphore);
                            let backend_lookup = Arc::clone(&backend_lookup);
                            async move {
                                let query = match MultiQuery::single(domain.as_str(), rt) {
                                    Ok(q) => q,
                                    Err(e) => {
                                        send_error(&tx_err, "RESOLVER_ERROR", &e.to_string())
                                            .await;
                                        return (rt, Lookups::empty(), true);
                                    }
                                };

                                let mut handles = Vec::with_capacity(resolvers.len());
                                for (idx, resolver) in resolvers.iter().enumerate() {
                                    let breaker_key = &breaker_keys[idx];
                                    if let Err(BreakerState::Open) =
                                        circuit_breakers.check(breaker_key)
                                    {
                                        continue;
                                    }
                                    let r = resolver.clone();
                                    let q = query.clone();
                                    let sem = Arc::clone(&semaphore);
                                    let lookup = Arc::clone(&backend_lookup);
                                    handles.push(tokio::spawn(async move {
                                        let _permit = sem.acquire().await;
                                        lookup.lookup(&r, q).await
                                    }));
                                }

                                let mut merged = Lookups::empty();
                                let mut had_error = false;
                                for handle in handles {
                                    match handle.await {
                                        Ok(Ok(lookups)) => {
                                            record_breaker_outcomes(&circuit_breakers, &lookups);
                                            merged = merged.merge(lookups);
                                        }
                                        Ok(Err(e)) => {
                                            had_error = true;
                                            tracing::warn!(domain = %domain, record_type = %rt, transport = transport_label(t), error = %e, "resolver lookup failed");
                                            send_error(&tx_err, "RESOLVER_ERROR", &e.to_string())
                                                .await;
                                        }
                                        Err(e) => {
                                            had_error = true;
                                            tracing::error!(domain = %domain, record_type = %rt, transport = transport_label(t), error = %e, "resolver task panicked");
                                            send_error(&tx_err, "INTERNAL_ERROR", &e.to_string())
                                                .await;
                                        }
                                    }
                                }
                                (rt, merged, had_error)
                            }
                        })
                        .collect();

                    let results: Vec<_> = rt_futs.collect().await;
                    (t, results)
                }));
            }

            tokio::pin!(transport_futs);
            let mut timed_out = false;

            loop {
                tokio::select! {
                    maybe = transport_futs.next() => {
                        match maybe {
                            None => break,
                            Some((transport, results)) => {
                                if results.is_empty() {
                                    all_warnings.push(format!("no resolvers available for {}", transport_label(transport)));
                                    continue;
                                }
                                actual_transports.push(transport_label(transport).to_owned());

                                for (rt, merged, _had_error) in results {
                                    completed += 1;
                                    let batch = DnsBatch {
                                        request_id: rid.clone(),
                                        record_type: rt.to_string(),
                                        lookups: BatchLookups::Resolved(merged),
                                        completed,
                                        total: total_batches,
                                        transport: Some(transport_label(transport).to_owned()),
                                        source: None,
                                    };
                                    if enrichment_svc.is_some() {
                                        batch_events.push(CachedEvent {
                                            event_type: "batch".to_owned(),
                                            data: batch.cache_value(),
                                        });
                                    }
                                    if tx.send(DnsCompareEvent::Batch(batch)).await.is_err() {
                                        prism.endpoint("compare", "error");
                                        prism.active("prism_active_compares", -1.0);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        send_error(&tx, "STREAM_TIMEOUT", "stream deadline exceeded").await;
                        prism.endpoint("compare", "error");
                        break;
                    }
                }
            }

            if !timed_out {
                // Enrichment.
                if let Some(ref svc) = enrichment_svc {
                    let ips = extract_ips_from_cached_events(&batch_events);
                    if let Some(e) = enrichment_event(svc, &ips, &rid).await {
                        let _ = tx.send(DnsCompareEvent::Enrichment(e)).await;
                    }
                }

                let elapsed = start.elapsed();
                tracing::info!(
                    request_id = %rid,
                    domain = %domain,
                    duration_ms = elapsed.as_millis(),
                    transports = ?actual_transports,
                    "compare completed"
                );

                actual_transports.sort();
                let done = DnsCompareDone {
                    request_id: rid,
                    total_queries: completed,
                    duration_ms: elapsed.as_millis() as u64,
                    warnings: all_warnings,
                    transports: actual_transports,
                    cache_key: None,
                };
                let _ = tx.send(DnsCompareEvent::Done(done)).await;

                prism.endpoint("compare", "ok");
                prism.duration("prism_compare_duration_seconds", elapsed);
            }

            prism.active("prism_active_compares", -1.0);
        });

        Ok(DnsJob::new(rx))
    }

    /// An authcompare job: the authoritative servers against the recursive resolvers.
    pub async fn authcompare(
        &self,
        req: &PostQueryRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsAuthCompareEvent>, ApiError> {
        let parsed = convert_post_body(req.clone())?;
        self.authcompare_parsed(parsed, client, limits, request_id)
            .await
    }

    /// An authcompare job over a parsed query.
    pub async fn authcompare_parsed(
        &self,
        parsed: ParsedQuery,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsAuthCompareEvent>, ApiError> {
        let policy = QueryPolicy::new(&self.config);
        policy.validate(&parsed)?;

        let timeout = Duration::from_secs(self.config.limits.max_timeout_secs);

        // Rate limiting: cost = record_types * servers * 2 (auth + recursive) + 16 (NS discovery).
        let effective_servers = effective_server_specs(&parsed, &self.config);
        let target_keys = target_keys_from_servers(&effective_servers);
        let num_types = parsed.record_types.len() as u32;
        let num_servers = effective_servers.len().max(1) as u32;
        let total_cost = num_types * num_servers * 2 + 16;
        let stream_guard = limits.check_query_cost(client, &target_keys, total_cost, num_types)?;

        let domain = parsed.domain.clone();
        let trace_timeout = Duration::from_secs(self.config.trace.query_timeout_secs);

        // Phase 1: Discover authoritative NS (before spawn — ResolverGroup is !Send)
        let domain_name =
            Name::from_ascii(&domain).map_err(|e| ApiError::InvalidDomain(e.to_string()))?;

        let (_resolver_group, _breaker_keys) =
            build_resolver_group(&parsed, &self.config, timeout).await?;

        // Extract individual resolvers (Send) for the recursive branch.
        let resolvers: Vec<Resolver> = _resolver_group.resolvers().to_vec();

        let (tx, rx) = mpsc::channel::<DnsAuthCompareEvent>(64);
        let rid = request_id;
        let warnings = parsed.warnings.clone();
        let record_types: Vec<RecordType> = parsed.record_types.clone();
        let circuit_breakers = self.circuit_breakers.clone();
        let enrichment_svc = self.ip_enrichment.clone();
        let query_semaphore = self.query_semaphore.clone();
        let backend_lookup = Arc::clone(&self.backend.lookup);
        let raw_outbound = self.backend.raw.clone();
        let prism = PrismMetrics(self.prism_metrics);

        tokio::spawn(async move {
            let _stream_guard = stream_guard;
            prism.active("prism_active_authcompares", 1.0);
            let start = Instant::now();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(STREAM_TIMEOUT_SECS);
            let mut batch_events: Vec<CachedEvent> = Vec::new();
            let mut completed: u32 = 0;
            let mut all_warnings = warnings;

            // Phase 1: Discover authoritative NS (inside spawned task)

            // Query NS records using individual resolvers (Send-safe).
            let ns_query = match MultiQuery::single(domain.as_str(), RecordType::NS) {
                Ok(q) => q,
                Err(e) => {
                    send_error(&tx, "RESOLVER_ERROR", &e.to_string()).await;
                    prism.active("prism_active_authcompares", -1.0);
                    return;
                }
            };

            let mut ns_lookups = Lookups::empty();
            for resolver in &resolvers {
                let r = resolver.clone();
                let q = ns_query.clone();
                let lookup = Arc::clone(&backend_lookup);
                match tokio::spawn(async move { lookup.lookup(&r, q).await }).await {
                    Ok(Ok(lookups)) => {
                        ns_lookups = ns_lookups.merge(lookups);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(domain = %domain, error = %e, "NS lookup failed during authcompare discovery");
                        send_error(&tx, "RESOLVER_ERROR", &e.to_string()).await;
                    }
                    Err(e) => {
                        tracing::error!(domain = %domain, error = %e, "NS lookup task panicked during authcompare discovery");
                        send_error(&tx, "INTERNAL_ERROR", &e.to_string()).await;
                    }
                }
            }

            // Extract NS hostnames from response by serializing to JSON.
            let mut ns_names: Vec<String> = Vec::new();
            if let Ok(lookups_json) = serde_json::to_value(&ns_lookups)
                && let Some(lookups_arr) = lookups_json.get("lookups").and_then(|v| v.as_array())
            {
                for lookup in lookups_arr {
                    if let Some(resp) = lookup.get("result").and_then(|r| r.get("Response"))
                        && let Some(records) = resp.get("records").and_then(|r| r.as_array())
                    {
                        for record in records {
                            if let Some(ns_val) = record
                                .get("data")
                                .and_then(|d| d.get("NS"))
                                .and_then(|v| v.as_str())
                            {
                                let name = ns_val.to_string();
                                if !ns_names.contains(&name) {
                                    ns_names.push(name);
                                }
                            }
                        }
                    }
                }
            }

            if ns_names.is_empty() {
                all_warnings.push("no NS records found — auth comparison unavailable".to_owned());
                send_error(
                    &tx,
                    "NO_NS_RECORDS",
                    "could not discover authoritative nameservers",
                )
                .await;
            }

            // Resolve NS hostnames to IPs through the outbound context.
            let auth =
                crate::api::authcompare::resolve_auth_servers(&raw_outbound, &ns_names).await;
            all_warnings.extend(auth.warnings);
            let auth_servers = auth.servers;
            let auth_server_labels = auth.labels;

            let has_auth = !auth_servers.is_empty();
            let total_batches = if has_auth {
                record_types.len() as u32 * 2
            } else {
                record_types.len() as u32
            };

            // Phase 2: Parallel fan-out (auth + recursive)

            let mut timed_out = false;

            type BranchResult = Vec<(RecordType, BatchLookups, &'static str)>;
            let futs: FuturesUnordered<
                Pin<Box<dyn std::future::Future<Output = BranchResult> + Send>>,
            > = FuturesUnordered::new();

            // Recursive branch.
            {
                let resolvers = resolvers;
                let record_types = record_types.clone();
                let domain = domain.clone();
                let circuit_breakers = Arc::clone(&circuit_breakers);
                let tx_err = tx.clone();
                let query_semaphore = Arc::clone(&query_semaphore);
                let backend_lookup = Arc::clone(&backend_lookup);

                futs.push(Box::pin(async move {
                    let rt_futs: FuturesUnordered<_> = record_types
                        .iter()
                        .map(|rt| {
                            let rt = *rt;
                            let domain = domain.clone();
                            let resolvers = resolvers.clone();
                            let circuit_breakers = Arc::clone(&circuit_breakers);
                            let tx_err = tx_err.clone();
                            let semaphore = Arc::clone(&query_semaphore);
                            let backend_lookup = Arc::clone(&backend_lookup);
                            async move {
                                let query = match MultiQuery::single(domain.as_str(), rt) {
                                    Ok(q) => q,
                                    Err(e) => {
                                        send_error(&tx_err, "RESOLVER_ERROR", &e.to_string())
                                            .await;
                                        return (
                                            rt,
                                            BatchLookups::Resolved(Lookups::empty()),
                                            "recursive",
                                        );
                                    }
                                };

                                let mut handles = Vec::with_capacity(resolvers.len());
                                for resolver in &resolvers {
                                    let r = resolver.clone();
                                    let q = query.clone();
                                    let sem = Arc::clone(&semaphore);
                                    let lookup = Arc::clone(&backend_lookup);
                                    handles.push(tokio::spawn(async move {
                                        let _permit = sem.acquire().await;
                                        lookup.lookup(&r, q).await
                                    }));
                                }

                                let mut merged = Lookups::empty();
                                for handle in handles {
                                    match handle.await {
                                        Ok(Ok(lookups)) => {
                                            record_breaker_outcomes(&circuit_breakers, &lookups);
                                            merged = merged.merge(lookups);
                                        }
                                        Ok(Err(e)) => {
                                            tracing::warn!(domain = %domain, record_type = %rt, error = %e, "recursive resolver lookup failed");
                                            send_error(&tx_err, "RESOLVER_ERROR", &e.to_string())
                                                .await;
                                        }
                                        Err(e) => {
                                            tracing::error!(domain = %domain, record_type = %rt, error = %e, "recursive resolver task panicked");
                                            send_error(&tx_err, "INTERNAL_ERROR", &e.to_string())
                                                .await;
                                        }
                                    }
                                }
                                (rt, BatchLookups::Resolved(merged), "recursive")
                            }
                        })
                        .collect();

                    rt_futs.collect().await
                }));
            }

            // Authoritative branch (only if we have auth servers).
            if has_auth {
                let auth_servers = auth_servers.clone();
                let raw_outbound = raw_outbound.clone();
                let record_types = record_types.clone();
                let domain_name = domain_name.clone();
                let tx_err = tx.clone();

                futs.push(Box::pin(async move {
                    let rt_futs: FuturesUnordered<_> = record_types
                        .iter()
                        .map(|rt| {
                            let rt = *rt;
                            let domain_name = domain_name.clone();
                            let auth_servers = auth_servers.clone();
                            let raw_outbound = raw_outbound.clone();
                            let tx_err = tx_err.clone();
                            async move {
                                // Convert mhost RecordType to hickory RecordType.
                                let hickory_rt: HickoryRecordType =
                                    hickory_proto::rr::RecordType::from(u16::from(rt));

                                let results = crate::api::authcompare::query_auth_servers(
                                    &raw_outbound,
                                    &auth_servers,
                                    &domain_name,
                                    hickory_rt,
                                    trace_timeout,
                                )
                                .await;

                                // Convert raw responses to mhost Lookups format.
                                let mut lookups_vec = Vec::new();
                                for qr in &results {
                                    match &qr.result {
                                        Ok(raw_resp) => {
                                            let latency = raw_resp.latency;
                                            let response_time = WireDuration {
                                                secs: latency.as_secs(),
                                                nanos: latency.subsec_nanos(),
                                            };
                                            let records: Vec<_> = raw_resp
                                                .answers()
                                                .iter()
                                                .map(|r| {
                                                    (
                                                        dns_raw::record_to_dns_record(r),
                                                        dns_raw::rdata_presentation(&r.data),
                                                    )
                                                })
                                                .collect();

                                            // Build a synthetic lookup result.
                                            let ns_name = format!(
                                                "udp:{}:{}",
                                                qr.server.ip(),
                                                qr.server.port()
                                            );
                                            let result = if raw_resp.response_code()
                                                == hickory_proto::op::ResponseCode::NXDomain
                                            {
                                                AuthResult::NxDomain { response_time }
                                            } else {
                                                let rcode = raw_resp.response_code();
                                                let answered =
                                                    rcode == hickory_proto::op::ResponseCode::NoError;
                                                // A referral (NOERROR, no answer, not
                                                // authoritative, NS in authority) is no answer
                                                // (Rule M).
                                                let referral = answered
                                                    && raw_resp.answers().is_empty()
                                                    && !raw_resp.is_authoritative()
                                                    && !raw_resp.referral_ns_names().is_empty();
                                                AuthResult::Response {
                                                    failure: if !answered {
                                                        Some(rcode.to_string())
                                                    } else if referral {
                                                        Some("referral, no answer".to_owned())
                                                    } else {
                                                        None
                                                    },
                                                    failure_code: (!answered)
                                                        .then(|| rcode_mnemonic(rcode)),
                                                    records: records
                                                        .into_iter()
                                                        .map(|(r, presentation)| AuthRecord {
                                                            data: BTreeMap::from([(
                                                                r.record_type.clone(),
                                                                r.rdata,
                                                            )]),
                                                            name: r.name,
                                                            record_type: r.record_type,
                                                            ttl: r.ttl,
                                                            presentation,
                                                        })
                                                        .collect(),
                                                    response_time,
                                                }
                                            };

                                            lookups_vec.push(AuthLookup {
                                                query: AuthQuery {
                                                    name: domain_name.to_ascii(),
                                                    record_type: rt.to_string(),
                                                },
                                                name_server: ns_name,
                                                result,
                                            });
                                        }
                                        Err(e) => {
                                            tracing::warn!(domain = %domain_name, record_type = %rt, server = %qr.server, error = %e, "authoritative query failed");
                                            send_error(
                                                &tx_err,
                                                "AUTH_QUERY_ERROR",
                                                &format!("{}: {e}", qr.server),
                                            )
                                            .await;
                                        }
                                    }
                                }

                                (
                                    rt,
                                    BatchLookups::Authoritative(AuthLookups {
                                        lookups: lookups_vec,
                                    }),
                                    "authoritative",
                                )
                            }
                        })
                        .collect();

                    rt_futs.collect().await
                }));
            }

            tokio::pin!(futs);

            loop {
                tokio::select! {
                    maybe = futs.next() => {
                        match maybe {
                            None => break,
                            Some(results) => {
                                for (rt, lookups, source) in results {
                                    completed += 1;
                                    let batch = DnsBatch {
                                        request_id: rid.clone(),
                                        record_type: rt.to_string(),
                                        lookups,
                                        completed,
                                        total: total_batches,
                                        transport: None,
                                        source: Some(source.to_owned()),
                                    };
                                    if enrichment_svc.is_some() {
                                        batch_events.push(CachedEvent {
                                            event_type: "batch".to_owned(),
                                            data: batch.cache_value(),
                                        });
                                    }
                                    if tx.send(DnsAuthCompareEvent::Batch(batch)).await.is_err() {
                                        prism.endpoint("authcompare", "error");
                                        prism.active("prism_active_authcompares", -1.0);
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        send_error(&tx, "STREAM_TIMEOUT", "stream deadline exceeded").await;
                        prism.endpoint("authcompare", "error");
                        break;
                    }
                }
            }

            if !timed_out {
                // Enrichment.
                if let Some(ref svc) = enrichment_svc {
                    let ips = extract_ips_from_cached_events(&batch_events);
                    if let Some(e) = enrichment_event(svc, &ips, &rid).await {
                        let _ = tx.send(DnsAuthCompareEvent::Enrichment(e)).await;
                    }
                }

                let elapsed = start.elapsed();
                tracing::info!(
                    request_id = %rid,
                    domain = %domain,
                    duration_ms = elapsed.as_millis(),
                    auth_servers = ?auth_server_labels,
                    "authcompare completed"
                );

                let done = DnsAuthCompareDone {
                    request_id: rid,
                    total_queries: completed,
                    duration_ms: elapsed.as_millis() as u64,
                    warnings: all_warnings,
                    auth_servers: auth_server_labels,
                    cache_key: None,
                };
                let _ = tx.send(DnsAuthCompareEvent::Done(done)).await;

                prism.endpoint("authcompare", "ok");
                prism.duration("prism_authcompare_duration_seconds", elapsed);
            }

            prism.active("prism_active_authcompares", -1.0);
        });

        Ok(DnsJob::new(rx))
    }

    /// A trace job: the delegation walk from the root servers.
    pub async fn trace(
        &self,
        req: &TraceRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsTraceEvent>, ApiError> {
        let domain = req.domain.to_ascii_lowercase();
        if domain.is_empty() {
            return Err(ApiError::InvalidDomain("empty domain".into()));
        }
        if domain.len() > 253 {
            return Err(ApiError::InvalidDomain(format!(
                "domain exceeds maximum length of 253 characters (got {})",
                domain.len()
            )));
        }

        let name =
            dns_trace::parse_name(&domain).map_err(|e| ApiError::InvalidDomain(e.to_string()))?;

        let record_type = dns_trace::parse_record_type(&req.record_type)
            .map_err(|_| ApiError::InvalidRecordType(req.record_type.clone()))?;

        // Block types that don't make sense in an iterative trace.
        match record_type {
            HickoryRecordType::ANY | HickoryRecordType::AXFR | HickoryRecordType::IXFR => {
                return Err(ApiError::InvalidRecordType(req.record_type.clone()));
            }
            _ => {}
        }

        // Clamp timeout to config max; default to config max.
        let timeout_secs = req
            .timeout_secs
            .map(|t| t.min(self.config.limits.max_timeout_secs))
            .unwrap_or(self.config.trace.query_timeout_secs);
        let query_timeout = Duration::from_secs(timeout_secs);

        let max_hops = self.config.trace.max_hops as usize;

        // Rate limiting: flat cost against per-IP and global limiters only.
        // Trace queries public infrastructure (not user-specified servers), so
        // per-target charging is skipped.
        let stream_guard = limits.check_query_cost(client, &[], TRACE_COST, TRACE_COST)?;

        let (tx, rx) = mpsc::channel::<DnsTraceEvent>(32);
        let rid = request_id;
        let enrichment_svc = self.ip_enrichment.clone();
        let raw = self.backend.raw.clone();
        let prism = PrismMetrics(self.prism_metrics);

        tokio::spawn(async move {
            let _stream_guard = stream_guard;
            prism.active("prism_active_traces", 1.0);
            let start = Instant::now();

            let hops = match tokio::time::timeout(
                Duration::from_secs(STREAM_TIMEOUT_SECS),
                dns_trace::walk_with(&raw, name, record_type, max_hops, query_timeout),
            )
            .await
            {
                Ok(hops) => hops,
                Err(_) => {
                    send_error(&tx, "STREAM_TIMEOUT", "stream deadline exceeded").await;
                    prism.endpoint("trace", "error");
                    prism.active("prism_active_traces", -1.0);
                    return;
                }
            };
            let hop_count = hops.len();

            // Server IPs of all hops, for enrichment.
            let mut ips: Vec<IpAddr> = Vec::new();
            for hop in hops {
                if enrichment_svc.is_some() {
                    for sr in &hop.server_results {
                        if !ips.contains(&sr.server_ip) {
                            ips.push(sr.server_ip);
                        }
                    }
                }
                let event = DnsHopEvent {
                    request_id: rid.clone(),
                    hop,
                };
                if tx.send(DnsTraceEvent::Hop(event)).await.is_err() {
                    prism.endpoint("trace", "error");
                    prism.active("prism_active_traces", -1.0);
                    return;
                }
            }

            // Enrich server IPs from all hops.
            if let Some(ref svc) = enrichment_svc
                && let Some(e) = enrichment_event(svc, &ips, &rid).await
            {
                let _ = tx.send(DnsTraceEvent::Enrichment(e)).await;
            }

            let elapsed = start.elapsed();
            tracing::info!(
                request_id = %rid,
                domain = %domain,
                duration_ms = elapsed.as_millis(),
                "trace completed"
            );
            let done = DnsTraceDone {
                request_id: rid,
                duration_ms: elapsed.as_millis() as u64,
                hops: hop_count,
                cache_key: None,
            };
            let _ = tx.send(DnsTraceEvent::Done(done)).await;

            prism.endpoint("trace", "ok");
            prism.duration("prism_trace_duration_seconds", elapsed);
            prism.active("prism_active_traces", -1.0);
        });

        Ok(DnsJob::new(rx))
    }

    /// A dnssec job: the chain-of-trust walk from the root servers.
    pub async fn dnssec(
        &self,
        req: &DnssecRequest,
        client: IpAddr,
        limits: &RateLimitState,
        request_id: String,
    ) -> Result<DnsJob<DnsDnssecEvent>, ApiError> {
        let domain = req.domain.to_ascii_lowercase();
        if domain.is_empty() {
            return Err(ApiError::InvalidDomain("empty domain".into()));
        }
        if domain.len() > 253 {
            return Err(ApiError::InvalidDomain(format!(
                "domain exceeds maximum length of 253 characters (got {})",
                domain.len()
            )));
        }

        let name = dns_dnssec::parse_name(&domain).map_err(ApiError::InvalidDomain)?;

        // Clamp timeout to config max; default to trace query timeout.
        let timeout_secs = req
            .timeout_secs
            .map(|t| t.min(self.config.limits.max_timeout_secs))
            .unwrap_or(self.config.trace.query_timeout_secs);
        let query_timeout = Duration::from_secs(timeout_secs);

        let max_hops = self.config.trace.max_hops as usize;

        // Rate limiting: flat cost against per-IP and global limiters only.
        let stream_guard = limits.check_query_cost(client, &[], DNSSEC_COST, DNSSEC_COST)?;

        let (tx, rx) = mpsc::channel::<DnsDnssecEvent>(32);
        let rid = request_id;
        let raw = self.backend.raw.clone();
        let prism = PrismMetrics(self.prism_metrics);

        tokio::spawn(async move {
            let _stream_guard = stream_guard;
            prism.active("prism_active_dnssec", 1.0);
            let start = Instant::now();

            let levels = match tokio::time::timeout(
                Duration::from_secs(STREAM_TIMEOUT_SECS),
                dns_dnssec::walk_chain_with(&raw, name, max_hops, query_timeout),
            )
            .await
            {
                Ok(levels) => levels,
                Err(_) => {
                    send_error(&tx, "STREAM_TIMEOUT", "stream deadline exceeded").await;
                    prism.endpoint("dnssec", "error");
                    prism.active("prism_active_dnssec", -1.0);
                    return;
                }
            };
            let level_count = levels.len();

            for level in levels {
                let event = DnsChainEvent {
                    request_id: rid.clone(),
                    level,
                };
                if tx.send(DnsDnssecEvent::Chain(event)).await.is_err() {
                    prism.endpoint("dnssec", "error");
                    prism.active("prism_active_dnssec", -1.0);
                    return;
                }
            }

            let elapsed = start.elapsed();
            tracing::info!(
                request_id = %rid,
                domain = %domain,
                duration_ms = elapsed.as_millis(),
                "dnssec chain walk completed"
            );
            let done = DnsDnssecDone {
                request_id: rid,
                duration_ms: elapsed.as_millis() as u64,
                levels: level_count,
                cache_key: None,
            };
            let _ = tx.send(DnsDnssecEvent::Done(done)).await;

            prism.endpoint("dnssec", "ok");
            prism.duration("prism_dnssec_duration_seconds", elapsed);
            prism.active("prism_active_dnssec", -1.0);
        });

        Ok(DnsJob::new(rx))
    }
}
