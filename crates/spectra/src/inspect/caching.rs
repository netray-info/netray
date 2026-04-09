use reqwest::header::HeaderMap;

use super::assembler::{CacheControlDirectives, CachingReport, CdnReport};

/// CDN signature headers: (header_name, cdn_name).
const CDN_SIGNATURES: &[(&str, &str)] = &[
    ("cf-ray", "Cloudflare"),
    ("cf-cache-status", "Cloudflare"),
    ("x-cdn", "Generic CDN"),
    ("x-cache", "Generic CDN"),
    ("x-amz-cf-id", "Amazon CloudFront"),
    ("x-amz-cf-pop", "Amazon CloudFront"),
    ("x-served-by", "Fastly"),
    ("x-fastly-request-id", "Fastly"),
    ("x-akamai-transformed", "Akamai"),
    ("x-azure-ref", "Azure CDN"),
    ("x-msedge-ref", "Azure CDN"),
    ("x-vercel-id", "Vercel"),
    ("x-vercel-cache", "Vercel"),
    ("fly-request-id", "Fly.io"),
    ("x-nf-request-id", "Netlify"),
    ("x-bunnyCDN-version", "BunnyCDN"),
];

/// Deprecated security headers per SDD §18.
const DEPRECATED_HEADERS: &[&str] = &[
    "x-xss-protection",
    "expect-ct",
    "public-key-pins",
    "public-key-pins-report-only",
];

pub fn analyze_caching(headers: &HeaderMap) -> CachingReport {
    let cache_control = headers
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let directives = parse_cache_control(cache_control.as_deref().unwrap_or(""));

    let etag = headers.contains_key("etag");
    let last_modified = headers.contains_key("last-modified");

    let vary = headers
        .get("vary")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    let age = headers
        .get("age")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());

    CachingReport {
        cache_control,
        directives,
        etag,
        last_modified,
        vary,
        age,
    }
}

fn parse_cache_control(value: &str) -> CacheControlDirectives {
    let lower = value.to_lowercase();
    let parts: Vec<&str> = lower.split(',').map(|s| s.trim()).collect();

    let mut max_age = None;
    for part in &parts {
        if let Some(val) = part.strip_prefix("max-age=") {
            max_age = val.trim().parse().ok();
        }
    }

    CacheControlDirectives {
        public: parts.contains(&"public"),
        private: parts.contains(&"private"),
        max_age,
        no_store: parts.contains(&"no-store"),
        no_cache: parts.contains(&"no-cache"),
        must_revalidate: parts.contains(&"must-revalidate"),
        immutable: parts.contains(&"immutable"),
    }
}

pub fn detect_cdn(headers: &HeaderMap) -> CdnReport {
    let mut detected: Option<String> = None;
    let mut indicators = Vec::new();

    for &(header, cdn_name) in CDN_SIGNATURES {
        if headers.contains_key(header) {
            indicators.push(header.to_string());
            if detected.is_none() {
                detected = Some(cdn_name.to_string());
            }
        }
    }

    let cache_status = headers
        .get("x-cache")
        .or_else(|| headers.get("cf-cache-status"))
        .or_else(|| headers.get("x-vercel-cache"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    CdnReport {
        detected,
        cache_status,
        indicators,
    }
}

pub fn detect_deprecated(headers: &HeaderMap) -> Vec<String> {
    let mut found = Vec::new();
    for &name in DEPRECATED_HEADERS {
        if headers.contains_key(name) {
            // Use original casing for display
            let display_name = match name {
                "x-xss-protection" => "X-XSS-Protection",
                "expect-ct" => "Expect-CT",
                "public-key-pins" => "Public-Key-Pins",
                "public-key-pins-report-only" => "Public-Key-Pins-Report-Only",
                _ => name,
            };
            found.push(display_name.to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        h
    }

    #[test]
    fn parses_cache_control() {
        let h = headers_with(&[("cache-control", "public, max-age=3600, must-revalidate")]);
        let report = analyze_caching(&h);
        assert!(report.directives.public);
        assert_eq!(report.directives.max_age, Some(3600));
        assert!(report.directives.must_revalidate);
    }

    #[test]
    fn detects_etag_and_last_modified() {
        let h = headers_with(&[
            ("etag", "\"abc123\""),
            ("last-modified", "Mon, 01 Jan 2024 00:00:00 GMT"),
        ]);
        let report = analyze_caching(&h);
        assert!(report.etag);
        assert!(report.last_modified);
    }

    #[test]
    fn parses_vary() {
        let h = headers_with(&[("vary", "Accept-Encoding, Accept-Language")]);
        let report = analyze_caching(&h);
        assert_eq!(report.vary, vec!["Accept-Encoding", "Accept-Language"]);
    }

    #[test]
    fn detects_cloudflare() {
        let h = headers_with(&[("cf-ray", "abc123-IAD")]);
        let cdn = detect_cdn(&h);
        assert_eq!(cdn.detected.as_deref(), Some("Cloudflare"));
        assert!(cdn.indicators.contains(&"cf-ray".to_string()));
    }

    #[test]
    fn detects_cloudfront() {
        let h = headers_with(&[("x-amz-cf-id", "abc")]);
        let cdn = detect_cdn(&h);
        assert_eq!(cdn.detected.as_deref(), Some("Amazon CloudFront"));
    }

    #[test]
    fn no_cdn_detected() {
        let h = headers_with(&[("content-type", "text/html")]);
        let cdn = detect_cdn(&h);
        assert!(cdn.detected.is_none());
        assert!(cdn.indicators.is_empty());
    }

    #[test]
    fn detects_deprecated_xss_protection() {
        let h = headers_with(&[("x-xss-protection", "1; mode=block")]);
        let deprecated = detect_deprecated(&h);
        assert!(deprecated.contains(&"X-XSS-Protection".to_string()));
    }

    #[test]
    fn detects_deprecated_expect_ct() {
        let h = headers_with(&[("expect-ct", "max-age=0")]);
        let deprecated = detect_deprecated(&h);
        assert!(deprecated.contains(&"Expect-CT".to_string()));
    }

    #[test]
    fn no_deprecated_headers() {
        let h = headers_with(&[("content-type", "text/html")]);
        let deprecated = detect_deprecated(&h);
        assert!(deprecated.is_empty());
    }
}
