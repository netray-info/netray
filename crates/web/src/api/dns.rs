//! The DNS inspector jobs of v1: `POST /dns/{query,compare,authcompare,trace,dnssec}`, over the
//! `netray_dns::DnsJobs` part. Each answers 503 first when the part is off, then reads the body,
//! then starts the job, which charges the `[api.dns]` limiter held by `ApiState`. Nothing is
//! stored or cached.
use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response, Sse};
use futures::StreamExt;
use netray_common::error::ErrorResponse;
use netray_dns::{
    DnsAuthCompareEvent, DnsAuthCompareResult, DnsCompareEvent, DnsCompareResult, DnsDnssecEvent,
    DnsDnssecResult, DnsJob, DnsJobEvent, DnsJobs, DnsQueryEvent, DnsQueryResult, DnsRefusal,
    DnsTraceEvent, DnsTraceResult, DnssecRequest, PostQueryRequest, TraceRequest,
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::inspectors::request_id;
use super::ip::{Peer, json_body, peer_of};
use super::limit;
use super::runs::wants_event_stream;
use super::state::ApiState;

const DNS_OFF: &str = "the DNS inspector is off: no DNS module was built";

fn jobs_of(state: &ApiState) -> Result<&DnsJobs, V1Error> {
    state
        .dns_jobs()
        .ok_or_else(|| V1Error::Unavailable(DNS_OFF.into()))
}

fn refused(error: netray_dns::error::ApiError) -> V1Error {
    DnsRefusal::from(error).into()
}

/// Streams the job's events under their names, or collects them into one object.
async fn respond<E: DnsJobEvent>(job: DnsJob<E>, headers: &HeaderMap) -> Response {
    if wants_event_stream(headers) {
        return Sse::new(job.map(|event| Ok::<_, Infallible>(event.sse_event()))).into_response();
    }
    Json(job.collect().await).into_response()
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
    Ok(respond(job, &headers).await)
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
    Ok(respond(job, &headers).await)
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
    Ok(respond(job, &headers).await)
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
    Ok(respond(job, &headers).await)
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
    Ok(respond(job, &headers).await)
}

pub(super) fn dns_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(dns_query))
        .routes(routes!(dns_compare))
        .routes(routes!(dns_authcompare))
        .routes(routes!(dns_trace))
        .routes(routes!(dns_dnssec))
}
