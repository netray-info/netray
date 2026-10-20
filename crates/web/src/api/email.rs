//! The email inspection of v1: `POST /email/inspect`, over the `netray_email::EmailInspect` part.
//! It answers 503 first when the part is off, then reads the body, then starts the inspection,
//! which charges the `[api.email]` limiter and slots held by `ApiState`. Nothing is stored.
use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::HeaderMap;
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use futures::StreamExt;
use netray_common::error::ErrorResponse;
use netray_email::EmailInspectRequest;
use netray_email::EmailInspectResult;
use netray_email::quality::types::SseEvent;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::error::V1Error;
use super::ip::{Peer, json_body, peer_of};
use super::limit;
use super::runs::wants_event_stream;
use super::state::ApiState;

pub(super) const EMAIL_OFF: &str = "the email inspector is off: no email module was built";

/// The mail posture of a domain: twelve categories, the evidence and a summary. With
/// `Accept: text/event-stream` the same events arrive as they complete.
#[utoipa::path(
    post,
    path = "/email/inspect",
    tag = "email",
    request_body = EmailInspectRequest,
    responses(
        (status = 200, description = "The inspection of the domain", content(
            (EmailInspectResult = "application/json"),
            (SseEvent = "text/event-stream"),
        )),
        (status = 400, description = "Malformed body, unknown field, invalid domain", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The inspection ended without a summary", body = ErrorResponse),
        (status = 503, description = "The email module is off, or every inspection slot is taken", body = ErrorResponse),
    )
)]
pub(super) async fn email_inspect(
    State(state): State<ApiState>,
    peer: Peer,
    headers: HeaderMap,
    body: Result<Json<EmailInspectRequest>, JsonRejection>,
) -> Result<Response, V1Error> {
    let inspect = state
        .email_inspect()
        .ok_or_else(|| V1Error::Unavailable(EMAIL_OFF.into()))?;
    let request = json_body(body)?;
    let client = limit::client(&state, &headers, peer_of(peer));
    state.email_limiter().prune_if_due();
    let job = inspect.inspect(&request, client, state.email_limiter(), state.email_slots())?;
    if wants_event_stream(&headers) {
        return Ok(
            Sse::new(job.map(|event| Ok::<_, Infallible>(Event::from(event)))).into_response(),
        );
    }
    let result = job
        .collect()
        .await
        .ok_or_else(|| V1Error::Internal("the email inspection ended without a summary".into()))?;
    Ok(Json(result).into_response())
}

pub(super) fn email_doc_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new().routes(routes!(email_inspect))
}
