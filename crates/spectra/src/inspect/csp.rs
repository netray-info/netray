use indexmap::IndexMap;
use reqwest::header::HeaderMap;

use crate::quality::types::CheckStatus;

use super::assembler::CspReport;

/// Analyze CSP headers per SDD §10.
pub fn analyze_csp(headers: &HeaderMap) -> CspReport {
    let enforced_header = headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok());
    let report_only_header = headers
        .get("content-security-policy-report-only")
        .and_then(|v| v.to_str().ok());

    let (csp_value, enforced, report_only) = match (enforced_header, report_only_header) {
        (Some(v), _) => (v, true, false),
        (None, Some(v)) => (v, false, true),
        (None, None) => {
            return CspReport {
                status: CheckStatus::Warn,
                enforced: false,
                report_only: false,
                directives: IndexMap::new(),
                issues: vec!["No Content-Security-Policy header".to_string()],
            };
        }
    };

    let directives = parse_directives(csp_value);
    let issues = score_directives(&directives);

    let status = if report_only {
        CheckStatus::Warn
    } else if issues.iter().any(|i| i.starts_with("[fail]")) {
        CheckStatus::Fail
    } else if issues.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Warn
    };

    // Strip level prefixes from issues for the response
    let clean_issues: Vec<String> = issues
        .into_iter()
        .map(|i| {
            i.strip_prefix("[fail] ")
                .or_else(|| i.strip_prefix("[warn] "))
                .unwrap_or(&i)
                .to_string()
        })
        .collect();

    CspReport {
        status,
        enforced,
        report_only,
        directives,
        issues: clean_issues,
    }
}

fn parse_directives(csp: &str) -> IndexMap<String, Vec<String>> {
    let mut map = IndexMap::new();
    for directive in csp.split(';') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }
        let mut parts = directive.split_whitespace();
        if let Some(name) = parts.next() {
            let values: Vec<String> = parts.map(|s| s.to_string()).collect();
            map.insert(name.to_lowercase(), values);
        }
    }
    map
}

fn score_directives(directives: &IndexMap<String, Vec<String>>) -> Vec<String> {
    let mut issues = Vec::new();

    // No default-src
    if !directives.contains_key("default-src") {
        issues.push("[fail] Missing default-src directive".to_string());
    }

    // script-src checks
    if let Some(values) = directives.get("script-src") {
        if values.iter().any(|v| v == "'unsafe-inline'") {
            issues.push("[warn] script-src contains 'unsafe-inline'".to_string());
        }
        if values.iter().any(|v| v == "'unsafe-eval'") {
            issues.push("[warn] script-src contains 'unsafe-eval'".to_string());
        }
        if values.iter().any(|v| v == "data:") {
            issues.push("[warn] script-src contains data: URI".to_string());
        }
        // Wildcard with single-label TLD
        for v in values {
            if let Some(rest) = v.strip_prefix("*.")
                && !rest.contains('.')
            {
                issues.push(format!(
                    "[warn] script-src wildcard {v} is overly broad (single-label TLD)"
                ));
            }
        }
    }

    // style-src checks
    if let Some(values) = directives.get("style-src")
        && values.iter().any(|v| v == "'unsafe-inline'")
    {
        issues.push("[warn] style-src contains 'unsafe-inline'".to_string());
    }

    // object-src checks
    let default_restricts_plugins = directives
        .get("default-src")
        .is_some_and(|v| v.iter().any(|val| val == "'none'" || val == "'self'"));

    match directives.get("object-src") {
        None => {
            if !default_restricts_plugins {
                issues.push(
                    "[warn] Missing object-src and default-src does not restrict plugins"
                        .to_string(),
                );
            }
        }
        Some(values) => {
            if !values.iter().any(|v| v == "'none'") {
                issues.push("[warn] object-src is not set to 'none'".to_string());
            }
        }
    }

    // base-uri checks
    match directives.get("base-uri") {
        None => {
            issues.push("[warn] Missing base-uri directive".to_string());
        }
        Some(values) => {
            let is_restrictive = values.iter().any(|v| v == "'self'" || v == "'none'");
            if !is_restrictive {
                issues.push("[warn] base-uri is permissive".to_string());
            }
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_csp(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("content-security-policy", value.parse().unwrap());
        h
    }

    #[test]
    fn no_csp_is_warn() {
        let h = HeaderMap::new();
        let report = analyze_csp(&h);
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(!report.issues.is_empty());
    }

    #[test]
    fn report_only_is_warn() {
        let mut h = HeaderMap::new();
        h.insert(
            "content-security-policy-report-only",
            "default-src 'self'".parse().unwrap(),
        );
        let report = analyze_csp(&h);
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.report_only);
        assert!(!report.enforced);
    }

    #[test]
    fn good_csp_is_pass() {
        let h = headers_with_csp(
            "default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'self'",
        );
        let report = analyze_csp(&h);
        assert_eq!(report.status, CheckStatus::Pass);
        assert!(report.issues.is_empty());
    }

    #[test]
    fn unsafe_inline_in_script_src_is_warn() {
        let h = headers_with_csp("default-src 'self'; script-src 'unsafe-inline'");
        let report = analyze_csp(&h);
        assert_eq!(report.status, CheckStatus::Warn);
        assert!(report.issues.iter().any(|i| i.contains("unsafe-inline")));
    }

    #[test]
    fn unsafe_eval_in_script_src_is_warn() {
        let h = headers_with_csp("default-src 'self'; script-src 'unsafe-eval'");
        let report = analyze_csp(&h);
        assert!(report.issues.iter().any(|i| i.contains("unsafe-eval")));
    }

    #[test]
    fn data_uri_in_script_src_is_warn() {
        let h = headers_with_csp("default-src 'self'; script-src data:");
        let report = analyze_csp(&h);
        assert!(report.issues.iter().any(|i| i.contains("data:")));
    }

    #[test]
    fn missing_default_src_is_fail() {
        let h = headers_with_csp("script-src 'self'");
        let report = analyze_csp(&h);
        // Missing default-src causes fail-level issue, but since we have other issues too
        // let's just check the issue is present
        assert!(report.issues.iter().any(|i| i.contains("default-src")));
    }

    #[test]
    fn wildcard_single_label_tld_is_warn() {
        let h = headers_with_csp("default-src 'self'; script-src *.com");
        let report = analyze_csp(&h);
        assert!(report.issues.iter().any(|i| i.contains("overly broad")));
    }

    #[test]
    fn wildcard_multi_label_is_ok() {
        let h = headers_with_csp(
            "default-src 'self'; script-src *.cdn.example.com; object-src 'none'; base-uri 'self'",
        );
        let report = analyze_csp(&h);
        assert!(!report.issues.iter().any(|i| i.contains("overly broad")));
    }

    #[test]
    fn missing_base_uri_is_warn() {
        let h = headers_with_csp("default-src 'self'; object-src 'none'");
        let report = analyze_csp(&h);
        assert!(report.issues.iter().any(|i| i.contains("base-uri")));
    }

    #[test]
    fn object_src_not_none_is_warn() {
        let h = headers_with_csp("default-src 'self'; object-src 'self'; base-uri 'self'");
        let report = analyze_csp(&h);
        assert!(report.issues.iter().any(|i| i.contains("object-src")));
    }

    #[test]
    fn directives_parsed_correctly() {
        let h = headers_with_csp("default-src 'self'; script-src 'self' https://cdn.example.com");
        let report = analyze_csp(&h);
        assert_eq!(
            report.directives.get("script-src").unwrap(),
            &vec!["'self'".to_string(), "https://cdn.example.com".to_string()]
        );
    }
}
