//! Whether a request is a browser prefetch or prerender, read from `Sec-Purpose`.

use axum::http::HeaderMap;

/// True when a `Sec-Purpose` value holds the token `prefetch` or `prerender`; tokens are
/// separated by `;` or `,` and compared ASCII case-insensitively.
pub fn is_prefetch(headers: &HeaderMap) -> bool {
    headers
        .get_all("sec-purpose")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split([';', ',']))
        .map(str::trim)
        .any(|t| t.eq_ignore_ascii_case("prefetch") || t.eq_ignore_ascii_case("prerender"))
}
