//! The files under `/static`: the stylesheet, the islands bundle, the marks and the share images.

use axum::extract::Path;
use axum::extract::rejection::PathRejection;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

use super::errors;
use super::headers::secure;

const APP_CSS: &str = concat!(
    include_str!(concat!(env!("OUT_DIR"), "/tokens.css")),
    include_str!("../../static/base.css"),
    include_str!("../../static/grade.css"),
    include_str!("../../static/run_line.css"),
    include_str!("../../static/progress_ray.css"),
    include_str!("../../static/spectrum.css"),
    include_str!("../../static/trace.css"),
    include_str!("../../static/check_row.css"),
    include_str!("../../static/evidence_block.css"),
    include_str!("../../static/diff_block.css"),
    include_str!("../../static/grouping_switch.css"),
    include_str!("../../static/query_input.css"),
    include_str!("../../static/pages.css"),
);

const ISLANDS_JS: &[u8] = include_bytes!("../../islands/dist/islands.js");
const LOGO: &[u8] = include_bytes!("../../static/logo.svg");
const FAVICON: &[u8] = include_bytes!("../../static/favicon.svg");

const OG: [(&str, &[u8]); 1] = [("landing", include_bytes!("../../static/og/landing.png"))];

fn file(content_type: &'static str, body: &'static [u8]) -> Response {
    secure(
        (
            [
                (CONTENT_TYPE, content_type),
                (CACHE_CONTROL, "public, max-age=3600"),
            ],
            body,
        )
            .into_response(),
    )
}

pub(super) async fn serve(path: Result<Path<String>, PathRejection>) -> Response {
    let Ok(Path(path)) = path else {
        return errors::not_found().await;
    };
    match path.as_str() {
        "app.css" => file("text/css; charset=utf-8", APP_CSS.as_bytes()),
        "islands.js" => file("text/javascript; charset=utf-8", ISLANDS_JS),
        "logo.svg" => file("image/svg+xml", LOGO),
        "favicon.svg" => file("image/svg+xml", FAVICON),
        other => {
            let og = other
                .strip_prefix("og/")
                .and_then(|name| name.strip_suffix(".png"))
                .and_then(|name| OG.iter().find(|(known, _)| *known == name));
            match og {
                Some((_, bytes)) => file("image/png", bytes),
                None => errors::not_found().await,
            }
        }
    }
}
