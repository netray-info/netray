use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_engine::history::score_run;
use netray_engine::profile::Profiles;
use netray_store::Head;
use xxhash_rust::xxh3::xxh3_64;

use crate::admission::{Admit, Trigger, admit, until_done};
use crate::error::AppError;
use crate::input::validate_domain;
use crate::og::layout::{DEFAULT_LABEL, MAX_LABEL_LEN};
use crate::og::render::{RenderError, svg_for_entry, svg_for_grade, svg_to_png};
use crate::score::grade_label;
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct OgQueryParams {
    pub label: Option<String>,
}

fn compute_etag(domain: &str, grade: &str, label: &str, score_pct: f64) -> String {
    let input = format!("{domain}\x00{grade}\x00{label}\x00{score_pct:.1}");
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

/// Renders an entry's card in the V1 layout, without a score.
pub(crate) fn entry_card_renderer(state: &AppState) -> netray_web::entries::CardRenderer {
    let font_db = state.font_db.clone();
    std::sync::Arc::new(move |card: &netray_web::entries::EntryCard| {
        let svg = svg_for_entry(&card.domain, &card.grade, DEFAULT_LABEL, card.created_at);
        svg_to_png(&svg, font_db.clone()).map_err(|e| Box::new(render_error_from(e)))
    })
}

pub async fn og_handler(
    State(state): State<AppState>,
    Path(domain_png): Path<String>,
    Query(params): Query<OgQueryParams>,
    req_headers: axum::http::HeaderMap,
) -> Response {
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

    let head = crate::cache::head(&state, &domain).await;
    let head_card = match &head {
        Some(lookup) => head_card(&state, &lookup.head).await,
        None => None,
    };

    // The head is inside the cache window — serve it immediately.
    if let Some(lookup) = &head
        && lookup.fresh
        && let Some(card) = &head_card
    {
        return og_response(&state, &req_headers, &domain, &label, card, true);
    }

    // Start or join the domain's run.
    let published = match admit(
        &state,
        &domain,
        std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        Trigger::Og,
        netray_common::prefetch::is_prefetch(&req_headers),
    ) {
        Admit::Admitted(mut stream) => until_done(&mut stream).await,
        Admit::Refused(_) | Admit::NotStarted => {
            metrics::counter!("lens_badge_requests_total", "cache" => "throttled").increment(1);
            let unscored = Card::new("?", 0.0, SystemTime::now());
            let card = head_card.as_ref().unwrap_or(&unscored);
            return og_response(&state, &req_headers, &domain, &label, card, false);
        }
    };
    let Some(published) = published else {
        return AppError::Internal("check run ended without a result".to_string()).into_response();
    };
    let score = &published.output.score;
    let run_card = Card::new(&score.grade, score.overall_percentage, SystemTime::now());
    if score.complete {
        return og_response(&state, &req_headers, &domain, &label, &run_card, true);
    }
    let card = head_card.as_ref().unwrap_or(&run_card);
    og_response(&state, &req_headers, &domain, &label, card, false)
}

/// What a card shows.
struct Card {
    grade: String,
    score_pct: f64,
    checked_at: SystemTime,
}

impl Card {
    fn new(grade: &str, score_pct: f64, checked_at: SystemTime) -> Self {
        Self {
            grade: grade.to_string(),
            score_pct,
            checked_at,
        }
    }
}

/// The card of the head entry: its grade, the percentage of its stored run under the profile
/// it was scored with, and when it was last seen. `None` when the entry is gone.
async fn head_card(state: &AppState, head: &Head) -> Option<Card> {
    let store = state.history_store.as_ref()?;
    let entry = match store.get(&head.id).await {
        Ok(entry) => entry?,
        Err(e) => {
            tracing::warn!(error = %e, id = %head.id, "history entry lookup failed");
            return None;
        }
    };
    let configured = &*state.scoring_profile;
    let profile = if entry.profile_version == configured.version {
        configured
    } else {
        // An unknown version falls back to the configured profile.
        Profiles::get(entry.profile_version).unwrap_or(configured)
    };
    let seen = u64::try_from(head.last_seen_at).unwrap_or(0);
    Some(Card::new(
        grade_label(head.grade),
        score_run(&entry.run, profile).overall_percentage,
        UNIX_EPOCH + Duration::from_secs(seen),
    ))
}

/// The card PNG; `long` asks for the long `Cache-Control`, which an unscored grade never gets.
fn og_response(
    state: &AppState,
    req_headers: &axum::http::HeaderMap,
    domain: &str,
    label: &str,
    card: &Card,
    long: bool,
) -> Response {
    let etag = compute_etag(domain, &card.grade, label, card.score_pct);
    if is_not_modified(req_headers, &etag) {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let svg = svg_for_grade(domain, &card.grade, label, card.score_pct, card.checked_at);
    let png = match svg_to_png(&svg, state.font_db.clone()) {
        Ok(b) => b,
        Err(e) => return render_error_from(e),
    };
    let cache_ctrl = if long && !crate::routes::is_unscored_grade(&card.grade) {
        "public, max-age=3600, s-maxage=3600, stale-while-revalidate=86400"
    } else {
        "public, max-age=300, s-maxage=300"
    };
    build_png_response(png, cache_ctrl, &etag)
}
