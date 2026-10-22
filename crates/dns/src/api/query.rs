//! Query endpoint: parse, validate, execute DNS lookups, and stream results as SSE.
//!
//! - `GET /api/query?q=...` — parse a dig-inspired query string.
//! - `POST /api/query` — accept a structured JSON body.
//!
//! Both handlers validate the parsed query against [`QueryPolicy`], build a set
//! of resolvers from the server specs, fan out per-record-type lookups across all
//! resolvers concurrently, and stream batch results as SSE events.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use axum::Json;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::response::sse::Event;
use mhost::RecordType;
use mhost::nameserver::predefined::PredefinedProvider;
use mhost::nameserver::{NameServerConfig, NameServerConfigGroup};
use mhost::resolver::{Lookups, ResolverGroup, ResolverGroupBuilder};
use serde::Deserialize;

use crate::RequestId;
use crate::api::{AppState, respond};
use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::config::Config;
use crate::error::{ApiError, ErrorResponse};
use crate::jobs::DnsErrorEvent;
use crate::parser::{self, ParsedQuery, ServerSpec, Transport};
use crate::result_cache::CachedEvent;
use std::sync::Arc;

/// Extract IP addresses from cached batch events (A/AAAA record values).
pub(crate) fn extract_ips_from_cached_events(events: &[CachedEvent]) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    for event in events {
        if event.event_type != "batch" {
            continue;
        }
        // Walk the JSON: data.lookups.lookups[].result.Response.records[].data.{A,AAAA}
        let lookups = &event.data["lookups"]["lookups"];
        if let Some(arr) = lookups.as_array() {
            for lookup in arr {
                if let Some(records) = lookup["result"]["Response"]["records"].as_array() {
                    for record in records {
                        if let Some(data) = record["data"].as_object() {
                            for key in ["A", "AAAA"] {
                                if let Some(val) = data.get(key).and_then(|v| v.as_str())
                                    && let Ok(ip) = val.parse::<IpAddr>()
                                    && !ips.contains(&ip)
                                {
                                    ips.push(ip);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    ips
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

/// Shared query parameters for all streaming endpoints.
#[derive(Deserialize, utoipa::IntoParams)]
pub struct StreamParams {
    /// Set to `false` to collect all events and return a single JSON response
    /// instead of streaming Server-Sent Events.
    #[serde(default = "default_stream")]
    pub stream: bool,
}

fn default_stream() -> bool {
    true
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct QueryParams {
    /// Query string in dig-inspired syntax: `domain [TYPE...] [@server...] [+flag...]`.
    /// Example: `example.com MX @cloudflare +tls`
    q: Option<String>,
    /// Set to `false` to receive a single JSON response instead of SSE.
    #[serde(default = "default_stream")]
    stream: bool,
}

/// Run a DNS query using the dig-inspired query language.
///
/// Parses the `q` parameter, fans out per-record-type lookups across all specified
/// resolvers concurrently, and streams results as Server-Sent Events.
///
/// ## SSE Events
///
/// - `batch` — one per record type: `{"request_id","record_type","lookups","completed","total"}`
/// - `done` — final summary: `{"request_id","total_queries","duration_ms","warnings","transport","dnssec"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    get, path = "/api/query",
    tag = "Query",
    params(QueryParams),
    responses(
        (status = 200, description = "SSE stream of DNS lookup results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid query syntax, domain, or server)", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy (blocked type, private IP, limits exceeded)", body = ErrorResponse),
        (status = 429, description = "Rate limit exceeded", body = ErrorResponse),
    )
)]
pub async fn get_handler(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
    Query(params): Query<QueryParams>,
) -> Result<Response, ApiError> {
    let q = params
        .q
        .ok_or(ApiError::ParseError("missing query parameter 'q'".into()))?;

    let parsed = parser::parse(&q).map_err(|e| ApiError::ParseError(e.to_string()))?;

    let client_ip = state.ip_extractor.extract(&headers, peer_addr);
    tracing::Span::current().record("client_ip", tracing::field::display(&client_ip));
    tracing::debug!(%client_ip, %peer_addr, "query GET");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .query_parsed(parsed, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(
        job,
        Some((state.result_cache.clone(), "query", q)),
        params.stream,
    )
    .await)
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

/// Run a DNS query using a structured JSON body.
///
/// Fans out per-record-type lookups across all specified resolvers concurrently
/// and streams results as Server-Sent Events.
///
/// ## SSE Events
///
/// - `batch` — one per record type: `{"request_id","record_type","lookups","completed","total"}`
/// - `done` — final summary: `{"request_id","total_queries","duration_ms","warnings","transport","dnssec"}`
/// - `error` — non-fatal error: `{"code","message"}`
#[utoipa::path(
    post, path = "/api/query",
    tag = "Query",
    request_body = PostQueryRequest,
    responses(
        (status = 200, description = "SSE stream of DNS lookup results", content_type = "text/event-stream"),
        (status = 400, description = "Bad request (invalid domain, record type, or server)", body = ErrorResponse),
        (status = 422, description = "Query rejected by policy (blocked type, private IP, limits exceeded)", body = ErrorResponse),
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
    tracing::debug!(%client_ip, %peer_addr, "query POST");

    let limiter = state.hot_state.rate_limiter.load_full();
    let job = state
        .jobs()
        .query(&body, client_ip, &limiter, request_id.0)
        .await?;
    Ok(respond(
        job,
        Some((state.result_cache.clone(), "query", body.domain)),
        stream_params.stream,
    )
    .await)
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct PostQueryRequest {
    /// Domain name to query (e.g. `"example.com"`).
    pub domain: String,
    /// DNS record types to query (e.g. `["A", "MX"]`). Defaults to A, AAAA, CNAME, MX.
    #[serde(default)]
    pub record_types: Vec<String>,
    /// DNS servers to use (e.g. `["cloudflare", "8.8.8.8"]`). Defaults to config default_servers.
    #[serde(default)]
    pub servers: Vec<String>,
    /// Transport: `udp` (default), `tcp`, `tls`, or `https`.
    #[serde(default)]
    pub transport: Option<String>,
    /// Enable DNSSEC mode (adds DNSKEY and DS record types).
    #[serde(default)]
    pub dnssec: bool,
}

/// Convert a structured POST body into a [`ParsedQuery`].
pub(crate) fn convert_post_body(body: PostQueryRequest) -> Result<ParsedQuery, ApiError> {
    let domain = body.domain.to_ascii_lowercase();
    if domain.is_empty() {
        return Err(ApiError::InvalidDomain("empty domain".into()));
    }

    let mut record_types = Vec::new();
    for rt_str in &body.record_types {
        let rt = RecordType::from_str(&rt_str.to_ascii_uppercase())
            .map_err(|_| ApiError::InvalidRecordType(rt_str.clone()))?;
        if !record_types.contains(&rt) {
            record_types.push(rt);
        }
    }

    if record_types.is_empty() {
        if domain.parse::<std::net::IpAddr>().is_ok() {
            record_types.push(RecordType::PTR);
        } else {
            record_types.extend_from_slice(&[
                RecordType::A,
                RecordType::AAAA,
                RecordType::CNAME,
                RecordType::MX,
            ]);
        }
    }

    let mut servers = Vec::new();
    for name in &body.servers {
        let server = parse_server_spec(name)?;
        servers.push(server);
    }

    let transport = body
        .transport
        .as_deref()
        .map(|t| match t.to_ascii_lowercase().as_str() {
            "udp" => Ok(Transport::Udp),
            "tcp" => Ok(Transport::Tcp),
            "tls" => Ok(Transport::Tls),
            "https" => Ok(Transport::Https),
            _ => Err(ApiError::ParseError(format!("unknown transport: {t}"))),
        })
        .transpose()?;

    Ok(ParsedQuery {
        domain,
        record_types,
        servers,
        transport,
        dnssec: body.dnssec,
        short: false,
        recursive: true,
        truncated_servers: false,
        warnings: Vec::new(),
    })
}

pub(crate) fn parse_server_spec(name: &str) -> Result<ServerSpec, ApiError> {
    if name.eq_ignore_ascii_case("system") {
        return Ok(ServerSpec::System);
    }
    if let Ok(provider) = PredefinedProvider::from_str(name) {
        return Ok(ServerSpec::Predefined(provider));
    }
    if let Ok(addr) = name.parse::<std::net::IpAddr>() {
        return Ok(ServerSpec::Ip { addr, port: 53 });
    }
    if let Some((addr_str, port_str)) = name.rsplit_once(':')
        && let (Ok(addr), Ok(port)) = (
            addr_str.parse::<std::net::IpAddr>(),
            port_str.parse::<u16>(),
        )
    {
        return Ok(ServerSpec::Ip { addr, port });
    }
    Err(ApiError::InvalidServer(name.to_owned()))
}

/// Record circuit breaker outcomes from lookup results.
pub(crate) fn record_breaker_outcomes(cb: &Arc<CircuitBreakerRegistry>, lookups: &Lookups) {
    for lookup in lookups.iter() {
        let server_name = lookup.name_server().to_string();
        if lookup.result().is_err() {
            cb.record_failure(&server_name);
        } else {
            cb.record_success(&server_name);
        }
    }
}

/// Resolve the effective server specs for a query, applying config defaults
/// when no servers are specified.
pub(crate) fn effective_server_specs(parsed: &ParsedQuery, config: &Config) -> Vec<ServerSpec> {
    if parsed.servers.is_empty() {
        config
            .dns
            .default_servers
            .iter()
            .filter_map(|s| PredefinedProvider::from_str(s).ok())
            .map(ServerSpec::Predefined)
            .collect()
    } else {
        parsed.servers.clone()
    }
}

/// Derive rate-limiting target keys from server specs.
///
/// Each unique server produces a key for per-target rate limiting:
/// - Predefined provider → lowercase provider name (e.g., `"cloudflare"`)
/// - System → `"system"`
/// - IP → address string (e.g., `"1.1.1.1"`)
pub(crate) fn target_keys_from_servers(servers: &[ServerSpec]) -> Vec<String> {
    servers
        .iter()
        .map(|s| match s {
            ServerSpec::Predefined(p) => p.to_string().to_ascii_lowercase(),
            ServerSpec::System => "system".to_string(),
            ServerSpec::Ip { addr, .. } => addr.to_string(),
        })
        .collect()
}

/// Build a [`ResolverGroup`] from the parsed query's server specs.
///
/// If the query specifies no servers, the config's default servers are used.
/// For predefined providers, configs are filtered to the requested transport
/// (default UDP) and IPv4 only — this avoids hanging on unreachable IPv6 or
/// slow TLS/HTTPS connections. Each provider contributes its primary + secondary
/// IPv4 addresses for the selected transport (typically 2 resolvers).
///
/// Returns the resolver group and a parallel `Vec<String>` of circuit breaker keys
/// (one per resolver, in the same order as `resolvers()`).
pub(crate) async fn build_resolver_group(
    parsed: &ParsedQuery,
    config: &Config,
    timeout: Duration,
) -> Result<(ResolverGroup, Vec<String>), ApiError> {
    let servers = effective_server_specs(parsed, config);

    let mut builder = ResolverGroupBuilder::new().timeout(timeout);
    if !servers.iter().any(|s| matches!(s, ServerSpec::System)) {
        builder = builder.deny_non_global(true);
    }
    let mut breaker_keys: Vec<String> = Vec::new();
    let mut system_added = false;

    for server in &servers {
        match server {
            ServerSpec::Predefined(provider) => {
                let transport = parsed.transport.unwrap_or(Transport::Udp);
                let key = provider.to_string().to_ascii_lowercase();
                for ns_config in provider.configs() {
                    if !matches_transport(&ns_config, transport) {
                        continue;
                    }
                    if !is_ipv4_config(&ns_config) {
                        continue;
                    }
                    builder = builder.nameserver(ns_config);
                    breaker_keys.push(key.clone());
                }
            }
            ServerSpec::System => {
                if !system_added {
                    system_added = true;
                    let system = NameServerConfigGroup::from_system_config()
                        .map_err(|e| ApiError::ResolverError(e.to_string()))?;
                    for ns_config in system {
                        builder = builder.nameserver(ns_config);
                        breaker_keys.push("system".to_string());
                    }
                }
            }
            ServerSpec::Ip { addr, port } => {
                let sock = SocketAddr::new(*addr, *port);
                let ns_config = match parsed.transport {
                    Some(Transport::Tcp) => NameServerConfig::tcp(sock),
                    Some(Transport::Tls) => NameServerConfig::tls(sock, addr.to_string()),
                    Some(Transport::Https) => NameServerConfig::https(sock, addr.to_string()),
                    Some(Transport::Udp) | None => NameServerConfig::udp(sock),
                };
                builder = builder.nameserver(ns_config);
                breaker_keys.push(format!("{addr}:{port}"));
            }
        }
    }

    let group = builder.build().await.map_err(|e| match e {
        mhost::Error::NameServerNotGlobal { name_server } => ApiError::BlockedTargetIp {
            ip: name_server,
            reason: "blocked address range".to_string(),
        },
        e => ApiError::ResolverError(e.to_string()),
    })?;

    Ok((group, breaker_keys))
}

/// Check if a nameserver config uses the specified transport.
fn matches_transport(config: &NameServerConfig, transport: Transport) -> bool {
    matches!(
        (config, transport),
        (NameServerConfig::Udp { .. }, Transport::Udp)
            | (NameServerConfig::Tcp { .. }, Transport::Tcp)
            | (NameServerConfig::Tls { .. }, Transport::Tls)
            | (NameServerConfig::Https { .. }, Transport::Https)
    )
}

/// Check if a nameserver config targets an IPv4 address.
fn is_ipv4_config(config: &NameServerConfig) -> bool {
    let ip = match config {
        NameServerConfig::Udp { ip_addr, .. }
        | NameServerConfig::Tcp { ip_addr, .. }
        | NameServerConfig::Tls { ip_addr, .. }
        | NameServerConfig::Https { ip_addr, .. } => ip_addr,
    };
    ip.is_ipv4()
}

pub(crate) fn make_error_event(code: &str, message: &str) -> Event {
    let payload = DnsErrorEvent::new(code, message);
    Event::default()
        .event("error")
        .json_data(&payload)
        .unwrap_or_else(|_| Event::default().event("error").data("{}"))
}

#[cfg(test)]
pub(super) mod build_resolver_group_tests {
    use super::*;
    use crate::config::{
        CircuitBreakerConfig, DnsConfig, EcosystemConfig, LimitsConfig, ServerConfig,
        TelemetryConfig, TraceConfig,
    };

    pub(super) fn config(allow_system_resolvers: bool) -> Config {
        Config {
            site_name: "prism".to_string(),
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: vec![],
            },
            limits: LimitsConfig {
                per_ip_per_minute: 120,
                per_ip_burst: 40,
                per_target_per_minute: 60,
                per_target_burst: 20,
                global_per_minute: 1000,
                global_burst: 50,
                max_concurrent_connections: 256,
                per_ip_max_streams: 10,
                max_timeout_secs: 10,
                max_record_types: 10,
                max_servers: 4,
            },
            circuit_breaker: CircuitBreakerConfig {
                window_secs: 60,
                cooldown_secs: 30,
                failure_threshold: 0.5,
                min_requests: 5,
            },
            dns: DnsConfig {
                default_servers: vec!["cloudflare".to_owned()],
                allow_system_resolvers,
                allow_arbitrary_servers: false,
            },
            trace: TraceConfig {
                max_hops: 10,
                query_timeout_secs: 3,
            },
            telemetry: TelemetryConfig::default(),
            ecosystem: EcosystemConfig::default(),
            backends: crate::config::BackendsConfig::default(),
        }
    }

    pub(super) fn parsed(servers: Vec<ServerSpec>) -> ParsedQuery {
        ParsedQuery {
            domain: "example.com".to_string(),
            record_types: vec![RecordType::A],
            servers,
            transport: None,
            dnssec: false,
            short: false,
            recursive: true,
            truncated_servers: false,
            warnings: Vec::new(),
        }
    }

    // The parse-time policy check is bypassed by building ParsedQuery directly:
    // build_resolver_group must itself refuse non-global addresses (second layer).
    #[tokio::test]
    async fn build_resolver_group_refuses_non_global_ip_servers() {
        for ip in ["127.0.0.1", "10.0.0.1"] {
            let q = parsed(vec![ServerSpec::Ip {
                addr: ip.parse().unwrap(),
                port: 53,
            }]);
            let res = build_resolver_group(&q, &config(true), Duration::from_secs(1)).await;
            assert!(
                matches!(res, Err(ApiError::BlockedTargetIp { .. })),
                "{ip}: expected BlockedTargetIp, got {:?}",
                res.map(|(_, keys)| keys)
            );
        }
    }

    #[tokio::test]
    async fn build_resolver_group_allows_system_when_configured() {
        let q = parsed(vec![ServerSpec::System]);
        let res = build_resolver_group(&q, &config(true), Duration::from_secs(1)).await;
        assert!(res.is_ok(), "system resolvers: {:?}", res.err());
    }

    #[tokio::test]
    async fn build_resolver_group_allows_predefined_public_provider() {
        let q = parsed(vec![ServerSpec::Predefined(PredefinedProvider::Cloudflare)]);
        let res = build_resolver_group(&q, &config(true), Duration::from_secs(1)).await;
        assert!(res.is_ok(), "cloudflare: {:?}", res.err());
    }
}

#[cfg(test)]
#[path = "query_system_keys_tests.rs"]
mod query_system_keys_tests;
