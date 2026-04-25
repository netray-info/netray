use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use crate::config::SiteConfig;
use crate::state::AppState;

/// Embedded Vite build output (`frontend/dist`). Populated by `make frontend`
/// or the Docker build; empty in test builds (only `.gitkeep` is checked in).
#[derive(RustEmbed)]
#[folder = "frontend/dist"]
pub struct Assets;

/// Substitute the four `[site]` placeholders into the embedded HTML shell.
///
/// Placeholders: `{{site_title}}`, `{{site_description}}`, `{{site_og_image}}`,
/// `{{site_og_site_name}}`. All values are HTML-escaped before substitution
/// (SDD product-repositioning §3 Requirement 23 — config-driven XSS
/// prevention). When `site.og_image` is `None` or empty, the entire
/// `og:image` meta tag line is stripped rather than left as `content=""`.
pub fn render_apex_html(template: &str, site: &SiteConfig) -> String {
    let og_image = site.og_image.as_deref().filter(|s| !s.is_empty());

    let mut html = if og_image.is_some() {
        template.to_string()
    } else {
        strip_lines_containing(template, "{{site_og_image}}")
    };

    html = html.replace(
        "{{site_title}}",
        &html_escape(site.title.as_deref().unwrap_or("")),
    );
    html = html.replace(
        "{{site_description}}",
        &html_escape(site.description.as_deref().unwrap_or("")),
    );
    html = html.replace(
        "{{site_og_site_name}}",
        &html_escape(site.og_site_name.as_deref().unwrap_or("")),
    );
    if let Some(url) = og_image {
        html = html.replace("{{site_og_image}}", &html_escape(url));
    }

    html
}

fn strip_lines_containing(s: &str, needle: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.split_inclusive('\n') {
        if !line.contains(needle) {
            out.push_str(line);
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// SPA static-file handler.
///
/// - Asset paths (`/favicon.svg`, hashed `/assets/*` from Vite, etc.) →
///   serve embedded bytes with year-long immutable cache.
/// - `/`, `/index.html`, unknown paths (SPA client-side routes) →
///   serve the templated HTML shell from `state.rendered_html`, `no-cache`.
/// - Empty embed (test build, no Vite output) → `500 frontend not found`.
pub async fn handler(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty()
        && path != "index.html"
        && let Some(file) = Assets::get(path)
    {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_string()),
                (
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_string(),
                ),
            ],
            file.data.to_vec(),
        )
            .into_response();
    }

    serve_index(&state)
}

fn serve_index(state: &AppState) -> Response {
    if let Some(html) = state.rendered_html.as_deref() {
        return (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ],
            html.as_bytes().to_vec(),
        )
            .into_response();
    }
    (StatusCode::INTERNAL_SERVER_ERROR, "frontend not found").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "\
<!DOCTYPE html>
<html lang=\"en\">
<head>
  <title>{{site_title}}</title>
  <meta name=\"description\" content=\"{{site_description}}\" />
  <meta property=\"og:site_name\" content=\"{{site_og_site_name}}\" />
  <meta property=\"og:image\" content=\"{{site_og_image}}\" />
</head>
<body><div id=\"root\"></div></body>
</html>
";

    #[test]
    fn renders_default_site_config() {
        let site = SiteConfig::default();
        let out = render_apex_html(TEMPLATE, &site);
        assert!(out.contains("netray.info — your domain"));
        assert!(out.contains("og:site_name\" content=\"netray.info\""));
        // og_image default is None → tag must be stripped entirely.
        assert!(!out.contains("og:image"));
        // No unsubstituted placeholders left behind.
        assert!(!out.contains("{{site_"));
    }

    #[test]
    fn renders_with_og_image_set() {
        let site = SiteConfig {
            og_image: Some("https://example.com/card.png".into()),
            ..SiteConfig::default()
        };
        let out = render_apex_html(TEMPLATE, &site);
        assert!(out.contains("og:image\" content=\"https://example.com/card.png\""));
    }

    #[test]
    fn empty_og_image_is_treated_as_none() {
        let site = SiteConfig {
            og_image: Some(String::new()),
            ..SiteConfig::default()
        };
        let out = render_apex_html(TEMPLATE, &site);
        assert!(!out.contains("og:image"));
    }

    #[test]
    fn html_escapes_user_supplied_values() {
        // SDD Requirement 23: config-driven XSS prevention.
        let site = SiteConfig {
            title: Some("<script>alert(1)</script>".into()),
            description: Some("\"onload=alert(1) x=\"".into()),
            ..SiteConfig::default()
        };
        let out = render_apex_html(TEMPLATE, &site);
        assert!(out.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!out.contains("<script>alert(1)</script>"));
        assert!(out.contains("&quot;onload=alert(1) x=&quot;"));
    }

    #[test]
    fn ampersand_is_escaped_to_avoid_entity_breakage() {
        let site = SiteConfig {
            title: Some("Acme & Co".into()),
            ..SiteConfig::default()
        };
        let out = render_apex_html(TEMPLATE, &site);
        assert!(out.contains("Acme &amp; Co"));
    }

    #[test]
    fn missing_optional_fields_substitute_empty_string() {
        let site = SiteConfig {
            title: None,
            description: None,
            og_site_name: None,
            ..SiteConfig::default()
        };
        let out = render_apex_html(TEMPLATE, &site);
        assert!(!out.contains("{{site_"));
        assert!(out.contains("<title></title>"));
    }
}
