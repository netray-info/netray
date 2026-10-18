//! The security headers every page response carries.

use std::sync::LazyLock;

use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::{HeaderName, HeaderValue};
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use super::SPECULATION_RULES;

static CSP: LazyLock<HeaderValue> = LazyLock::new(|| {
    let hash = STANDARD.encode(Sha256::digest(SPECULATION_RULES.as_bytes()));
    let policy = format!(
        "default-src 'self'; script-src 'self' 'sha256-{hash}'; style-src 'self' 'unsafe-inline'; \
         font-src 'self'; img-src 'self' data:; connect-src 'self' https://stats.uptimerobot.com; \
         frame-ancestors 'none'; base-uri 'self'; object-src 'none'; \
         form-action 'self' https://*.netray.info"
    );
    HeaderValue::from_str(&policy).expect("the policy is a header value")
});

const FIXED: [(HeaderName, &str); 7] = [
    (
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    ),
    (
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    ),
    (
        STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains; preload",
    ),
    (X_FRAME_OPTIONS, "DENY"),
    (X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (REFERRER_POLICY, "strict-origin-when-cross-origin"),
    (
        HeaderName::from_static("permissions-policy"),
        "camera=(), microphone=(), geolocation=(), payment=()",
    ),
];

/// Adds the policy and the seven fixed headers to a response.
pub(super) fn secure(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(CONTENT_SECURITY_POLICY, CSP.clone());
    for (name, value) in FIXED {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}
