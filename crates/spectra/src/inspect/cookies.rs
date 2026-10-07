use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;

use super::assembler::CookieEntry;

/// Parse all Set-Cookie response headers.
pub fn parse_cookies(headers: &HeaderMap) -> Vec<CookieEntry> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(parse_single_cookie)
        .collect()
}

fn parse_single_cookie(raw: &str) -> CookieEntry {
    let mut parts = raw.split(';');

    // First part is name=value
    let name_value = parts.next().unwrap_or("").trim();
    let name = name_value
        .split_once('=')
        .map(|(n, _)| n.trim().to_string())
        .unwrap_or_else(|| name_value.to_string());

    let mut secure = false;
    let mut httponly = false;
    let mut samesite: Option<String> = None;
    let mut path: Option<String> = None;
    let mut domain: Option<String> = None;
    let mut expires: Option<DateTime<Utc>> = None;

    for part in parts {
        let part = part.trim();
        let lower = part.to_lowercase();

        if lower == "secure" {
            secure = true;
        } else if lower == "httponly" {
            httponly = true;
        } else if let Some(val) = lower.strip_prefix("samesite=") {
            samesite = Some(val.trim().to_string());
        } else if let Some(val) = part
            .strip_prefix("Path=")
            .or_else(|| part.strip_prefix("path="))
        {
            path = Some(val.trim().to_string());
        } else if let Some(val) = part
            .strip_prefix("Domain=")
            .or_else(|| part.strip_prefix("domain="))
        {
            domain = Some(val.trim().to_string());
        } else if let Some(val) = part
            .strip_prefix("Expires=")
            .or_else(|| part.strip_prefix("expires="))
        {
            expires = parse_cookie_date(val.trim());
        }
    }

    CookieEntry {
        name,
        secure,
        httponly,
        samesite,
        path,
        domain,
        expires,
    }
}

fn parse_cookie_date(s: &str) -> Option<DateTime<Utc>> {
    // Try common cookie date formats
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT")
                .ok()
                .map(|d| d.and_utc())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_cookies(cookies: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for c in cookies {
            h.append("set-cookie", c.parse().unwrap());
        }
        h
    }

    #[test]
    fn parses_simple_cookie() {
        let h = headers_with_cookies(&["session=abc; HttpOnly; Secure; Path=/"]);
        let cookies = parse_cookies(&h);
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "session");
        assert!(cookies[0].httponly);
        assert!(cookies[0].secure);
        assert_eq!(cookies[0].path.as_deref(), Some("/"));
    }

    #[test]
    fn detects_insecure_cookie() {
        let h = headers_with_cookies(&["session=abc; HttpOnly"]);
        let cookies = parse_cookies(&h);
        assert_eq!(cookies.len(), 1);
        assert!(!cookies[0].secure);
    }

    #[test]
    fn parses_samesite() {
        let h = headers_with_cookies(&["token=xyz; SameSite=Strict; Secure"]);
        let cookies = parse_cookies(&h);
        assert_eq!(cookies[0].samesite.as_deref(), Some("strict"));
    }

    #[test]
    fn parses_domain() {
        let h = headers_with_cookies(&["id=1; Domain=.example.com"]);
        let cookies = parse_cookies(&h);
        assert_eq!(cookies[0].domain.as_deref(), Some(".example.com"));
    }

    #[test]
    fn parses_multiple_cookies() {
        let h = headers_with_cookies(&["a=1; Secure", "b=2; HttpOnly"]);
        let cookies = parse_cookies(&h);
        assert_eq!(cookies.len(), 2);
        assert!(cookies[0].secure);
        assert!(cookies[1].httponly);
    }
}
