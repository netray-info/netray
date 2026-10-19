//! The V1 path family of the `ip.` host (`/`, `/json`, `/country`, `/country/yaml`, `/ip/cidr`, ...)
//! over the `/api/v1/ip` answers: the same renderers, limiters, refusals and metadata headers. A
//! request it does not serve comes back untouched for the next handler.
use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{ConnectInfo, Query, Request};
use axum::http::{HeaderMap, HeaderValue, Method, header};
use axum::response::{IntoResponse, Response};
use netray_ip::negotiate::{NegotiatedFormat, is_cli_client, negotiate};

use super::error::V1Error;
use super::ip::{Form, IpQuery, answer_caller, form_of, with_metadata};
use super::state::ApiState;

const FIELDS: [&str; 16] = [
    "ip",
    "tcp",
    "location",
    "user_agent",
    "network",
    "host",
    "isp",
    "country",
    "city",
    "asn",
    "timezone",
    "latitude",
    "longitude",
    "region",
    "headers",
    "all",
];
const FORMATS: [&str; 4] = ["json", "yaml", "toml", "csv"];

/// The part of a path `ip_host` serves: a field (the document without one) and a format suffix.
struct Route<'a> {
    field: Option<&'a str>,
    suffix: Option<&'a str>,
}

fn route(path: &str) -> Option<Route<'_>> {
    let rest = path.strip_prefix('/')?;
    let segments: Vec<&str> = rest.split('/').collect();
    match segments.as_slice() {
        [""] => Some(Route {
            field: None,
            suffix: None,
        }),
        [suffix] if FORMATS.contains(suffix) => Some(Route {
            field: None,
            suffix: Some(suffix),
        }),
        [field] if FIELDS.contains(field) => Some(Route {
            field: Some(field),
            suffix: None,
        }),
        ["ip", "cidr"] => Some(Route {
            field: Some("cidr"),
            suffix: None,
        }),
        ["ip", suffix] if !FORMATS.contains(suffix) => None,
        ["asn", number] if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) => None,
        [field, suffix] if FIELDS.contains(field) && !suffix.is_empty() => Some(Route {
            field: Some(field),
            suffix: Some(suffix),
        }),
        _ => None,
    }
}

/// V1's negotiation, with a CLI agent that sends no `Accept` treated as one sending `*/*`.
fn negotiated(headers: &HeaderMap) -> NegotiatedFormat {
    let agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    if !headers.contains_key(header::ACCEPT) && is_cli_client(agent, Some("*/*")) {
        return NegotiatedFormat::Plain;
    }
    negotiate(None, headers)
}

/// The form named by a negotiation; `None` for HTML or nothing.
fn negotiated_form(headers: &HeaderMap) -> Option<Form> {
    let name = match negotiated(headers) {
        NegotiatedFormat::Plain => "plain",
        NegotiatedFormat::Json => "json",
        NegotiatedFormat::Yaml => "yaml",
        NegotiatedFormat::Toml => "toml",
        NegotiatedFormat::Csv => "csv",
        NegotiatedFormat::Html | NegotiatedFormat::Unknown => return None,
    };
    form_of(name).ok()
}

/// Answers GET and HEAD on the V1 paths of the `ip.` host; any other request is handed back
/// unchanged, as is a `/` that negotiates to HTML or to nothing.
// The spec fixes `Err(Request)`: the caller needs the request back unchanged, so it cannot be boxed.
#[allow(clippy::result_large_err)]
pub async fn ip_host(state: &ApiState, req: Request<Body>) -> Result<Response, Request<Body>> {
    let is_head = match *req.method() {
        Method::GET => false,
        Method::HEAD => true,
        _ => return Err(req),
    };
    let Some(route) = route(req.uri().path()) else {
        return Err(req);
    };
    let unknown_suffix = route.suffix.filter(|suffix| !FORMATS.contains(suffix));
    let is_root = route.field.is_none();
    let query = Query::<IpQuery>::try_from_uri(req.uri());
    let format = query.as_ref().ok().and_then(|q| q.0.format.as_deref());
    let form = match (route.suffix, format) {
        (Some(name), _) | (None, Some(name)) => form_of(name),
        (None, None) => match negotiated_form(req.headers()) {
            Some(form) => Ok(form),
            None if is_root => return Err(req),
            None => form_of("plain"),
        },
    };
    // A refusal here still goes through the metadata step, as the API's refusals do.
    let request = match (unknown_suffix, query, form) {
        (Some(suffix), _, _) => Err(V1Error::NotFound(format!(
            "unknown format `{suffix}`; valid formats: json, yaml, toml, csv"
        ))),
        (None, Ok(Query(mut query)), Ok(form)) => {
            query.field = match (route.field, &form) {
                (None, Form::Plain) => Some("ip".to_string()),
                (field, _) => field.map(str::to_string),
            };
            Ok((query, form))
        }
        (None, Err(e), _) => Err(V1Error::InvalidInput(e.body_text())),
        (None, _, Err(e)) => Err(e),
    };
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let headers = req.headers();
    let if_none_match = headers.get(header::IF_NONE_MATCH).cloned();
    let response = with_metadata(state, true, if_none_match, async {
        match request {
            Ok((query, form)) => answer_caller(state, peer, headers, &query, Some(form))
                .await
                .unwrap_or_else(IntoResponse::into_response),
            Err(refusal) => refusal.into_response(),
        }
    })
    .await;
    if !is_head {
        return Ok(response);
    }
    // HEAD keeps GET's length: a later layer would measure the empty body as 0.
    let (mut parts, body) = response.into_parts();
    if let Some(length) = axum::body::HttpBody::size_hint(&body).exact() {
        parts
            .headers
            .insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(Response::from_parts(parts, Body::empty()))
}
