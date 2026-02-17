//! Query endpoint: parse, validate, execute DNS lookups, and stream results as SSE.
//!
//! - `GET /api/query?q=...` — parse a dig-inspired query string.
//! - `POST /api/query` — accept a structured JSON body.
//!
//! Both handlers validate the parsed query against [`QueryPolicy`], build a set
//! of resolvers from the server specs, fan out per-record-type lookups across all
//! resolvers concurrently, and stream batch results as SSE events.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use mhost::RecordType;
use mhost::nameserver::NameServerConfig;
use mhost::nameserver::predefined::PredefinedProvider;
use mhost::resolver::{Lookups, MultiQuery, Resolver, ResolverGroup, ResolverGroupBuilder};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::api::AppState;
use crate::circuit_breaker::CircuitBreakerRegistry;
use crate::config::Config;
use crate::error::ApiError;
use crate::parser::{self, ParsedQuery, ServerSpec, Transport};
use crate::security::QueryPolicy;

// ---------------------------------------------------------------------------
// SSE event payloads
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct BatchEvent {
    request_id: String,
    record_type: String,
    lookups: Lookups,
    completed: u32,
    total: u32,
}

#[derive(Serialize)]
struct DoneEvent {
    request_id: String,
    total_queries: u32,
    duration_ms: u64,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ErrorEvent {
    code: String,
    message: String,
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct QueryParams {
    q: Option<String>,
}

pub async fn get_handler(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let q = params
        .q
        .ok_or(ApiError::ParseError("missing query parameter 'q'".into()))?;

    let parsed = parser::parse(&q).map_err(|e| ApiError::ParseError(e.to_string()))?;

    execute_query(parsed, state).await
}

// ---------------------------------------------------------------------------
// POST handler
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PostQueryRequest {
    domain: String,
    #[serde(default)]
    record_types: Vec<String>,
    #[serde(default)]
    servers: Vec<PostServerSpec>,
    #[serde(default)]
    transport: Option<String>,
    #[serde(default)]
    dnssec: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum PostServerSpec {
    Named(String),
}

pub async fn post_handler(
    State(state): State<AppState>,
    raw_query: axum::extract::RawQuery,
    Json(body): Json<PostQueryRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Reject POST with a query string -- ambiguous input.
    if raw_query.0.is_some() {
        return Err(ApiError::AmbiguousInput);
    }

    let parsed = convert_post_body(body)?;
    execute_query(parsed, state).await
}

/// Convert a structured POST body into a [`ParsedQuery`].
fn convert_post_body(body: PostQueryRequest) -> Result<ParsedQuery, ApiError> {
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
    for spec in &body.servers {
        let PostServerSpec::Named(name) = spec;
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
        mode: parser::QueryMode::Normal,
        dnssec: body.dnssec,
        warnings: Vec::new(),
    })
}

fn parse_server_spec(name: &str) -> Result<ServerSpec, ApiError> {
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

// ---------------------------------------------------------------------------
// Shared execution pipeline
// ---------------------------------------------------------------------------

async fn execute_query(
    parsed: ParsedQuery,
    state: AppState,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let request_id = uuid::Uuid::now_v7().to_string();

    // Validate against query policy.
    let policy = QueryPolicy::new(&state.config);
    policy.validate(&parsed)?;

    // Determine per-query timeout (clamped to config max, default 10s).
    let timeout_secs = state.config.limits.max_timeout_secs.min(10);
    let timeout = Duration::from_secs(timeout_secs);

    // Build resolver group.
    let resolver_group = build_resolver_group(&parsed, &state.config, timeout).await?;

    // Extract individual resolvers from the group. We call Resolver::lookup()
    // per record type (which is Send — it uses tokio::task::spawn internally)
    // rather than ResolverGroup::lookup() (which is !Send due to ThreadRng in
    // the Uni mode path).
    let resolvers: Vec<Resolver> = resolver_group.resolvers().to_vec();

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);

    let warnings = parsed.warnings.clone();
    let record_types = parsed.record_types.clone();
    let domain = parsed.domain.clone();
    let rid = request_id;
    let circuit_breakers = state.circuit_breakers.clone();

    tokio::spawn(async move {
        let start = Instant::now();
        let total = record_types.len() as u32;
        for (completed, rt) in (0_u32..).zip(record_types.iter()) {
            let query = match MultiQuery::single(domain.as_str(), *rt) {
                Ok(q) => q,
                Err(e) => {
                    let _ = tx
                        .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                        .await;
                    return;
                }
            };

            // Fan out this single-type query across all resolvers concurrently.
            // Each Resolver::lookup() spawns its own tokio task internally.
            let mut handles = Vec::with_capacity(resolvers.len());
            for resolver in &resolvers {
                let r = resolver.clone();
                let q = query.clone();
                handles.push(tokio::spawn(async move { r.lookup(q).await }));
            }

            // Collect results from all resolvers for this record type.
            let mut merged = Lookups::empty();
            for handle in handles {
                match handle.await {
                    Ok(Ok(lookups)) => {
                        record_breaker_outcomes(&circuit_breakers, &lookups);
                        merged = merged.merge(lookups);
                    }
                    Ok(Err(e)) => {
                        let _ = tx
                            .send(Ok(make_error_event("RESOLVER_ERROR", &e.to_string())))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Ok(make_error_event("INTERNAL_ERROR", &e.to_string())))
                            .await;
                    }
                }
            }

            let batch = BatchEvent {
                request_id: rid.clone(),
                record_type: rt.to_string(),
                lookups: merged,
                completed: completed + 1,
                total,
            };
            let event = Event::default()
                .event("batch")
                .json_data(&batch)
                .unwrap_or_else(|_| Event::default().event("batch").data("{}"));
            if tx.send(Ok(event)).await.is_err() {
                return; // Client disconnected.
            }
        }

        // Send done event.
        let done = DoneEvent {
            request_id: rid,
            total_queries: total,
            duration_ms: start.elapsed().as_millis() as u64,
            warnings,
        };
        let event = Event::default()
            .event("done")
            .json_data(&done)
            .unwrap_or_else(|_| Event::default().event("done").data("{}"));
        let _ = tx.send(Ok(event)).await;
    });

    let stream = ReceiverStream::new(rx);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Record circuit breaker outcomes from lookup results.
fn record_breaker_outcomes(cb: &Arc<CircuitBreakerRegistry>, lookups: &Lookups) {
    for lookup in lookups.iter() {
        let server_name = lookup.name_server().to_string();
        if lookup.result().is_err() {
            cb.record_failure(&server_name);
        } else {
            cb.record_success(&server_name);
        }
    }
}

/// Build a [`ResolverGroup`] from the parsed query's server specs.
///
/// If the query specifies no servers, the config's default servers are used.
/// For predefined providers, configs are filtered to the requested transport
/// (default UDP) and IPv4 only — this avoids hanging on unreachable IPv6 or
/// slow TLS/HTTPS connections. Each provider contributes its primary + secondary
/// IPv4 addresses for the selected transport (typically 2 resolvers).
async fn build_resolver_group(
    parsed: &ParsedQuery,
    config: &Config,
    timeout: Duration,
) -> Result<ResolverGroup, ApiError> {
    let servers: Vec<ServerSpec> = if parsed.servers.is_empty() {
        config
            .dns
            .default_servers
            .iter()
            .filter_map(|s| PredefinedProvider::from_str(s).ok())
            .map(ServerSpec::Predefined)
            .collect()
    } else {
        parsed.servers.clone()
    };

    let mut builder = ResolverGroupBuilder::new().timeout(timeout);

    for server in &servers {
        match server {
            ServerSpec::Predefined(provider) => {
                let transport = parsed.transport.unwrap_or(Transport::Udp);
                for ns_config in provider.configs() {
                    if !matches_transport(&ns_config, transport) {
                        continue;
                    }
                    if !is_ipv4_config(&ns_config) {
                        continue;
                    }
                    builder = builder.nameserver(ns_config);
                }
            }
            ServerSpec::System => {
                builder = builder.system();
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
            }
        }
    }

    builder
        .build()
        .await
        .map_err(|e| ApiError::ResolverError(e.to_string()))
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

fn make_error_event(code: &str, message: &str) -> Event {
    let payload = ErrorEvent {
        code: code.to_owned(),
        message: message.to_owned(),
    };
    Event::default()
        .event("error")
        .json_data(&payload)
        .unwrap_or_else(|_| Event::default().event("error").data("{}"))
}
