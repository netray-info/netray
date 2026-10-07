pub mod palette;
pub mod render;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::config::BadgesConfig;
use crate::error::AppError;
use crate::input::validate_domain;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeStyle {
    #[default]
    Flat,
    ForTheBadge,
}

#[derive(Debug, serde::Deserialize)]
pub struct BadgeQuery {
    pub style: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BadgeRequest {
    pub domain: String,
    pub style: BadgeStyle,
    pub label: String,
}

pub fn parse_badge_request(
    domain_raw: &str,
    query: BadgeQuery,
    config: &BadgesConfig,
) -> Result<BadgeRequest, AppError> {
    let domain = validate_domain(domain_raw)?;

    let style = match query.style.as_deref() {
        Some("for-the-badge") => BadgeStyle::ForTheBadge,
        _ => BadgeStyle::Flat,
    };

    let label_raw = query.label.unwrap_or_else(|| config.default_label.clone());

    if !label_raw.bytes().all(|b| (0x20..=0x7E).contains(&b)) {
        return Err(AppError::InvalidLabel(
            "label must be ASCII printable (0x20–0x7E), max 32 chars".to_string(),
        ));
    }

    let label = if label_raw.len() > config.max_label_len {
        label_raw[..config.max_label_len].to_string()
    } else {
        label_raw
    };

    Ok(BadgeRequest {
        domain,
        style,
        label,
    })
}

/// Derive an ETag for a badge response.
///
/// Uses `DefaultHasher` which is not stable across process restarts. This is
/// acceptable because ETags are cache hints, not persistent identifiers. CDN
/// revalidation on process restart is correct behavior.
pub fn compute_etag(domain: &str, grade: &str, label: &str, style: BadgeStyle) -> String {
    let mut h = DefaultHasher::new();
    domain.hash(&mut h);
    grade.hash(&mut h);
    label.hash(&mut h);
    (style == BadgeStyle::ForTheBadge).hash(&mut h);
    format!("\"{:016x}\"", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BadgesConfig;

    fn default_cfg() -> BadgesConfig {
        BadgesConfig::default()
    }

    #[test]
    fn valid_domain_and_defaults() {
        let q = BadgeQuery {
            style: None,
            label: None,
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.domain, "example.com");
        assert_eq!(r.style, BadgeStyle::Flat);
        assert_eq!(r.label, "lens");
    }

    #[test]
    fn for_the_badge_style_parsed() {
        let q = BadgeQuery {
            style: Some("for-the-badge".into()),
            label: None,
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.style, BadgeStyle::ForTheBadge);
    }

    #[test]
    fn unknown_style_falls_back_to_flat() {
        let q = BadgeQuery {
            style: Some("garbage".into()),
            label: None,
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.style, BadgeStyle::Flat);
    }

    #[test]
    fn label_override_accepted() {
        let q = BadgeQuery {
            style: None,
            label: Some("myco".into()),
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.label, "myco");
    }

    #[test]
    fn label_truncated_to_max_len() {
        let long = "a".repeat(64);
        let q = BadgeQuery {
            style: None,
            label: Some(long),
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.label.len(), 32);
    }

    #[test]
    fn invalid_label_non_ascii_printable_rejected() {
        // 0x01 is a control character, outside 0x20–0x7E range
        let q = BadgeQuery {
            style: None,
            label: Some("bad\x01label".into()),
        };
        let err = parse_badge_request("example.com", q, &default_cfg()).unwrap_err();
        assert!(matches!(err, AppError::InvalidLabel(_)));
    }

    #[test]
    fn label_with_angle_brackets_is_valid_and_escaped_in_svg() {
        // < and > are ASCII printable (0x3C, 0x3E), so they pass the byte check.
        // They are HTML-escaped in the SVG output.
        let q = BadgeQuery {
            style: None,
            label: Some("<script>".into()),
        };
        let r = parse_badge_request("example.com", q, &default_cfg()).unwrap();
        assert_eq!(r.label, "<script>");
    }

    #[test]
    fn invalid_domain_rejected() {
        let q = BadgeQuery {
            style: None,
            label: None,
        };
        let err = parse_badge_request("192.168.1.1", q, &default_cfg()).unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn etag_is_quoted_hex() {
        let etag = compute_etag("example.com", "A", "lens", BadgeStyle::Flat);
        assert!(etag.starts_with('"'), "ETag must start with quote");
        assert!(etag.ends_with('"'), "ETag must end with quote");
        let inner = &etag[1..etag.len() - 1];
        assert_eq!(inner.len(), 16, "ETag must be 16 hex chars");
        assert!(inner.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn etag_differs_by_grade() {
        let e1 = compute_etag("example.com", "A", "lens", BadgeStyle::Flat);
        let e2 = compute_etag("example.com", "B", "lens", BadgeStyle::Flat);
        assert_ne!(e1, e2);
    }

    #[test]
    fn etag_differs_by_style() {
        let e1 = compute_etag("example.com", "A", "lens", BadgeStyle::Flat);
        let e2 = compute_etag("example.com", "A", "lens", BadgeStyle::ForTheBadge);
        assert_ne!(e1, e2);
    }
}
