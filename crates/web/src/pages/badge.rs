//! `GET /badge/{domain}.svg`: the grade badge of a domain. Copied from lens's `routes.rs`
//! (spec v2-cutover, Phase 6).

use std::net::IpAddr;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use netray_engine::domain_run::{OverallScore, grade_label};

use crate::admission::{Admit, ApexState, Trigger, admit, head, until_done};
use crate::badge::render::svg_for_grade;
use crate::badge::{
    BadgeQuery, BadgeRequest, compute_etag, is_unscored_grade, parse_badge_request,
};

/// The badge route on `state`. The path segment carries the `.svg` suffix (matchit has no
/// literal suffix after a parameter); any other suffix answers 404.
pub fn router(state: ApexState) -> Router {
    Router::new()
        .route("/badge/{domain}", get(badge_handler))
        .with_state(state)
}

fn badge_is_not_modified(headers: &axum::http::HeaderMap, etag: &str) -> bool {
    headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == etag)
        .unwrap_or(false)
}

fn build_svg_response(svg: String, cache_control: &str, etag: &str) -> Response {
    use axum::http::{StatusCode, header};
    let mut resp = (StatusCode::OK, svg).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        "image/svg+xml; charset=utf-8".parse().unwrap(),
    );
    headers.insert(header::CACHE_CONTROL, cache_control.parse().unwrap());
    headers.insert(header::ETAG, etag.parse().unwrap());
    resp
}

// ---------------------------------------------------------------------------
// Badge handler
// ---------------------------------------------------------------------------

/// The `?` badge a throttled request gets.
fn throttled_badge_response(
    badge_req: &BadgeRequest,
    req_headers: &axum::http::HeaderMap,
) -> Response {
    use std::time::Instant;
    metrics::counter!("netray_badge_requests_total", "cache" => "throttled").increment(1);
    let render_start = Instant::now();
    let svg = svg_for_grade(&badge_req.label, "error", badge_req.style);
    metrics::histogram!("netray_badge_render_duration_seconds")
        .record(render_start.elapsed().as_secs_f64());
    let etag = compute_etag(&badge_req.domain, "?", &badge_req.label, badge_req.style);
    if badge_is_not_modified(req_headers, &etag) {
        return axum::http::StatusCode::NOT_MODIFIED.into_response();
    }
    build_svg_response(svg, "public, max-age=300, s-maxage=300", &etag)
}

/// The badge for `grade`; `long` asks for the long `Cache-Control`, which an unscored grade
/// never gets.
fn badge_response(
    badge_req: &BadgeRequest,
    req_headers: &axum::http::HeaderMap,
    grade: &str,
    long: bool,
) -> Response {
    let render_start = std::time::Instant::now();
    let svg = svg_for_grade(&badge_req.label, grade, badge_req.style);
    metrics::histogram!("netray_badge_render_duration_seconds")
        .record(render_start.elapsed().as_secs_f64());
    let etag = compute_etag(&badge_req.domain, grade, &badge_req.label, badge_req.style);
    if badge_is_not_modified(req_headers, &etag) {
        return axum::http::StatusCode::NOT_MODIFIED.into_response();
    }
    let cache_ctrl = if long && !is_unscored_grade(grade) {
        "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400"
    } else {
        "public, max-age=300, s-maxage=300"
    };
    build_svg_response(svg, cache_ctrl, &etag)
}

pub async fn badge_handler(
    State(state): State<ApexState>,
    Path(domain): Path<String>,
    Query(query): Query<BadgeQuery>,
    req_headers: axum::http::HeaderMap,
) -> Response {
    let domain = match domain.strip_suffix(".svg") {
        Some(d) => d.to_owned(),
        None => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    let badge_req = match parse_badge_request(&domain, query, &state.config.badges) {
        Ok(r) => r,
        Err(e) => return e.into_response(),
    };

    let head = head(&state, &badge_req.domain).await;

    // The head is inside the cache window — serve it, bypass all rate limiters.
    if let Some(lookup) = &head
        && lookup.fresh
    {
        metrics::counter!("netray_badge_requests_total", "cache" => "hit").increment(1);
        return badge_response(
            &badge_req,
            &req_headers,
            grade_label(lookup.head.grade),
            true,
        );
    }
    let stale = head.as_ref().map(|l| grade_label(l.head.grade));

    // Start or join the domain's run.
    let published = match admit(
        &state,
        &badge_req.domain,
        IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        Trigger::Badge,
        netray_common::prefetch::is_prefetch(&req_headers),
    ) {
        Admit::Admitted(mut stream) => until_done(&mut stream).await,
        Admit::Refused(_) | Admit::NotStarted => {
            return match stale {
                Some(grade) => {
                    metrics::counter!("netray_badge_requests_total", "cache" => "throttled")
                        .increment(1);
                    badge_response(&badge_req, &req_headers, grade, false)
                }
                None => throttled_badge_response(&badge_req, &req_headers),
            };
        }
    };
    let Some(published) = published else {
        return match stale {
            Some(grade) => badge_response(&badge_req, &req_headers, grade, false),
            None => throttled_badge_response(&badge_req, &req_headers),
        };
    };

    if published.output.score.complete {
        metrics::counter!("netray_badge_requests_total", "cache" => "miss").increment(1);
        let grade = badge_grade(&published.output.score);
        return badge_response(&badge_req, &req_headers, &grade, true);
    }
    metrics::counter!("netray_badge_requests_total", "cache" => "failed").increment(1);
    badge_response(
        &badge_req,
        &req_headers,
        stale.unwrap_or("incomplete"),
        false,
    )
}

/// The grade a badge shows: `"incomplete"` for an incomplete result, whatever its grade.
fn badge_grade(score: &OverallScore) -> String {
    if score.complete {
        score.grade.clone()
    } else {
        "incomplete".to_string()
    }
}
