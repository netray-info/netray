//! The ip endpoints of v1: the caller, one address, a batch, a range, a diff and an ASN, over the
//! `netray_ip::IpLookup` seam. Every handler answers 503 first, charges the per-IP limiter, validates,
//! charges the per-target or batch limiter, then looks up.
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use netray_common::error::{ErrorInfo, ErrorResponse};
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_ip::backend::Ifconfig;
use netray_ip::extractors::{extract_headers, filter_headers};
use netray_ip::handlers;
use netray_ip::lookup::{
    self, AsnLookupResponse, DiffRequest, DiffResponse, RangeResponse, parse_address,
    parse_diff_pair,
};
use netray_ip::{IpLookup, IpRefusal};
use serde::de::value::{Error as ValueError, StrDeserializer};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::limit;
use super::state::ApiState;

const UNAVAILABLE: &str = "the IP module is off: [modules.ip] is not configured";
const ADDRESS_NAMES: &str =
    "ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network";
const CALLER_NAMES: &str = concat!(
    "ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network, ",
    "tcp, user_agent, host, headers"
);

/// The fields of `?field=` that describe an address.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum AddressField {
    Ip,
    Asn,
    Country,
    City,
    Region,
    Timezone,
    Latitude,
    Longitude,
    Isp,
    Location,
    Network,
}

/// The fields of `?field=` on the caller's own address: the address fields and what the request carries.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum CallerField {
    Ip,
    Asn,
    Country,
    City,
    Region,
    Timezone,
    Latitude,
    Longitude,
    Isp,
    Location,
    Network,
    Tcp,
    UserAgent,
    Host,
    Headers,
}

impl AddressField {
    fn plain(self, ifconfig: &Ifconfig) -> String {
        match self {
            Self::Ip => handlers::ip::to_plain(ifconfig),
            Self::Asn => handlers::asn::to_plain(ifconfig),
            Self::Country => handlers::country::to_plain(ifconfig),
            Self::City => handlers::city::to_plain(ifconfig),
            Self::Region => handlers::region::to_plain(ifconfig),
            Self::Timezone => handlers::timezone::to_plain(ifconfig),
            Self::Latitude => handlers::latitude::to_plain(ifconfig),
            Self::Longitude => handlers::longitude::to_plain(ifconfig),
            Self::Isp => handlers::isp::to_plain(ifconfig),
            Self::Location => handlers::location::to_plain(ifconfig),
            Self::Network => handlers::network::to_plain(ifconfig),
        }
    }
}

impl CallerField {
    /// The value rendered from the lookup; `None` for `headers`, which comes from the request.
    fn plain(self, ifconfig: &Ifconfig) -> Option<String> {
        Some(match self {
            Self::Ip => handlers::ip::to_plain(ifconfig),
            Self::Asn => handlers::asn::to_plain(ifconfig),
            Self::Country => handlers::country::to_plain(ifconfig),
            Self::City => handlers::city::to_plain(ifconfig),
            Self::Region => handlers::region::to_plain(ifconfig),
            Self::Timezone => handlers::timezone::to_plain(ifconfig),
            Self::Latitude => handlers::latitude::to_plain(ifconfig),
            Self::Longitude => handlers::longitude::to_plain(ifconfig),
            Self::Isp => handlers::isp::to_plain(ifconfig),
            Self::Location => handlers::location::to_plain(ifconfig),
            Self::Network => handlers::network::to_plain(ifconfig),
            Self::Tcp => handlers::tcp::to_plain(ifconfig),
            Self::UserAgent => handlers::user_agent::to_plain(ifconfig),
            Self::Host => handlers::host::to_plain(ifconfig),
            Self::Headers => return None,
        })
    }
}

fn field_of<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    T::deserialize(StrDeserializer::<ValueError>::new(name)).ok()
}

#[derive(Deserialize)]
pub(super) struct FieldQuery {
    field: Option<String>,
}

fn field_name(query: Result<Query<FieldQuery>, QueryRejection>) -> Result<Option<String>, V1Error> {
    let Query(FieldQuery { field }) = query.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    Ok(field)
}

fn caller_field(name: Option<String>) -> Result<Option<CallerField>, V1Error> {
    let Some(name) = name else { return Ok(None) };
    field_of(&name).map(Some).ok_or_else(|| {
        V1Error::InvalidInput(format!(
            "unknown field `{name}`; valid fields: {CALLER_NAMES}"
        ))
    })
}

fn address_field(name: Option<String>) -> Result<Option<AddressField>, V1Error> {
    let Some(name) = name else { return Ok(None) };
    if let Some(field) = field_of(&name) {
        return Ok(Some(field));
    }
    Err(V1Error::InvalidInput(
        if field_of::<CallerField>(&name).is_some() {
            format!("field `{name}` describes the caller: use /api/v1/ip?field={name}")
        } else {
            format!("unknown field `{name}`; valid fields: {ADDRESS_NAMES}")
        },
    ))
}

fn plain(body: String) -> Response {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

fn seam(state: &ApiState) -> Result<Arc<dyn IpLookup>, V1Error> {
    state
        .ip_lookup()
        .cloned()
        .ok_or_else(|| V1Error::Unavailable(UNAVAILABLE.into()))
}

fn charge(
    limiter: &KeyedLimiter<IpAddr>,
    key: IpAddr,
    cost: u32,
    scope: &'static str,
) -> Result<(), V1Error> {
    let cost = NonZeroU32::new(cost).unwrap_or(NonZeroU32::MIN);
    check_keyed_cost(limiter, &key, cost, scope, "lens").map_err(|rejection| V1Error::RateLimited {
        retry_after_secs: rejection.retry_after_secs,
    })
}

fn charge_caller(
    state: &ApiState,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    cost: u32,
) -> Result<IpAddr, V1Error> {
    let client = limit::client(state, headers, peer);
    state.ip_limiters().prune_if_due();
    charge(&state.ip_limiters().per_ip, client, cost, "api_v1_ip")?;
    Ok(client)
}

pub(super) type Peer = Option<Extension<ConnectInfo<SocketAddr>>>;

pub(super) fn peer_of(peer: Peer) -> Option<SocketAddr> {
    peer.map(|Extension(ConnectInfo(addr))| addr)
}

pub(super) fn json_body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, V1Error> {
    match body {
        Ok(Json(value)) => Ok(value),
        Err(e) if e.status() == StatusCode::PAYLOAD_TOO_LARGE => Err(V1Error::PayloadTooLarge),
        Err(e) => Err(V1Error::InvalidInput(e.body_text())),
    }
}

/// The caller's address, or one field of it.
#[utoipa::path(
    get,
    path = "/ip",
    tag = "ip",
    params(("field" = Option<CallerField>, Query, description = "Answer this one field as text/plain, with a trailing newline")),
    responses(
        (
            status = 200,
            description = "The caller's lookup; with `?field`, that one value as text/plain",
            content(
                (Ifconfig = "application/json"),
                (String = "text/plain"),
            )
        ),
        (status = 400, description = "Unknown field", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn caller(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    query: Result<Query<FieldQuery>, QueryRejection>,
) -> Result<Response, V1Error> {
    let ip = seam(&state)?;
    let peer = peer_of(peer);
    let client = charge_caller(&state, &headers, peer, 1)?;
    let field = caller_field(field_name(query)?)?;
    if let Some(CallerField::Headers) = field {
        let shown = filter_headers(extract_headers(&headers), state.header_filters());
        return Ok(plain(handlers::headers::to_plain(&shown)));
    }
    charge(
        &state.ip_limiters().per_target,
        client,
        1,
        "api_v1_ip_target",
    )?;
    let remote = SocketAddr::new(client, peer.map_or(0, |p| p.port()));
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok());
    let ifconfig = ip.lookup_caller(remote, user_agent).await?;
    Ok(match field.and_then(|f| f.plain(&ifconfig)) {
        Some(body) => plain(body),
        None => Json(ifconfig).into_response(),
    })
}

/// One global address, or one field of it.
#[utoipa::path(
    get,
    path = "/ip/{address}",
    tag = "ip",
    params(
        ("address" = String, Path, description = "An IPv4 or IPv6 address"),
        ("field" = Option<AddressField>, Query, description = "Answer this one field as text/plain, with a trailing newline"),
    ),
    responses(
        (
            status = 200,
            description = "The address's lookup; with `?field`, that one value as text/plain",
            content(
                (Ifconfig = "application/json"),
                (String = "text/plain"),
            )
        ),
        (status = 400, description = "Malformed or non-global address, caller-only or unknown field", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn address(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    address: Result<Path<String>, PathRejection>,
    query: Result<Query<FieldQuery>, QueryRejection>,
) -> Result<Response, V1Error> {
    let ip = seam(&state)?;
    charge_caller(&state, &headers, peer_of(peer), 1)?;
    let field = address_field(field_name(query)?)?;
    let Path(address) = address.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    let target = parse_address(&address, state.config().ip.internal_mode)?;
    charge(
        &state.ip_limiters().per_target,
        target,
        1,
        "api_v1_ip_target",
    )?;
    let ifconfig = ip.lookup(target).await?;
    Ok(match field {
        Some(field) => plain(field.plain(&ifconfig)),
        None => Json(ifconfig).into_response(),
    })
}

/// The addresses of a batch lookup.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchRequest {
    /// At most `batch_max_size` addresses, answered in this order.
    addresses: Vec<String>,
}

/// One answer per requested address, in request order.
#[derive(Serialize, ToSchema)]
pub(super) struct BatchResponse {
    items: Vec<BatchItem>,
}

/// The lookup of one address, or why it was not answered.
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(super) enum BatchItem {
    Result {
        /// The address as requested.
        address: String,
        result: Box<Ifconfig>,
    },
    Error {
        /// The address as requested.
        address: String,
        error: ErrorInfo,
    },
}

fn item_error(address: String, code: &'static str, message: impl Into<String>) -> BatchItem {
    BatchItem::Error {
        address,
        error: ErrorInfo {
            code,
            message: message.into(),
        },
    }
}

fn refusal_item(address: String, refusal: IpRefusal) -> BatchItem {
    match refusal {
        IpRefusal::InvalidInput(message) => item_error(address, "INVALID_INPUT", message),
        IpRefusal::Unavailable(message) => item_error(address, "UNAVAILABLE", message),
    }
}

/// Up to `batch_max_size` addresses in one request.
#[utoipa::path(
    post,
    path = "/ip/batch",
    tag = "ip",
    request_body = BatchRequest,
    responses(
        (status = 200, description = "One item per address, in order", body = BatchResponse),
        (status = 400, description = "Malformed body, no addresses, or `BATCH_TOO_MANY`", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn batch(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<BatchRequest>, JsonRejection>,
) -> Result<Json<BatchResponse>, V1Error> {
    let ip = seam(&state)?;
    let client = charge_caller(&state, &headers, peer_of(peer), 1)?;
    let BatchRequest { addresses } = json_body(body)?;
    let config = &state.config().ip;
    if addresses.is_empty() {
        return Err(V1Error::InvalidInput("addresses is empty".into()));
    }
    if addresses.len() > config.batch_max_size as usize {
        return Err(V1Error::BatchTooMany(format!(
            "batch size exceeds limit ({})",
            config.batch_max_size
        )));
    }
    let count = u32::try_from(addresses.len()).unwrap_or(u32::MAX);
    charge(
        &state.ip_limiters().batch_addresses,
        client,
        count,
        "api_v1_ip_batch",
    )?;

    let mut parsed: Vec<Result<IpAddr, BatchItem>> = Vec::with_capacity(addresses.len());
    for address in &addresses {
        parsed.push(match parse_address(address, config.internal_mode) {
            Err(refusal) => Err(refusal_item(address.clone(), refusal)),
            Ok(target) => match charge(
                &state.ip_limiters().per_target,
                target,
                1,
                "api_v1_ip_target",
            ) {
                Ok(()) => Ok(target),
                Err(_) => Err(item_error(
                    address.clone(),
                    "RATE_LIMITED",
                    "target rate limit exceeded",
                )),
            },
        });
    }
    let targets: Vec<IpAddr> = parsed
        .iter()
        .filter_map(|p| p.as_ref().ok())
        .copied()
        .collect();
    let mut answers = lookup::lookup_batch(&targets, |target| ip.lookup(target))
        .await
        .into_iter();
    let items = addresses
        .into_iter()
        .zip(parsed)
        .map(|(address, parsed)| match parsed {
            Err(item) => item,
            Ok(_) => match answers.next() {
                Some(Ok(result)) => BatchItem::Result {
                    address,
                    result: Box::new(result),
                },
                Some(Err(refusal)) => refusal_item(address, refusal),
                None => item_error(address, "UNAVAILABLE", "lookup was not answered"),
            },
        })
        .collect();
    Ok(Json(BatchResponse { items }))
}

/// What is known about a CIDR prefix.
#[utoipa::path(
    get,
    path = "/ip/range/{cidr}",
    tag = "ip",
    params(("cidr" = String, Path, description = "A CIDR prefix; the slash may be sent as `%2F`")),
    responses(
        (status = 200, description = "The prefix's classification", body = RangeResponse),
        (status = 400, description = "Malformed or private, loopback or link-local prefix", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn range(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    cidr: Result<Path<String>, PathRejection>,
) -> Result<Json<RangeResponse>, V1Error> {
    let ip = seam(&state)?;
    charge_caller(&state, &headers, peer_of(peer), 1)?;
    let Path(cidr) = cidr.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    Ok(Json(ip.range(&cidr).await?))
}

async fn range_missing() -> V1Error {
    V1Error::InvalidInput("missing CIDR prefix: use /api/v1/ip/range/{cidr}".into())
}

/// Two addresses side by side.
#[utoipa::path(
    post,
    path = "/ip/diff",
    tag = "ip",
    request_body = DiffRequest,
    responses(
        (status = 200, description = "Both lookups, part by part, with whether the parts are equal", body = DiffResponse),
        (status = 400, description = "Malformed body, malformed or non-global address", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn compare(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<DiffRequest>, JsonRejection>,
) -> Result<Json<DiffResponse>, V1Error> {
    let ip = seam(&state)?;
    charge_caller(&state, &headers, peer_of(peer), 2)?;
    let request = json_body(body)?;
    let (a, b) = parse_diff_pair(&request, state.config().ip.internal_mode)?;
    let (a, b) = futures::join!(ip.lookup(a), ip.lookup(b));
    Ok(Json(lookup::diff(a?, b?)))
}

/// What is known about an autonomous system.
#[utoipa::path(
    get,
    path = "/asn/{n}",
    tag = "ip",
    params(("n" = String, Path, description = "The AS number, digits only")),
    responses(
        (status = 200, description = "The AS's classification", body = AsnLookupResponse),
        (status = 400, description = "Not a number", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn asn(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    n: Result<Path<String>, PathRejection>,
) -> Result<Json<AsnLookupResponse>, V1Error> {
    let ip = seam(&state)?;
    charge_caller(&state, &headers, peer_of(peer), 1)?;
    let Path(n) = n.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    let number = (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .then(|| n.parse::<u32>().ok())
        .flatten()
        .ok_or_else(|| V1Error::InvalidInput(format!("ASN `{n}` is not a number")))?;
    Ok(Json(ip.asn(number).await?))
}

/// The six endpoints with their documents, for `openapi()`; the range path is the document's
/// `{cidr}`, which the router cannot serve (see `ip_routes`).
pub(super) fn ip_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(caller))
        .routes(routes!(address))
        .routes(routes!(batch))
        .routes(routes!(range))
        .routes(routes!(compare))
        .routes(routes!(asn))
}

/// The routes the router serves: `{cidr}` would not match a prefix's unescaped slash, so the range
/// is registered as `{*cidr}` by hand, with a bare `/ip/range` answering 400.
pub(super) fn ip_routes() -> Router<ApiState> {
    let (router, _) = OpenApiRouter::new()
        .routes(routes!(caller))
        .routes(routes!(address))
        .routes(routes!(batch))
        .routes(routes!(compare))
        .routes(routes!(asn))
        .split_for_parts();
    router
        .route("/ip/range/{*cidr}", get(range))
        .route("/ip/range", get(range_missing))
}
