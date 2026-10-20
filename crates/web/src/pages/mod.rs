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
pub mod result;
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
    CachedDns, DnsResults, InspectConfig, InspectDeps, InspectLimits, InspectorJobs, Job, JobPart,
    JobRequest, Refusal, SharedRuns, Tool, email_input, inspect_router, registrable_domain,
};

/// The speculation rules every page embeds; the CSP allows exactly their hash.
pub const SPECULATION_RULES: &str = r#"{"prerender":[{"where":{"or":[{"href_matches":"/guide/*"},{"href_matches":"/r/*"},{"href_matches":"/inspect/*"}]},"eagerness":"moderate"}]}"#;

/// The one origin of the canonical link and of every absolute link a page offers to copy.
pub(crate) const ORIGIN: &str = "https://netray.info";

/// The guides, compare, status, the sitemap and the static files under `/static`, plus the 301s
/// of V1's site paths (`/compare/`, `/status/`, `/tools`; `/guide/<slug>.html` in `guides::show`).
pub fn router() -> Router {
    Router::new()
        .route("/guide", get(guides::index_redirect))
        .route("/guide/", get(guides::index))
        .route("/guide/{slug}", get(guides::show))
        .route("/guide/{slug}/", get(errors::not_found))
        .route("/compare", get(compare::show))
        .route("/compare/", get(|| async { guides::moved("/compare") }))
        .route("/status", get(status::show))
        .route("/status/", get(|| async { guides::moved("/status") }))
        .route("/tools", get(|| async { guides::moved("/") }))
        .route("/tools/", get(|| async { guides::moved("/") }))
        .route("/sitemap.xml", get(sitemap))
        .route("/static/", get(errors::not_found))
        .route("/static/{*path}", get(assets::serve))
        .layer(axum::middleware::map_response(
            |response: Response| async move { headers::secure(response) },
        ))
}

/// The body of `/robots.txt`: everything allowed, the sitemap at the origin.
pub fn robots_body() -> String {
    format!("User-agent: *\nAllow: /\nSitemap: {ORIGIN}/sitemap.xml\n")
}

/// `/robots.txt`.
pub async fn robots() -> Response {
    headers::secure(
        (
            [
                (CONTENT_TYPE, "text/plain; charset=utf-8"),
                (CACHE_CONTROL, "public, max-age=3600"),
            ],
            robots_body(),
        )
            .into_response(),
    )
}

/// `/sitemap.xml`: the pages this crate serves plus the entry pages and the inspector pages.
async fn sitemap() -> Response {
    let mut paths = vec!["/".to_string(), "/guide/".to_string()];
    paths.extend(guides::GUIDES.iter().map(|g| format!("/guide/{}", g.slug)));
    paths.extend(["/compare", "/status", "/privacy", "/tools", "/api/v1/docs"].map(str::to_string));
    paths.extend(Tool::ALL.map(|t| format!("/inspect/{}", t.as_str())));
    let locs = paths.into_iter().map(|p| format!("{ORIGIN}{p}"));
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
    pub(crate) fn origin(&self) -> &'static str {
        ORIGIN
    }

    pub(crate) fn new(title: &str, description: &str, canonical: &str) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
            canonical: canonical.to_string(),
            og_image: format!("{ORIGIN}/static/og/landing.png"),
            rules: SPECULATION_RULES,
        }
    }
}

#[derive(Template)]
#[template(path = "partials/inspect_export.html")]
struct ExportFragment {
    filename: String,
    exports: Option<(String, String, String)>,
}

/// The export slot named `filename`: disabled without `exports` (Markdown, CSV, JSON).
pub(crate) fn export_slot(
    filename: String,
    exports: Option<(String, String, String)>,
) -> Option<String> {
    let exports = exports.map(|(m, c, j)| (html_text(&m), html_text(&c), html_text(&j)));
    (ExportFragment { filename, exports }).render().ok()
}

/// `s` as HTML text, quotes as named references.
fn html_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// What the help dialog of the result page and the evidence views says.
pub(crate) struct PageHelp {
    pub title: &'static str,
    pub what: &'static str,
    /// The shortcut rows: the keys of a row and what they do on the page.
    pub keys: &'static [(&'static [&'static str], &'static str)],
}

impl PageHelp {
    pub(crate) const RESULT: PageHelp = PageHelp {
        title: "Result page",
        what: "The grade, the spectrum, the open issues and the evidence of the latest run of this domain. Copy or download the stored run from the Evidence section.",
        keys: &[
            (&["r"], "Check again"),
            (&["j", "k"], "Next or previous open issue"),
            (&["?"], "Open this help"),
            (&["Esc"], "Close this help"),
        ],
    };

    pub(crate) const EVIDENCE: PageHelp = PageHelp {
        title: "Evidence view",
        what: "The checks of one protocol in a stored run and the evidence they read. Copy or download this protocol's checks and evidence.",
        keys: &[
            (&["r"], "Check the domain again"),
            (&["j", "k"], "Next or previous check"),
            (&["?"], "Open this help"),
            (&["Esc"], "Close this help"),
        ],
    };
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
