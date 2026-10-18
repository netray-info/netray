//! The TLS and HTTP inspections of v1: `POST /tls/inspect` and `POST /http/inspect`, over the
//! `netray_tls::TlsInspector` and `netray_http::HttpInspect` parts. Each answers 503 first when its
//! part is off, then reads the body, then runs the inspection, which charges the `[api.tls]` or
//! `[api.http]` limiters held by `ApiState`.
use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use netray_common::error::ErrorResponse;
use netray_http::HttpInspectRequest;
use netray_tls::TlsInspectRequest;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::ip::{Peer, json_body, peer_of};
use super::limit;
use super::state::ApiState;

pub(super) const TLS_OFF: &str = "the TLS inspector is off: no TLS module was built";
pub(super) const HTTP_OFF: &str = "the HTTP inspector is off: no HTTP module was built";

pub(super) fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// The certificate chain, validation and quality of a host on one port.
#[utoipa::path(
    post,
    path = "/tls/inspect",
    tag = "tls",
    request_body = TlsInspectRequest,
    responses(
        (status = 200, description = "The inspection of the host on the port", body = netray_tls::routes::InspectResponse),
        (status = 400, description = "Malformed body, unknown field, invalid host or port", body = ErrorResponse),
        (status = 403, description = "The target is not a public address (`TARGET_BLOCKED`)", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The TLS module is off, the host did not resolve, or the handshake timed out", body = ErrorResponse),
    )
)]
pub(super) async fn tls_inspect(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<TlsInspectRequest>, JsonRejection>,
) -> Result<Json<netray_tls::routes::InspectResponse>, V1Error> {
    let inspector = state
        .tls_inspector()
        .ok_or_else(|| V1Error::Unavailable(TLS_OFF.into()))?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.tls_limiters().prune_if_due();
    let response = inspector
        .inspect(&request, client, state.tls_limiters(), request_id(&headers))
        .await?;
    Ok(Json(response))
}

/// The security headers, redirects and content policy of a URL.
#[utoipa::path(
    post,
    path = "/http/inspect",
    tag = "http",
    request_body = HttpInspectRequest,
    responses(
        (status = 200, description = "The inspection of the URL", body = netray_http::inspect::assembler::InspectResponse),
        (status = 400, description = "Malformed body, unknown field, invalid URL", body = ErrorResponse),
        (status = 403, description = "The target is not a public address (`TARGET_BLOCKED`)", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 503, description = "The HTTP module is off, the host did not resolve, or the request timed out", body = ErrorResponse),
    )
)]
pub(super) async fn http_inspect(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<HttpInspectRequest>, JsonRejection>,
) -> Result<Json<netray_http::inspect::assembler::InspectResponse>, V1Error> {
    let inspect = state
        .http_inspect()
        .ok_or_else(|| V1Error::Unavailable(HTTP_OFF.into()))?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.http_limiters().prune_if_due();
    let response =
        netray_http::job::inspect(&request, client, state.http_limiters(), inspect.as_ref())
            .await?;
    Ok(Json(response))
}

pub(super) fn tls_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new().routes(routes!(tls_inspect))
}

pub(super) fn http_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new().routes(routes!(http_inspect))
}

/// spectra's two header maps are `value_type = Object` in its own document; v1 types them.
pub(super) fn typed_http_maps(doc: utoipa::openapi::OpenApi) -> utoipa::openapi::OpenApi {
    let mut value = serde_json::to_value(&doc).expect("the document serialises");
    for (pointer, typed) in [
        (
            "/components/schemas/InspectResponse/properties/headers",
            serde_json::json!({"type": "object", "additionalProperties": {"type": "string"}}),
        ),
        (
            "/components/schemas/CspReport/properties/directives",
            serde_json::json!({
                "type": "object",
                "additionalProperties": {"type": "array", "items": {"type": "string"}}
            }),
        ),
    ] {
        *value
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("the HTTP document has no {pointer}")) = typed;
    }
    serde_json::from_value(value).expect("the typed document is still a document")
}
