use axum::http::HeaderMap;

pub fn extract_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    const MAX_HEADERS: usize = 64;
    const MAX_VALUE_LEN: usize = 1024;
    headers
        .iter()
        .take(MAX_HEADERS)
        .map(|(name, value)| {
            let v = value.to_str().unwrap_or("");
            // Header values are ASCII; safe to slice at byte boundary.
            let v = if v.len() > MAX_VALUE_LEN {
                &v[..MAX_VALUE_LEN]
            } else {
                v
            };
            (name.to_string(), v.to_string())
        })
        .collect()
}

/// Remove headers whose names match any of the provided regex filters.
pub fn filter_headers(headers: Vec<(String, String)>, filters: &regex::RegexSet) -> Vec<(String, String)> {
    if filters.is_empty() {
        return headers;
    }
    headers
        .into_iter()
        .filter(|(name, _)| !filters.is_match(name))
        .collect()
}

/// The header-name filter set for `patterns`; an invalid pattern is dropped with a warning.
pub fn header_filters(patterns: &[String]) -> regex::RegexSet {
    let valid: Vec<&str> = patterns
        .iter()
        .filter(|pattern| match regex::Regex::new(pattern) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("Invalid header filter regex '{}': {}", pattern, e);
                false
            }
        })
        .map(|s| s.as_str())
        .collect();
    regex::RegexSet::new(&valid).expect("pre-validated regex patterns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_headers_empty_filters() {
        let headers = vec![
            ("host".into(), "example.com".into()),
            ("x-koyeb-id".into(), "abc".into()),
        ];
        let filters = regex::RegexSet::empty();
        let result = filter_headers(headers.clone(), &filters);
        assert_eq!(result, headers);
    }

    #[test]
    fn filter_headers_removes_matching() {
        let headers = vec![
            ("host".into(), "example.com".into()),
            ("x-koyeb-id".into(), "abc".into()),
            ("x-koyeb-region".into(), "par".into()),
            ("cf-ray".into(), "123".into()),
            ("accept".into(), "*/*".into()),
        ];
        let filters = regex::RegexSet::new(["^x-koyeb-", "^cf-"]).unwrap();
        let result = filter_headers(headers, &filters);
        assert_eq!(
            result,
            vec![("host".into(), "example.com".into()), ("accept".into(), "*/*".into()),]
        );
    }

    #[test]
    fn filter_headers_no_match_keeps_all() {
        let headers = vec![
            ("host".into(), "example.com".into()),
            ("accept".into(), "text/html".into()),
        ];
        let filters = regex::RegexSet::new(["^x-koyeb-"]).unwrap();
        let result = filter_headers(headers.clone(), &filters);
        assert_eq!(result, headers);
    }
}
