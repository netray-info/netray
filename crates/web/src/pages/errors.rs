//! The 404 and 500 pages and the `?d=` redirect.

use std::collections::HashMap;

use askama::Template;
use axum::extract::Query;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, LOCATION};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::headers::secure;
use super::{Frame, page};
use crate::components::QueryInputView;

/// A selector list in a `Location` query: commas stay readable, everything else that is not
/// alphanumeric or a hyphen is encoded.
const SELECTORS_SET: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b',');

pub(super) const LOCATION_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'.')
    .remove(b'-')
    .remove(b'_')
    .remove(b'~');

#[derive(Template)]
#[template(path = "pages/not_found.html")]
struct NotFoundPage {
    frame: Frame,
    query: QueryInputView,
    invalid: bool,
    reason: Option<String>,
}

#[derive(Template)]
#[template(path = "pages/server_error.html")]
struct ServerErrorPage {
    frame: Frame,
}

pub(super) fn query_view(value: Option<String>) -> QueryInputView {
    QueryInputView {
        action: "/".to_string(),
        name: "d".to_string(),
        label: "Domain".to_string(),
        placeholder: "example.com".to_string(),
        value,
        submit: "Check".to_string(),
    }
}

/// What the 404 page answers: no page, a refused domain name or a refused selector list.
#[derive(PartialEq)]
enum Refused {
    Nothing,
    Domain,
    Selectors,
}

fn not_found_page(
    status: StatusCode,
    value: Option<String>,
    refused: Refused,
    reason: Option<String>,
) -> Response {
    let (title, description) = match refused {
        Refused::Domain => (
            "Not a domain name - netray",
            "That is not a domain name. Enter one, e.g. example.com.",
        ),
        Refused::Selectors => (
            "Not a list of DKIM selectors - netray",
            "That is not a list of DKIM selectors.",
        ),
        Refused::Nothing => (
            "Page not found - netray",
            "There is no page here. Enter a domain name to check it.",
        ),
    };
    page(
        status,
        "no-store",
        &NotFoundPage {
            frame: Frame::new(title, description, "/"),
            query: query_view(value),
            invalid: refused == Refused::Domain,
            reason,
        },
    )
}

/// The 400 page of a refused `dkim_selectors` list on `domain`'s page: the domain refilled.
pub(super) fn selectors_refused_page(domain: &str, detail: &str) -> Response {
    not_found_page(
        StatusCode::BAD_REQUEST,
        Some(domain.to_string()),
        Refused::Selectors,
        Some(format!(
            "A DKIM selector list is 1 to 5 names of letters, digits and hyphens, separated by commas. This one was refused: {detail}."
        )),
    )
}

/// The 400 page of a refused domain segment: the input refilled, the reason stated.
pub(super) fn refused_page(input: &str, reason: &str) -> Response {
    not_found_page(
        StatusCode::BAD_REQUEST,
        Some(input.to_string()),
        Refused::Domain,
        Some(reason.to_string()),
    )
}

/// The 404 page.
pub async fn not_found() -> Response {
    not_found_page(StatusCode::NOT_FOUND, None, Refused::Nothing, None)
}

/// The 500 page; a plain-text body when even that does not render.
pub fn server_error() -> Response {
    let frame = Frame::new(
        "Something went wrong - netray",
        "The page failed to render. The status page says whether netray is down.",
        "/",
    );
    match (ServerErrorPage { frame }).render() {
        Ok(html) => secure(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [
                    (CONTENT_TYPE, "text/html; charset=utf-8"),
                    (CACHE_CONTROL, "no-store"),
                ],
                html,
            )
                .into_response(),
        ),
        Err(_) => secure(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [
                    (CONTENT_TYPE, "text/plain; charset=utf-8"),
                    (CACHE_CONTROL, "no-store"),
                ],
                "Internal Server Error",
            )
                .into_response(),
        ),
    }
}

fn normalise(raw: &str) -> Option<String> {
    let lower = raw.trim().to_lowercase();
    let domain = lower.strip_suffix('.').unwrap_or(&lower);
    let valid = domain.contains('.')
        && !domain.contains(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#'));
    valid.then(|| domain.to_string())
}

/// `GET /?d=<domain>`: a 301 to `/<domain>`, the 404 page without `d`, a 400 for a bad value.
pub async fn query_redirect(uri: Uri) -> Response {
    let Ok(Query(params)) = Query::<HashMap<String, String>>::try_from_uri(&uri) else {
        return not_found().await;
    };
    let Some(raw) = params.get("d") else {
        return not_found().await;
    };
    match normalise(raw) {
        Some(domain) => {
            let mut location = format!("/{}", utf8_percent_encode(&domain, LOCATION_SET));
            // lens's `?d=` with `dkim_selectors` keeps them for the run page (spec
            // v2-dkim-legacy-urls requirement 7); the page validates them.
            if let Some(selectors) = params
                .get("dkim_selectors")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                location.push_str("?dkim_selectors=");
                location.extend(utf8_percent_encode(selectors, SELECTORS_SET));
            }
            secure((StatusCode::MOVED_PERMANENTLY, [(LOCATION, location)]).into_response())
        }
        None => not_found_page(
            StatusCode::BAD_REQUEST,
            Some(raw.clone()),
            Refused::Domain,
            None,
        ),
    }
}
