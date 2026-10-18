//! The pages every service shares (spec v2-pages): the frame, the static files, the error pages.
//! `router()` holds only what is common; a service adds its own `/`, `/robots.txt` and fallback.

mod assets;
pub mod compare;
mod errors;
mod evidence;
pub mod guides;
mod headers;
mod inspect;
mod jsonld;
mod status;

use askama::Template;
use axum::Router;
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

pub use errors::{not_found, query_redirect, server_error};
pub use evidence::{EvidenceDeps, evidence_router};
pub use inspect::{
    InspectConfig, InspectDeps, InspectLimits, InspectorJobs, Job, JobPart, JobRequest, Tool,
    inspect_router, registrable_domain,
};

/// The speculation rules every page embeds; the CSP allows exactly their hash.
pub const SPECULATION_RULES: &str = r#"{"prerender":[{"where":{"or":[{"href_matches":"/guide/*"},{"href_matches":"/r/*"},{"href_matches":"/inspect/*"}]},"eagerness":"moderate"}]}"#;

const ROBOTS: &str = "User-agent: *\nAllow: /\nSitemap: https://netray.info/sitemap.xml\n";

/// The guides, compare, status, the sitemap and the static files under `/static`.
pub fn router() -> Router {
    Router::new()
        .route("/guide", get(guides::index_redirect))
        .route("/guide/", get(guides::index))
        .route("/guide/{slug}", get(guides::show))
        .route("/guide/{slug}/", get(errors::not_found))
        .route("/compare", get(compare::show))
        .route("/status", get(status::show))
        .route("/sitemap.xml", get(sitemap))
        .route("/static/", get(errors::not_found))
        .route("/static/{*path}", get(assets::serve))
        .layer(axum::middleware::map_response(
            |response: Response| async move { headers::secure(response) },
        ))
}

/// `/robots.txt`.
pub async fn robots() -> Response {
    headers::secure(
        (
            [
                (CONTENT_TYPE, "text/plain; charset=utf-8"),
                (CACHE_CONTROL, "public, max-age=3600"),
            ],
            ROBOTS,
        )
            .into_response(),
    )
}

/// `/sitemap.xml`: the pages this crate serves plus the entry pages and the inspector roots.
async fn sitemap() -> Response {
    let mut locs = vec![
        "https://netray.info/".to_string(),
        "https://netray.info/guide/".to_string(),
    ];
    locs.extend(
        guides::GUIDES
            .iter()
            .map(|g| format!("https://netray.info/guide/{}", g.slug)),
    );
    locs.extend(
        [
            "https://netray.info/compare",
            "https://netray.info/status",
            "https://netray.info/privacy",
            "https://ip.netray.info/",
            "https://dns.netray.info/",
            "https://tls.netray.info/",
            "https://http.netray.info/",
        ]
        .map(str::to_string),
    );
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for loc in locs {
        xml.push_str(&format!("<url><loc>{loc}</loc></url>\n"));
    }
    xml.push_str("</urlset>\n");
    (
        [
            (CONTENT_TYPE, "application/xml; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=3600"),
        ],
        xml,
    )
        .into_response()
}

/// What the frame's head and share tags carry.
pub(crate) struct Frame {
    pub title: String,
    pub description: String,
    pub canonical: String,
    pub og_image: String,
    pub rules: &'static str,
}

impl Frame {
    pub(crate) fn new(title: &str, description: &str, canonical: &str) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
            canonical: canonical.to_string(),
            og_image: "https://netray.info/static/og/landing.png".to_string(),
            rules: SPECULATION_RULES,
        }
    }
}

/// Renders a page template into a secured HTML response; a render error is the 500 page.
pub(crate) fn page<T: Template>(
    status: StatusCode,
    cache_control: &'static str,
    template: &T,
) -> Response {
    match template.render() {
        Ok(html) => headers::secure(
            (
                status,
                [
                    (CONTENT_TYPE, "text/html; charset=utf-8"),
                    (CACHE_CONTROL, cache_control),
                ],
                html,
            )
                .into_response(),
        ),
        Err(_) => server_error(),
    }
}
