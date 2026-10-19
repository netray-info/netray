//! The DNS inspector jobs of v1: `POST /dns/{query,compare,authcompare,trace,dnssec}`, over the
//! `netray_dns::DnsJobs` part. Each answers 503 first when the part is off, then reads the body,
//! then starts the job, which charges the `[api.dns]` limiter held by `ApiState`. A job that
//! reaches `done` stores its collected result under the `done`'s `cache_key`, which
//! `GET /dns/results/{key}` replays.
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response, Sse};
use futures::{Stream, StreamExt};
use netray_common::error::ErrorResponse;
use netray_dns::parser::ParsedQuery;
use netray_dns::{
    DnsAuthCompareEvent, DnsAuthCompareResult, DnsCachedJob, DnsCachedResult, DnsCheckResult,
    DnsCompareEvent, DnsCompareResult, DnsDnssecEvent, DnsDnssecResult, DnsJob, DnsJobEvent,
    DnsJobs, DnsQueryEvent, DnsQueryResult, DnsRefusal, DnsResultCache, DnsTraceEvent,
    DnsTraceResult, DnssecRequest, PostQueryRequest, TraceRequest,
};
use netray_dns::{DnsParseAnswer, DnsParseRequest, DnsRecordTypes, DnsServerCatalogue};
use netray_engine::{Domain, Facts, Module, RunContext};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::inspectors::request_id;
use super::ip::{Peer, json_body, peer_of};
use super::limit;
use super::runs::wants_event_stream;
use super::state::ApiState;

const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The routing flags of `GET /dns/query`, in the order they are checked, and the job path each
/// names.
const ROUTING_FLAGS: [(&str, &str); 4] = [
    ("+trace", "trace"),
    ("+compare", "compare"),
    ("+auth", "authcompare"),
    ("+check", "check"),
];

#[derive(Deserialize)]
pub(super) struct QParams {
    q: Option<String>,
}

pub(super) const DNS_OFF: &str = "the DNS inspector is off: no DNS module was built";

fn jobs_of(state: &ApiState) -> Result<&DnsJobs, V1Error> {
    state
        .dns_jobs()
        .ok_or_else(|| V1Error::Unavailable(DNS_OFF.into()))
}

fn refused(error: netray_dns::error::ApiError) -> V1Error {
    DnsRefusal::from(error).into()
}

/// The job's events with `summary` before `done`, and a fresh key on `done`, under which the
/// collected result is stored before `done` is passed on.
fn keyed_events<E: DnsJobEvent>(
    job: DnsJob<E>,
    cache: DnsResultCache,
    query: String,
) -> impl Stream<Item = E> {
    job.scan(
        (E::Collected::default(), query),
        move |(collected, query), mut event| {
            let key = event.is_done().then(DnsResultCache::fresh_key);
            if let Some(key) = &key {
                event.set_cache_key(key.clone());
            }
            event.clone().collect_into(collected);
            let mut events = Vec::with_capacity(2);
            let stored = key.map(|key| {
                events.extend(E::summary_event(collected));
                DnsCachedResult {
                    key,
                    mode: E::MODE.to_owned(),
                    query: query.clone(),
                    result: collected.clone().into(),
                }
            });
            events.push(event);
            let cache = cache.clone();
            async move {
                if let Some(stored) = stored {
                    cache.insert(stored).await;
                }
                Some(futures::stream::iter(events))
            }
        },
    )
    .flatten()
}

/// Streams the job's events under their names, or collects them into one object.
async fn respond<E: DnsJobEvent>(
    job: DnsJob<E>,
    headers: &HeaderMap,
    state: &ApiState,
    query: String,
) -> Response {
    let events = keyed_events(job, state.dns_results().clone(), query);
    if wants_event_stream(headers) {
        let frames = events.map(|event| Ok::<_, Infallible>(event.sse_event()));
        return Sse::new(frames).into_response();
    }
    let collected = events
        .fold(E::Collected::default(), |mut collected, event| {
            event.collect_into(&mut collected);
            std::future::ready(collected)
        })
        .await;
    Json(collected).into_response()
}

/// Resolves a domain over the configured resolvers: one batch per provider and record type.
/// With `Accept: text/event-stream` the same events arrive as they complete.
#[utoipa::path(
    post,
    path = "/dns/query",
    tag = "dns",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "The lookups of the domain", content(
            (DnsQueryResult = "application/json"),
            (DnsQueryEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_query(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<PostQueryRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .query(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, request.domain).await)
}

/// The same query over UDP, TCP, DNS-over-TLS and DNS-over-HTTPS, to compare the transports.
#[utoipa::path(
    post,
    path = "/dns/compare",
    tag = "dns",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "The lookups of the domain per transport", content(
            (DnsCompareResult = "application/json"),
            (DnsCompareEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_compare(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<PostQueryRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .compare(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, request.domain).await)
}

/// The recursive answer against the authoritative one, queried from the domain's name servers.
#[utoipa::path(
    post,
    path = "/dns/authcompare",
    tag = "dns",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "The recursive and the authoritative lookups of the domain", content(
            (DnsAuthCompareResult = "application/json"),
            (DnsAuthCompareEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_authcompare(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<PostQueryRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .authcompare(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, request.domain).await)
}

/// The delegation path of a name from the root servers, one hop per server asked.
#[utoipa::path(
    post,
    path = "/dns/trace",
    tag = "dns",
    request_body = TraceRequest,
    responses(
        (status = 200, description = "The hops of the delegation", content(
            (DnsTraceResult = "application/json"),
            (DnsTraceEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body, invalid domain or record type", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_trace(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<TraceRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .trace(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, request.domain).await)
}

/// The DNSSEC chain of trust of a name, one level per zone from the root.
#[utoipa::path(
    post,
    path = "/dns/dnssec",
    tag = "dns",
    request_body = DnssecRequest,
    responses(
        (status = 200, description = "The levels of the chain of trust", content(
            (DnsDnssecResult = "application/json"),
            (DnsDnssecEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body or invalid domain", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_dnssec(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<DnssecRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .dnssec(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, request.domain).await)
}

fn q_of(params: Result<Query<QParams>, QueryRejection>) -> Result<String, V1Error> {
    let Query(QParams { q }) = params.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    q.filter(|q| !q.trim().is_empty())
        .ok_or_else(|| V1Error::InvalidInput("missing query parameter `q`".into()))
}

fn parsed_q(q: &str) -> Result<ParsedQuery, V1Error> {
    let parsed = netray_dns::parser::parse(q).map_err(|e| V1Error::InvalidInput(e.to_string()))?;
    if parsed.domain.starts_with(['@', '+']) {
        return Err(V1Error::InvalidInput(format!(
            "no domain: `{}` is a server or a flag",
            parsed.domain
        )));
    }
    Ok(parsed)
}

fn routing_redirect(q: &str) -> Option<Response> {
    let (_, path) = ROUTING_FLAGS.iter().find(|(flag, _)| {
        q.split_whitespace()
            .any(|token| token.eq_ignore_ascii_case(flag))
    })?;
    let location = format!(
        "{}/dns/{path}?q={}",
        super::MOUNT,
        utf8_percent_encode(q, UNRESERVED)
    );
    Some(
        (
            StatusCode::TEMPORARY_REDIRECT,
            [(header::LOCATION, location)],
        )
            .into_response(),
    )
}

/// The job of `POST /dns/query` over the dig-like query language in `q`. A routing flag (`+trace`,
/// `+compare`, `+auth`, `+check`) redirects to that job's path with the same `q`.
#[utoipa::path(
    get,
    path = "/dns/query",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain, then record types, `@server` names or addresses and `+flags`")),
    responses(
        (status = 200, description = "The lookups of the domain", content(
            (DnsQueryResult = "application/json"),
            (DnsQueryEvent = "text/event-stream"),
        )),
        (status = 307, description = "A routing flag (+trace, +compare, +auth, +check): the same q on that job's path"),
        (status = 400, description = "Missing or unparsable q, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_query_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let q = q_of(params)?;
    if let Some(redirect) = routing_redirect(&q) {
        return Ok(redirect);
    }
    let parsed = parsed_q(&q)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .query_parsed(parsed, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, q).await)
}

/// The job of `POST /dns/compare` over the query language in `q`.
#[utoipa::path(
    get,
    path = "/dns/compare",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain, then record types, `@server` names or addresses and `+flags`")),
    responses(
        (status = 200, description = "The lookups of the domain per transport", content(
            (DnsCompareResult = "application/json"),
            (DnsCompareEvent = "text/event-stream"),
        )),
        (status = 400, description = "Missing or unparsable q, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_compare_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let q = q_of(params)?;
    let parsed = parsed_q(&q)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .compare_parsed(parsed, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, q).await)
}

/// The job of `POST /dns/authcompare` over the query language in `q`.
#[utoipa::path(
    get,
    path = "/dns/authcompare",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain, then record types, `@server` names or addresses and `+flags`")),
    responses(
        (status = 200, description = "The recursive and the authoritative lookups of the domain", content(
            (DnsAuthCompareResult = "application/json"),
            (DnsAuthCompareEvent = "text/event-stream"),
        )),
        (status = 400, description = "Missing or unparsable q, invalid domain, record type or server, or a query the DNS policy refuses", body = ErrorResponse),
        (status = 403, description = "A server address in a blocked range", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_authcompare_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let q = q_of(params)?;
    let parsed = parsed_q(&q)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .authcompare_parsed(parsed, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, q).await)
}

/// The job of `POST /dns/trace` over the query language in `q`: the domain and its first record type.
#[utoipa::path(
    get,
    path = "/dns/trace",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain, then a record type (A when none)")),
    responses(
        (status = 200, description = "The hops of the delegation", content(
            (DnsTraceResult = "application/json"),
            (DnsTraceEvent = "text/event-stream"),
        )),
        (status = 400, description = "Missing or unparsable q, invalid domain or record type", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_trace_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let q = q_of(params)?;
    let parsed = parsed_q(&q)?;
    let request = TraceRequest {
        record_type: parsed
            .record_types
            .first()
            .map_or_else(|| "A".to_string(), ToString::to_string),
        domain: parsed.domain,
        timeout_secs: None,
    };
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .trace(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, q).await)
}

/// The job of `POST /dns/dnssec` over the query language in `q`: the domain.
#[utoipa::path(
    get,
    path = "/dns/dnssec",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain")),
    responses(
        (status = 200, description = "The levels of the chain of trust", content(
            (DnsDnssecResult = "application/json"),
            (DnsDnssecEvent = "text/event-stream"),
        )),
        (status = 400, description = "Missing or unparsable q, or invalid domain", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off, or no resolver could be built", body = ErrorResponse),
    )
)]
pub(super) async fn dns_dnssec_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let jobs = jobs_of(&state)?;
    let q = q_of(params)?;
    let parsed = parsed_q(&q)?;
    let request = DnssecRequest {
        domain: parsed.domain,
        timeout_secs: None,
    };
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .dnssec(&request, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers, &state, q).await)
}

/// The flat cost of one health check against the `[api.dns]` budget.
const CHECK_COST: u32 = 16;
const CHECK_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Deserialize, utoipa::ToSchema)]
pub(super) struct DnsCheckRequest {
    /// The domain to check
    domain: String,
}

fn module_of(state: &ApiState) -> Result<&Arc<dyn Module>, V1Error> {
    state
        .dns_module()
        .ok_or_else(|| V1Error::Unavailable(DNS_OFF.into()))
}

/// The name a check runs on: one domain, as lens validates it, with no address (a trailing dot
/// included), no whitespace and nothing but letters, digits, `-`, `_` and `.`.
fn check_domain(raw: &str) -> Result<String, V1Error> {
    let name = raw.trim();
    if name.trim_end_matches('.').parse::<IpAddr>().is_ok() {
        return Err(V1Error::InvalidInput(format!(
            "`{name}` is an IP address: the check takes a domain"
        )));
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        return Err(V1Error::InvalidInput(format!(
            "`{name}` is not a domain: it holds {c:?}"
        )));
    }
    netray_common::domain::validate(name).map_err(V1Error::InvalidInput)
}

async fn run_check(
    state: &ApiState,
    module: &Arc<dyn Module>,
    headers: &HeaderMap,
    peer: Peer,
    raw: &str,
    query: String,
) -> Result<Response, V1Error> {
    let domain = check_domain(raw)?;
    let client = limit::client(state, headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let _guard = state
        .dns_limiter()
        .check_query_cost(client, &[], CHECK_COST, CHECK_COST)
        .map_err(refused)?;
    let ctx = RunContext {
        deadline: Instant::now() + CHECK_DEADLINE,
        domain: Domain::new(domain),
        options: Default::default(),
    };
    let mut result = netray_dns::check_result(module.run(&ctx, &Facts::default()).await);
    let key = DnsResultCache::fresh_key();
    result.cache_key = Some(key.clone());
    state
        .dns_results()
        .insert(DnsCachedResult {
            key,
            mode: "check".to_owned(),
            query,
            result: DnsCachedJob::Check(result.clone()),
        })
        .await;
    Ok(Json(result).into_response())
}

/// Runs the DNS module's ten health checks on a domain and answers them as one JSON object,
/// whatever the `Accept` header.
#[utoipa::path(
    post,
    path = "/dns/check",
    tag = "dns",
    request_body = DnsCheckRequest,
    responses(
        (status = 200, description = "The DNS section of a run: its checks, evidence blocks and the key it replays under", body = DnsCheckResult),
        (status = 400, description = "Malformed body, or a blank domain or an IP address", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off", body = ErrorResponse),
    )
)]
pub(super) async fn dns_check(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<DnsCheckRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let module = module_of(&state)?;
    let request = json_body(body)?;
    let query = request.domain.clone();
    run_check(&state, module, &headers, peer, &request.domain, query).await
}

/// The check of `POST /dns/check` over the query language in `q`: the domain is its first token.
#[utoipa::path(
    get,
    path = "/dns/check",
    tag = "dns",
    params(("q" = String, Query, description = "The query: a domain")),
    responses(
        (status = 200, description = "The DNS section of a run: its checks, evidence blocks and the key it replays under", body = DnsCheckResult),
        (status = 400, description = "Missing or unparsable q, or a blank domain or an IP address", body = ErrorResponse),
        (status = 429, description = "Rate limited, or too many DNS jobs of this caller at once", body = ErrorResponse),
        (status = 503, description = "The DNS module is off", body = ErrorResponse),
    )
)]
pub(super) async fn dns_check_get(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    params: Result<Query<QParams>, QueryRejection>,
) -> Result<Response, V1Error> {
    let module = module_of(&state)?;
    let q = q_of(params)?;
    let first = q.split_whitespace().next().unwrap_or_default().to_owned();
    run_check(&state, module, &headers, peer, &first, q).await
}

pub(super) fn dns_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(dns_query, dns_query_get))
        .routes(routes!(dns_compare, dns_compare_get))
        .routes(routes!(dns_authcompare, dns_authcompare_get))
        .routes(routes!(dns_trace, dns_trace_get))
        .routes(routes!(dns_dnssec, dns_dnssec_get))
        .routes(routes!(dns_check, dns_check_get))
}

/// Parses a query string: its tokens with byte ranges, the completions at the cursor, and the
/// parsed query or the reason it does not parse.
#[utoipa::path(
    post,
    path = "/dns/parse",
    tag = "dns",
    request_body = DnsParseRequest,
    responses(
        (status = 200, description = "Tokens, completions and the parsed query or its error", body = DnsParseAnswer),
        (status = 400, description = "Malformed body", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The DNS module is off", body = ErrorResponse),
    )
)]
pub(super) async fn dns_parse(
    State(state): State<ApiState>,
    body: Result<Json<DnsParseRequest>, JsonRejection>,
) -> Result<Json<DnsParseAnswer>, V1Error> {
    let jobs = jobs_of(&state)?;
    let request = json_body(body)?;
    Ok(Json(netray_dns::parse_answer(
        &request,
        jobs.allows_system_resolvers(),
    )))
}

/// The predefined DNS providers with their transports, and the group aliases over them.
#[utoipa::path(
    get,
    path = "/dns/servers",
    tag = "dns",
    responses(
        (status = 200, description = "Providers and groups", body = DnsServerCatalogue),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The DNS module is off", body = ErrorResponse),
    )
)]
pub(super) async fn dns_servers(
    State(state): State<ApiState>,
) -> Result<Json<DnsServerCatalogue>, V1Error> {
    jobs_of(&state)?;
    Ok(Json(netray_dns::server_catalogue()))
}

/// The record types a query may ask for; the blocked and internal ones are left out.
#[utoipa::path(
    get,
    path = "/dns/record-types",
    tag = "dns",
    responses(
        (status = 200, description = "The record types", body = DnsRecordTypes),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The DNS module is off", body = ErrorResponse),
    )
)]
pub(super) async fn dns_record_types(
    State(state): State<ApiState>,
) -> Result<Json<DnsRecordTypes>, V1Error> {
    jobs_of(&state)?;
    Ok(Json(netray_dns::record_types()))
}

/// The collected result of a DNS job, under the `cache_key` of its `done`, for up to an hour.
#[utoipa::path(
    get,
    path = "/dns/results/{key}",
    tag = "dns",
    params(("key" = String, Path, description = "The 12-character lowercase hex `cache_key` of a job's `done`")),
    responses(
        (status = 200, description = "The stored result: the job's mode, its query and its collected result", body = DnsCachedResult),
        (status = 400, description = "The key is not 12 lowercase hex characters", body = ErrorResponse),
        (status = 404, description = "No result under this key: expired or never stored", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
pub(super) async fn dns_result(
    State(state): State<ApiState>,
    key: Result<Path<String>, PathRejection>,
) -> Result<Json<DnsCachedResult>, V1Error> {
    let Path(key) = key.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    if !netray_dns::is_key(&key) {
        return Err(V1Error::InvalidInput(format!(
            "`{key}` is not a result key: 12 lowercase hex characters"
        )));
    }
    state
        .dns_results()
        .get(&key)
        .await
        .map(Json)
        .ok_or_else(|| V1Error::NotFound(format!("no result under `{key}`")))
}

pub(super) fn dns_read_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(dns_result))
        .routes(routes!(dns_parse))
        .routes(routes!(dns_servers))
        .routes(routes!(dns_record_types))
}
