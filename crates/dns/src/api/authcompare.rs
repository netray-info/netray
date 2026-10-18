//! Authoritative-vs-Recursive comparison endpoint.
//!
//! - `POST /api/authcompare` — discover authoritative NS for a domain, then
//!   query both the authoritative servers (RD=0) and configured recursive
//!   resolvers in parallel, streaming comparison results.
//!
//! Phase 1: Discover authoritative nameservers via recursive NS lookup.
//! Phase 2: Fan out per-record-type lookups to both auth (RD=0) and recursive
//!           resolvers concurrently. Each batch event includes `source` field
//!           ("authoritative" or "recursive").

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use hickory_proto::rr::{Name, RecordType};

use crate::RequestId;
use crate::api::query::{PostQueryRequest, StreamParams};
use crate::api::{AppState, respond};
use crate::dns_raw::{self, NOT_PUBLIC, RawOutbound};
use crate::error::{ApiError, ErrorResponse};

// ---------------------------------------------------------------------------
// Authoritative servers
// ---------------------------------------------------------------------------

/// The authoritative servers of a domain that the outbound policy lets us query.
pub(crate) struct AuthServers {
    /// IPv4, port 53, allowed only.
    pub(crate) servers: Vec<SocketAddr>,
    /// `"ns1.example.com. (192.0.2.53)"` per listed server.
    pub(crate) labels: Vec<String>,
    /// `NOT_PUBLIC` once when any address was refused.
    pub(crate) warnings: Vec<String>,
}

/// Resolve the NS names to addresses and keep the ones the outbound policy allows.
pub(crate) async fn resolve_auth_servers(raw: &RawOutbound, ns_names: &[String]) -> AuthServers {
    let mut ns_ips: HashMap<String, Vec<IpAddr>> = HashMap::new();
    for ns in ns_names {
        ns_ips.insert(ns.clone(), Vec::new());
    }
    raw.resolve_missing_glue(&mut ns_ips).await;

    let list = raw.build_server_list(&ns_ips);
    let mut servers = Vec::with_capacity(list.servers.len());
    let mut labels = Vec::with_capacity(list.servers.len());
    for (addr, ns_name) in &list.servers {
        servers.push(*addr);
        if let Some(ns_name) = ns_name {
            labels.push(format!("{ns_name} ({})", addr.ip()));
        }
    }
    let warnings = if list.refused > 0 {
        vec![NOT_PUBLIC.to_owned()]
    } else {
        Vec::new()
    };
    AuthServers {
        servers,
        labels,
        warnings,
    }
}

/// The authoritative branch's per-record-type fan-out.
pub(crate) async fn query_auth_servers(
    raw: &RawOutbound,
    servers: &[SocketAddr],
    name: &Name,
    record_type: RecordType,
    timeout: Duration,
) -> Vec<dns_raw::RawQueryResult> {
    raw.parallel_queries(servers, name, record_type, timeout)
        .await
}

/// Compare DNS results from authoritative nameservers vs recursive resolvers.
///
/// Phase 1 discovers the domain's authoritative NS via recursive resolution.
/// Phase 2 queries both auth (RD=0) and recursive resolvers per record type.
///
/// ## SSE Events
///
/// - `batch` — per record type per source: includes `source` field ("authoritative" or "recursive")
/// - `done` — summary with `auth_servers` listing discovered nameservers
/// - `error` — non-fatal error
#[utoipa::path(
    post, path = "/api/authcompare",
    tag = "Query",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "SSE stream of auth-vs-recursive comparison results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn post_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(stream_params): Query<StreamParams>,
    Json(body): Json<PostQueryRequest>,
) -> Result<Response, ApiError> {
    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));
    tracing::debug!(%client_ip, %peer_addr, "authcompare POST");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .authcompare(&body, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(
        job,
        Some((state.result_cache.clone(), "auth", body.domain)),
        stream_params.stream,
    )
    .await)
}

#[cfg(test)]
#[path = "authcompare_outbound_tests.rs"]
mod outbound_tests;
