//! The DNS inspector jobs of v1: `POST /dns/{query,compare,authcompare,trace,dnssec}`, over the
//! `netray_dns::DnsJobs` part. Each answers 503 first when the part is off, then reads the body,
//! then starts the job, which charges the `[api.dns]` limiter held by `ApiState`. Nothing is
//! stored or cached.
use std::convert::Infallible;

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response, Sse};
use futures::StreamExt;
use netray_common::error::ErrorResponse;
use netray_dns::parser::ParsedQuery;
use netray_dns::{
    DnsAuthCompareEvent, DnsAuthCompareResult, DnsCompareEvent, DnsCompareResult, DnsDnssecEvent,
    DnsDnssecResult, DnsJob, DnsJobEvent, DnsJobs, DnsQueryEvent, DnsQueryResult, DnsRefusal,
    DnsTraceEvent, DnsTraceResult, DnssecRequest, PostQueryRequest, TraceRequest,
};
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

/// Streams the job's events under their names, or collects them into one object.
async fn respond<E: DnsJobEvent>(job: DnsJob<E>, headers: &HeaderMap) -> Response {
    if wants_event_stream(headers) {
        let frames = job
            .scan(E::Collected::default(), |collected, event| {
                event.clone().collect_into(collected);
                let mut frames = Vec::with_capacity(2);
                if event.is_done() {
                    frames.extend(E::summary_event(collected).map(|e| e.sse_event()));
                }
                frames.push(event.sse_event());
                std::future::ready(Some(futures::stream::iter(
                    frames.into_iter().map(Ok::<_, Infallible>),
                )))
            })
            .flatten();
        return Sse::new(frames).into_response();
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
    Ok(respond(job, &headers).await)
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
    let parsed = parsed_q(&q_of(params)?)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .compare_parsed(parsed, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers).await)
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
    let parsed = parsed_q(&q_of(params)?)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.dns_limiter().prune_if_due();
    let job = jobs
        .authcompare_parsed(parsed, client, state.dns_limiter(), request_id(&headers))
        .await
        .map_err(refused)?;
    Ok(respond(job, &headers).await)
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
    let parsed = parsed_q(&q_of(params)?)?;
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
    Ok(respond(job, &headers).await)
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
    let parsed = parsed_q(&q_of(params)?)?;
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
    Ok(respond(job, &headers).await)
}

pub(super) fn dns_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(dns_query, dns_query_get))
        .routes(routes!(dns_compare, dns_compare_get))
        .routes(routes!(dns_authcompare, dns_authcompare_get))
        .routes(routes!(dns_trace, dns_trace_get))
        .routes(routes!(dns_dnssec, dns_dnssec_get))
}
