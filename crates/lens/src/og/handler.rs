use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use xxhash_rust::xxh3::xxh3_64;

use crate::cache::{CachedResult, cache_key, is_fresh};
use crate::check::run_check;
use crate::error::AppError;
use crate::input::validate_domain;
use crate::og::layout::{DEFAULT_LABEL, MAX_LABEL_LEN, OG_TTL_SECONDS};
use crate::og::render::{RenderError, svg_for_grade, svg_to_png};
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct OgQueryParams {
    pub label: Option<String>,
}

fn compute_etag(domain: &str, grade: &str, label: &str) -> String {
    let input = format!("{domain}\x00{grade}\x00{label}");
    let hash = xxh3_64(input.as_bytes());
    format!("\"{hash:016x}\"")
}

fn is_not_modified(headers: &axum::http::HeaderMap, etag: &str) -> bool {
    headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == etag)
        .unwrap_or(false)
}

fn build_png_response(png: Vec<u8>, cache_control: &str, etag: &str) -> Response {
    use axum::http::header;
    let mut resp = (StatusCode::OK, png).into_response();
    let headers = resp.headers_mut();
    headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, cache_control.parse().unwrap());
    headers.insert(header::ETAG, etag.parse().unwrap());
    resp
}

fn render_error_from(e: RenderError) -> Response {
    tracing::error!(error = %e, "OG card render failed");
    AppError::RenderFailed(e.to_string()).into_response()
}

async fn invoke_check(state: &AppState, domain: &str) -> crate::check::CheckOutput {
    if let Some(ref f) = state.badge_check_fn {
        f(domain.to_owned()).await
    } else {
        run_check(state, domain).await
    }
}

pub async fn og_handler(
    State(state): State<AppState>,
    Path(domain_png): Path<String>,
    Query(params): Query<OgQueryParams>,
    req_headers: axum::http::HeaderMap,
) -> Response {
    use netray_common::rate_limit::check_keyed_cost;
    use std::num::NonZeroU32;
    use std::time::SystemTime;

    // Strip .png suffix; return 404 for anything else.
    let domain_raw = match domain_png.strip_suffix(".png") {
        Some(d) => d,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // Validate domain.
    let domain = match validate_domain(domain_raw) {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };

    // Validate label.
    let label_raw = params.label.unwrap_or_else(|| DEFAULT_LABEL.to_owned());
    if !label_raw.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return AppError::InvalidLabel(
            "label must be ASCII printable (0x20\u{2013}0x7E), max 32 chars".to_string(),
        )
        .into_response();
    }
    if label_raw.len() > MAX_LABEL_LEN {
        return AppError::InvalidLabel(
            "label must be ASCII printable (0x20\u{2013}0x7E), max 32 chars".to_string(),
        )
        .into_response();
    }
    let label = label_raw;

    let key = cache_key(&domain);

    // Cache hit — serve immediately.
    if let Some(cache) = &state.cache
        && let Some(cached) = cache.get(&key).await
        && is_fresh(&cached, OG_TTL_SECONDS)
    {
        let grade = cached.score.grade.clone();
        let etag = compute_etag(&domain, &grade, &label);
        if is_not_modified(&req_headers, &etag) {
            return StatusCode::NOT_MODIFIED.into_response();
        }
        let svg = svg_for_grade(&domain, &grade, &label);
        let png = match svg_to_png(&svg, state.font_db.clone()) {
            Ok(b) => b,
            Err(e) => return render_error_from(e),
        };
        let is_error = grade == "error";
        let cache_ctrl = if is_error {
            "public, max-age=300, s-maxage=300"
        } else {
            "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400"
        };
        return build_png_response(png, cache_ctrl, &etag);
    }

    // Per-domain recompute limiter.
    let cost = NonZeroU32::new(1).unwrap();
    if check_keyed_cost(&state.badge_recompute_limiter, &key, cost, "og", "lens").is_err() {
        let etag = compute_etag(&domain, "?", &label);
        if is_not_modified(&req_headers, &etag) {
            return StatusCode::NOT_MODIFIED.into_response();
        }
        let svg = svg_for_grade(&domain, "?", &label);
        let png = match svg_to_png(&svg, state.font_db.clone()) {
            Ok(b) => b,
            Err(e) => return render_error_from(e),
        };
        return build_png_response(png, "public, max-age=300, s-maxage=300", &etag);
    }

    // Coalesced recompute.
    let grade = if let Some(cache) = &state.cache {
        let state_for_init = state.clone();
        let domain_for_init = domain.clone();
        let entry = cache
            .entry(key.clone())
            .or_insert_with_if(
                async move {
                    use std::sync::Arc;
                    let output = invoke_check(&state_for_init, &domain_for_init).await;
                    Arc::new(CachedResult {
                        sections: output.sections,
                        score: output.score,
                        duration_ms: output.duration_ms,
                        cached_at: SystemTime::now(),
                    })
                },
                |existing| !is_fresh(existing, OG_TTL_SECONDS),
            )
            .await;
        entry.into_value().score.grade.clone()
    } else {
        let output = invoke_check(&state, &domain).await;
        output.score.grade
    };

    let is_error = grade == "error";
    let etag = compute_etag(&domain, &grade, &label);
    if is_not_modified(&req_headers, &etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }

    let svg = svg_for_grade(&domain, &grade, &label);
    let png = match svg_to_png(&svg, state.font_db.clone()) {
        Ok(b) => b,
        Err(e) => return render_error_from(e),
    };

    let cache_ctrl = if is_error {
        "public, max-age=300, s-maxage=300"
    } else {
        "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400"
    };
    build_png_response(png, cache_ctrl, &etag)
}
