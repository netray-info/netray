// CORS probe: the probe Origin is a public constant, and the analysis matches the reflected
// Access-Control-Allow-Origin against it byte-for-byte (`null` counts as reflected).

use netray_http::inspect::cors::{PROBE_ORIGIN, analyze_cors};
use netray_http::quality::types::CheckStatus;
use reqwest::header::HeaderMap;

/// (name, ACAO, credentials, reflects_origin, status, message)
type Row = (
    &'static str,
    String,
    bool,
    bool,
    CheckStatus,
    Option<&'static str>,
);

fn headers(acao: &str, credentials: bool) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("access-control-allow-origin", acao.parse().unwrap());
    if credentials {
        h.insert("access-control-allow-credentials", "true".parse().unwrap());
    }
    h
}

#[test]
fn probe_origin_is_the_evil_example_origin() {
    assert_eq!(PROBE_ORIGIN, "https://evil.example.com");
}

#[test]
fn cors_probe_origin_matching() {
    let rows: Vec<Row> = vec![
        (
            "exact probe origin",
            PROBE_ORIGIN.to_string(),
            true,
            true,
            CheckStatus::Fail,
            None,
        ),
        (
            "null origin",
            "null".into(),
            true,
            true,
            CheckStatus::Fail,
            None,
        ),
        (
            "case-different origin",
            "HTTPS://EVIL.EXAMPLE.COM".into(),
            true,
            false,
            CheckStatus::Pass,
            None,
        ),
        (
            "unrelated origin",
            "https://app.example.com".into(),
            true,
            false,
            CheckStatus::Pass,
            Some("CORS configured"),
        ),
        (
            "wildcard",
            "*".into(),
            false,
            false,
            CheckStatus::Warn,
            None,
        ),
    ];
    for (name, acao, creds, reflects, status, message) in rows {
        let report = analyze_cors(&headers(&acao, creds));
        assert_eq!(report.reflects_origin, reflects, "{name}: reflects_origin");
        assert_eq!(report.status, status, "{name}: status");
        if let Some(m) = message {
            assert_eq!(report.message, m, "{name}: message");
        }
    }
}
