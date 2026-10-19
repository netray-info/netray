//! The ip endpoints of v1: the caller, one address, a batch, a range, a diff and an ASN, over the
//! `netray_ip::IpLookup` seam. Every handler answers 503 first, charges the per-IP limiter, validates,
//! charges the per-target or batch limiter, then looks up; the caller's `headers`, `cidr` and plain
//! `ip` need no lookup and are answered before the seam check.
use std::cell::Cell;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use netray_common::error::{ErrorInfo, ErrorResponse};
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_ip::backend::Ifconfig;
use netray_ip::extractors::{extract_headers, filter_headers};
use netray_ip::format::{OutputFormat, filter_fields};
use netray_ip::handlers;
use netray_ip::lookup::{
    self, AsnLookupResponse, DiffRequest, DiffResponse, RangeResponse, parse_address,
    parse_diff_pair,
};
use netray_ip::negotiate::{NegotiatedFormat, negotiate};
use netray_ip::{IpLookup, IpRefusal, LookupOptions};
use serde::de::value::{Error as ValueError, StrDeserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::limit;
use super::state::ApiState;

const UNAVAILABLE: &str = "the IP module is off: [modules.ip] is not configured";
const ADDRESS_NAMES: &str = concat!(
    "ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network, ",
    "host, cidr"
);
const CALLER_NAMES: &str = concat!(
    "ip, asn, country, city, region, timezone, latitude, longitude, isp, location, network, ",
    "tcp, user_agent, host, headers, cidr, all"
);
const FORMAT_NAMES: &str = "json, yaml, toml, csv, plain";
const LANG_MAX_LEN: usize = 16;

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
    Host,
    Cidr,
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
    Cidr,
    All,
}

/// The forms of `?format=`.
#[derive(Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum Format {
    Json,
    Yaml,
    Toml,
    Csv,
    Plain,
}

impl From<AddressField> for CallerField {
    fn from(field: AddressField) -> Self {
        match field {
            AddressField::Ip => Self::Ip,
            AddressField::Asn => Self::Asn,
            AddressField::Country => Self::Country,
            AddressField::City => Self::City,
            AddressField::Region => Self::Region,
            AddressField::Timezone => Self::Timezone,
            AddressField::Latitude => Self::Latitude,
            AddressField::Longitude => Self::Longitude,
            AddressField::Isp => Self::Isp,
            AddressField::Location => Self::Location,
            AddressField::Network => Self::Network,
            AddressField::Host => Self::Host,
            AddressField::Cidr => Self::Cidr,
        }
    }
}

/// One answer in both forms: the text and, when the value has one, its JSON value.
struct Answer {
    plain: String,
    json: Option<Value>,
    /// A `?field=` value answers as text unless a form is asked for; the document as JSON.
    field: bool,
}

macro_rules! lookup_field {
    ($module:ident, $ifconfig:expr) => {
        Answer {
            plain: handlers::$module::to_plain($ifconfig),
            json: handlers::$module::to_json($ifconfig),
            field: true,
        }
    };
}

fn cidr_answer(addr: IpAddr) -> Answer {
    let bits = if addr.is_ipv4() { 32 } else { 128 };
    Answer {
        plain: format!("{addr}/{bits}\n"),
        json: None,
        field: true,
    }
}

/// The request's headers as the answers show them: filtered, and the `X-Forwarded-For` chain V1's
/// structured `headers` carried.
pub(super) struct Shown {
    headers: Vec<(String, String)>,
    xff: Vec<String>,
}

fn headers_answer(shown: &Shown) -> Answer {
    Answer {
        plain: handlers::headers::to_plain(&shown.headers),
        json: Some(handlers::headers::to_json_value_with_xff(
            &shown.headers,
            &shown.xff,
        )),
        field: true,
    }
}

/// The value of `field` (the whole document without one) over `ifconfig`, the answer for `addr`.
fn answer(field: Option<CallerField>, ifconfig: &Ifconfig, addr: IpAddr, shown: &Shown) -> Answer {
    let Some(field) = field else {
        return Answer {
            plain: handlers::root::to_plain(ifconfig),
            json: handlers::root::to_json(ifconfig),
            field: false,
        };
    };
    match field {
        CallerField::Ip => lookup_field!(ip, ifconfig),
        CallerField::Asn => lookup_field!(asn, ifconfig),
        CallerField::Country => lookup_field!(country, ifconfig),
        CallerField::City => lookup_field!(city, ifconfig),
        CallerField::Region => lookup_field!(region, ifconfig),
        CallerField::Timezone => lookup_field!(timezone, ifconfig),
        CallerField::Latitude => lookup_field!(latitude, ifconfig),
        CallerField::Longitude => lookup_field!(longitude, ifconfig),
        CallerField::Isp => lookup_field!(isp, ifconfig),
        CallerField::Location => lookup_field!(location, ifconfig),
        CallerField::Network => lookup_field!(network, ifconfig),
        CallerField::Tcp => lookup_field!(tcp, ifconfig),
        CallerField::UserAgent => lookup_field!(user_agent, ifconfig),
        CallerField::Host => lookup_field!(host, ifconfig),
        CallerField::Headers => headers_answer(shown),
        CallerField::Cidr => cidr_answer(addr),
        CallerField::All => {
            let mut json = handlers::all::to_json(ifconfig);
            if let Some(Value::Object(map)) = &mut json {
                map.insert(
                    "headers".into(),
                    handlers::headers::to_json_value(&shown.headers),
                );
            }
            Answer {
                plain: format!(
                    "{}{}",
                    handlers::all::to_plain(ifconfig),
                    handlers::headers::to_plain(&shown.headers)
                ),
                json,
                field: true,
            }
        }
    }
}

/// The caller's answers that need no lookup: the request's headers, the address as a prefix, and the
/// address itself as text.
fn offline_answer(
    field: Option<CallerField>,
    client: IpAddr,
    shown: &Shown,
    plain_form: bool,
) -> Option<Answer> {
    match field? {
        CallerField::Headers => Some(headers_answer(shown)),
        CallerField::Cidr => Some(cidr_answer(client)),
        CallerField::Ip if plain_form => Some(Answer {
            plain: format!("{client}\n"),
            json: None,
            field: true,
        }),
        _ => None,
    }
}

fn field_of<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    T::deserialize(StrDeserializer::<ValueError>::new(name)).ok()
}

#[derive(Deserialize)]
pub(super) struct IpQuery {
    pub(super) field: Option<String>,
    ip: Option<String>,
    dns: Option<String>,
    lang: Option<String>,
    fields: Option<String>,
    pub(super) format: Option<String>,
}

type QueryResult = Result<Query<IpQuery>, QueryRejection>;

fn ip_query(query: QueryResult) -> Result<IpQuery, V1Error> {
    let Query(query) = query.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    Ok(query)
}

/// `?dns=`: `true` or `1` in any case turns reverse DNS on, anything else off.
fn dns_on(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

fn valid_lang(lang: &str) -> bool {
    !lang.is_empty()
        && lang.len() <= LANG_MAX_LEN
        && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The form an answer is rendered in when it is not the default.
pub(super) enum Form {
    Plain,
    Structured(OutputFormat, String),
}

pub(super) fn form_of(name: &str) -> Result<Form, V1Error> {
    if name == "plain" {
        return Ok(Form::Plain);
    }
    OutputFormat::from_name(name)
        .map(|format| Form::Structured(format, name.to_string()))
        .ok_or_else(|| {
            V1Error::InvalidInput(format!(
                "unknown format `{name}`; valid formats: {FORMAT_NAMES}"
            ))
        })
}

/// An explicit `?format=` decides; else an `Accept` naming yaml, toml or csv; else the default.
fn choose_form(format: Option<&str>, headers: &HeaderMap) -> Result<Option<Form>, V1Error> {
    if let Some(name) = format {
        return form_of(name).map(Some);
    }
    let name = match negotiate(None, headers) {
        NegotiatedFormat::Yaml => "yaml",
        NegotiatedFormat::Toml => "toml",
        NegotiatedFormat::Csv => "csv",
        _ => return Ok(None),
    };
    form_of(name).map(Some)
}

/// The validated parameters of the lookup and its rendering.
struct Params {
    options: LookupOptions,
    fields: Option<HashSet<String>>,
    form: Option<Form>,
    label: String,
}

impl Params {
    /// `preset` is a form the route already decided; without it `?format=` and `Accept` decide.
    fn new(query: &IpQuery, headers: &HeaderMap, preset: Option<Form>) -> Result<Self, V1Error> {
        if let Some(lang) = query.lang.as_deref().filter(|lang| !valid_lang(lang)) {
            return Err(V1Error::InvalidInput(format!(
                "invalid lang `{lang}`: at most {LANG_MAX_LEN} ASCII letters, digits and `-`"
            )));
        }
        Ok(Self {
            options: LookupOptions {
                reverse_dns: query.dns.as_deref().is_none_or(dns_on),
                lang: query.lang.clone(),
            },
            fields: query.fields.as_deref().map(|fields| {
                fields
                    .split(',')
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .collect()
            }),
            form: match preset {
                Some(form) => Some(form),
                None => choose_form(query.format.as_deref(), headers)?,
            },
            label: query.field.clone().unwrap_or_else(|| "document".into()),
        })
    }

    fn plain_form(&self) -> bool {
        matches!(self.form, None | Some(Form::Plain))
    }
}

fn respond(answer: Answer, params: &Params) -> Result<Response, V1Error> {
    let Some(value) = answer.json else {
        return Ok(plain(answer.plain));
    };
    let value = match &params.fields {
        Some(fields) => filter_fields(value, fields),
        None => value,
    };
    match &params.form {
        Some(Form::Plain) => Ok(plain(answer.plain)),
        None if answer.field => Ok(plain(answer.plain)),
        None => Ok(Json(value).into_response()),
        Some(Form::Structured(format, name)) => {
            let body = format.serialize_body(&value).ok_or_else(|| {
                V1Error::InvalidInput(format!("{} has no {name} form", params.label))
            })?;
            Ok(([(header::CONTENT_TYPE, format.content_type())], body).into_response())
        }
    }
}

/// The answer over a lookup; the plain document with no form, no `fields` and no field is the
/// lookup itself.
fn render(
    field: Option<CallerField>,
    ifconfig: Ifconfig,
    addr: IpAddr,
    shown: &Shown,
    params: &Params,
) -> Result<Response, V1Error> {
    if field.is_none() && params.form.is_none() && params.fields.is_none() {
        return Ok(Json(ifconfig).into_response());
    }
    respond(answer(field, &ifconfig, addr, shown), params)
}

fn caller_field(name: Option<&str>) -> Result<Option<CallerField>, V1Error> {
    let Some(name) = name else { return Ok(None) };
    field_of(name).map(Some).ok_or_else(|| {
        V1Error::InvalidInput(format!(
            "unknown field `{name}`; valid fields: {CALLER_NAMES}"
        ))
    })
}

fn address_field(name: Option<&str>) -> Result<Option<CallerField>, V1Error> {
    let Some(name) = name else { return Ok(None) };
    if let Some(field) = field_of::<AddressField>(name) {
        return Ok(Some(field.into()));
    }
    Err(V1Error::InvalidInput(
        if field_of::<CallerField>(name).is_some() {
            format!("field `{name}` describes the caller: use /api/v1/ip?field={name}")
        } else {
            format!("unknown field `{name}`; valid fields: {ADDRESS_NAMES}")
        },
    ))
}

/// The field of `/ip?ip=X`: the address fields and what the request carries, not its connection.
fn queried_field(name: Option<&str>) -> Result<Option<CallerField>, V1Error> {
    let field = caller_field(name)?;
    if let (Some(CallerField::Tcp | CallerField::UserAgent), Some(name)) = (field, name) {
        return Err(V1Error::InvalidInput(format!(
            "field `{name}` describes the caller's connection: it has no answer with ?ip="
        )));
    }
    Ok(field)
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

pub(super) fn charge(
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

tokio::task_local! {
    static REMAINING: Cell<Option<u32>>;
}

fn note_remaining(remaining: u32) {
    let _ = REMAINING.try_with(|slot| slot.set(Some(remaining)));
}

const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// Adds the metadata headers to an ip route's answer: the rate-limit pair, the GeoIP database date and
/// age, and for a 200 GET a strong ETag, `Last-Modified` and the 304 for a matching `If-None-Match`.
pub(super) async fn metadata(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let is_get = request.method() == Method::GET;
    let if_none_match = request.headers().get(header::IF_NONE_MATCH).cloned();
    with_metadata(&state, is_get, if_none_match, next.run(request)).await
}

/// `answer` with the metadata headers of an ip route; `is_get` allows the ETag and the 304.
pub(super) async fn with_metadata(
    state: &ApiState,
    is_get: bool,
    if_none_match: Option<HeaderValue>,
    answer: impl Future<Output = Response>,
) -> Response {
    let (mut response, remaining) = REMAINING
        .scope(Cell::new(None), async {
            let response = answer.await;
            let remaining = REMAINING.with(Cell::get);
            (response, remaining)
        })
        .await;
    // Any 429 (per-IP, per-target or batch) tells the client it has nothing left.
    let remaining = if response.status() == StatusCode::TOO_MANY_REQUESTS {
        Some(0)
    } else {
        remaining
    };
    let headers = response.headers_mut();
    // The answer depends on the caller, its agent and its Accept: no shared cache, and every
    // reuse revalidates against the ETag (V1 sent `Vary: Accept, User-Agent` too).
    headers.insert(header::VARY, HeaderValue::from_static("Accept, User-Agent"));
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    headers.insert(
        "x-ratelimit-limit",
        HeaderValue::from(state.config().ip.per_ip_per_minute),
    );
    if let Some(remaining) = remaining {
        headers.insert("x-ratelimit-remaining", HeaderValue::from(remaining));
    }
    let date = state
        .ip_lookup()
        .and_then(|ip| ip.geoip_build_epoch())
        .and_then(|epoch| {
            let built = UNIX_EPOCH + Duration::from_secs(epoch);
            let age = SystemTime::now().duration_since(built).unwrap_or_default();
            HeaderValue::from_str(&httpdate::fmt_http_date(built))
                .ok()
                .map(|date| (date, age.as_secs() / 86400))
        });
    if let Some((date, age_days)) = &date {
        headers.insert("x-geoip-database-date", date.clone());
        headers.insert("x-geoip-database-age-days", HeaderValue::from(*age_days));
    }
    if !is_get || response.status() != StatusCode::OK {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, BODY_LIMIT).await else {
        return Response::from_parts(parts, Body::empty());
    };
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let etag = HeaderValue::from_str(&format!("\"{:016x}\"", hasher.finish()))
        .expect("a hex digest in quotes is a header value");
    parts.headers.insert(header::ETAG, etag.clone());
    if let Some((date, _)) = date {
        parts.headers.insert(header::LAST_MODIFIED, date);
    }
    let matches = if_none_match
        .as_ref()
        .and_then(|v| v.to_str().ok())
        .is_some_and(|list| {
            list.split(',')
                .map(str::trim)
                .any(|tag| tag == "*" || tag.as_bytes() == etag.as_bytes())
        });
    if matches {
        parts.status = StatusCode::NOT_MODIFIED;
        parts.headers.remove(header::CONTENT_LENGTH);
        parts.headers.remove(header::CONTENT_TYPE);
        return Response::from_parts(parts, Body::empty());
    }
    Response::from_parts(parts, Body::from(bytes))
}

fn charge_caller(
    state: &ApiState,
    headers: &HeaderMap,
    peer: Option<SocketAddr>,
    cost: u32,
) -> Result<IpAddr, V1Error> {
    let client = limit::client(state, headers, peer);
    state.ip_limiters().prune_if_due();
    match state.ip_limiters().charge_per_ip(client, cost) {
        Ok(remaining) => note_remaining(remaining),
        Err(retry_after_secs) => {
            note_remaining(0);
            return Err(V1Error::RateLimited { retry_after_secs });
        }
    }
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

fn shown_headers(state: &ApiState, headers: &HeaderMap) -> Shown {
    let xff = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    Shown {
        headers: filter_headers(extract_headers(headers), state.header_filters()),
        xff,
    }
}

/// One global address, as `/ip/{address}` and `/ip?ip=` answer it. `address_path` is the former:
/// its fields are the address fields only.
pub(super) async fn answer_address(
    state: &ApiState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    address: &str,
    query: &IpQuery,
    address_path: bool,
    preset: Option<Form>,
) -> Result<Response, V1Error> {
    let name = query.field.as_deref();
    let field = if address_path {
        address_field(name)?
    } else {
        queried_field(name)?
    };
    let params = Params::new(query, headers, preset)?;
    let target = parse_address(address, state.config().ip.internal_mode)?;
    let shown = shown_headers(state, headers);
    if let Some(answer) = offline_answer(field, target, &shown, false) {
        charge_caller(state, headers, peer, 1)?;
        return respond(answer, &params);
    }
    let ip = seam(state)?;
    charge_caller(state, headers, peer, 1)?;
    charge(
        &state.ip_limiters().per_target,
        target,
        1,
        "api_v1_ip_target",
    )?;
    let ifconfig = ip.lookup_with(target, params.options.clone()).await?;
    render(field, ifconfig, target, &shown, &params)
}

/// The caller's address, or one field of it; with `?ip=`, that address instead.
#[utoipa::path(
    get,
    path = "/ip",
    tag = "ip",
    params(
        ("field" = Option<CallerField>, Query, description = "Answer this one field as text/plain, with a trailing newline"),
        ("ip" = Option<String>, Query, description = "Answer for this address instead of the caller's"),
        ("dns" = Option<String>, Query, description = "Reverse DNS: `true` or `1` (default) looks it up, anything else leaves the hostname null"),
        ("lang" = Option<String>, Query, description = "Language tag of the localized location names: up to 16 ASCII letters, digits and `-`"),
        ("fields" = Option<String>, Query, description = "Comma-separated top-level keys to keep in a structured answer"),
        ("format" = Option<Format>, Query, description = "Render the answer in this form; without it, an `Accept` of yaml, toml or csv decides"),
    ),
    responses(
        (
            status = 200,
            description = "The lookup; with `?field`, that one value as text/plain",
            content(
                (Ifconfig = "application/json"),
                (String = "application/yaml"),
                (String = "application/toml"),
                (String = "text/csv"),
                (String = "text/plain"),
            )
        ),
        (status = 400, description = "Unknown field or format, malformed `lang`, or an `ip` that is malformed or non-global", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The IP module is off", body = ErrorResponse),
    )
)]
pub(super) async fn caller(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    query: QueryResult,
) -> Result<Response, V1Error> {
    let query = ip_query(query)?;
    answer_caller(&state, peer_of(peer), &headers, &query, None).await
}

/// The caller's answer, or with `?ip=` that address's: the field is `query.field`, the form `preset`
/// or else what `?format=` and `Accept` pick.
pub(super) async fn answer_caller(
    state: &ApiState,
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    query: &IpQuery,
    preset: Option<Form>,
) -> Result<Response, V1Error> {
    if let Some(address) = &query.ip {
        return answer_address(state, peer, headers, address, query, false, preset).await;
    }
    let field = caller_field(query.field.as_deref())?;
    let params = Params::new(query, headers, preset)?;
    let shown = shown_headers(state, headers);
    let client = limit::client(state, headers, peer);
    if let Some(answer) = offline_answer(field, client, &shown, params.plain_form()) {
        charge_caller(state, headers, peer, 1)?;
        return respond(answer, &params);
    }
    let ip = seam(state)?;
    charge_caller(state, headers, peer, 1)?;
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
    let ifconfig = ip
        .lookup_caller_with(remote, user_agent, params.options.clone())
        .await?;
    render(field, ifconfig, client, &shown, &params)
}

/// One global address, or one field of it.
#[utoipa::path(
    get,
    path = "/ip/{address}",
    tag = "ip",
    params(
        ("address" = String, Path, description = "An IPv4 or IPv6 address"),
        ("field" = Option<AddressField>, Query, description = "Answer this one field as text/plain, with a trailing newline"),
        ("dns" = Option<String>, Query, description = "Reverse DNS: `true` or `1` (default) looks it up, anything else leaves the hostname null"),
        ("lang" = Option<String>, Query, description = "Language tag of the localized location names: up to 16 ASCII letters, digits and `-`"),
        ("fields" = Option<String>, Query, description = "Comma-separated top-level keys to keep in a structured answer"),
        ("format" = Option<Format>, Query, description = "Render the answer in this form; without it, an `Accept` of yaml, toml or csv decides"),
    ),
    responses(
        (
            status = 200,
            description = "The address's lookup; with `?field`, that one value as text/plain",
            content(
                (Ifconfig = "application/json"),
                (String = "application/yaml"),
                (String = "application/toml"),
                (String = "text/csv"),
                (String = "text/plain"),
            )
        ),
        (status = 400, description = "Malformed or non-global address, caller-only or unknown field, unknown format or malformed `lang`", body = ErrorResponse),
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
    query: QueryResult,
) -> Result<Response, V1Error> {
    let query = ip_query(query)?;
    let Path(address) = address.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    answer_address(
        &state,
        peer_of(peer),
        &headers,
        &address,
        &query,
        true,
        None,
    )
    .await
}

/// The query of a batch lookup.
#[derive(Deserialize)]
pub(super) struct BatchQuery {
    dns: Option<String>,
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
    params(("dns" = Option<String>, Query, description = "Reverse DNS per address: `true` or `1` looks it up; without it the hostname is null")),
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
    query: Result<Query<BatchQuery>, QueryRejection>,
    body: Result<Json<BatchRequest>, JsonRejection>,
) -> Result<Json<BatchResponse>, V1Error> {
    let ip = seam(&state)?;
    let client = charge_caller(&state, &headers, peer_of(peer), 1)?;
    let Query(BatchQuery { dns }) = query.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    let options = LookupOptions {
        reverse_dns: dns.as_deref().is_some_and(dns_on),
        lang: None,
    };
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
    let mut answers =
        lookup::lookup_batch(&targets, |target| ip.lookup_with(target, options.clone()))
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
pub(super) fn ip_routes(state: ApiState) -> Router<ApiState> {
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
        .route_layer(axum::middleware::from_fn_with_state(state, metadata))
}
