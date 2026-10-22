// v2-backend-followups R1: `http.caching` holds one `directive` per Cache-Control directive
// over every header line, in received order, repeats kept; name lowercased, argument as received.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netray_engine::SectionOutcome;
use netray_http::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use netray_http::module::{REGISTER, translate};
use netray_model::evidence::{Block, Shape, Value as Ev, validate};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

fn task(headers: HeaderMap) -> TaskResult {
    TaskResult {
        final_url: "https://example.com/".into(),
        status: 200,
        http_version: "HTTP/1.1".into(),
        headers,
        redirects: vec![],
        redirect_limit_reached: false,
        error: None,
    }
}

fn run(lines: &[(&str, &str)]) -> Vec<Block> {
    let mut h = HeaderMap::new();
    h.insert("content-type", HeaderValue::from_static("text/html"));
    for (k, v) in lines {
        h.append(
            HeaderName::from_bytes(k.as_bytes()).unwrap(),
            HeaderValue::from_str(v).unwrap(),
        );
    }
    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)), 443),
        InspectResult {
            https: task(h.clone()),
            http_upgrade: None,
            cors: task(h),
        },
        EnrichmentData::default(),
        None,
        1,
    );
    match translate(&resp) {
        SectionOutcome::Measured { evidence, .. } => {
            validate("http", &REGISTER, &evidence).expect("blocks fit the register");
            evidence
        }
        other => panic!("expected Measured, got {other:?}"),
    }
}

fn caching(blocks: &[Block]) -> &[(String, Ev)] {
    blocks
        .iter()
        .find(|b| b.id.to_string() == "http.caching")
        .map(|b| match &b.shape {
            Shape::Fields(f) => f.as_slice(),
            other => panic!("http.caching must be fields, got {other:?}"),
        })
        .unwrap_or_else(|| panic!("no http.caching block: {blocks:?}"))
}

fn directives(f: &[(String, Ev)]) -> Vec<&str> {
    f.iter()
        .filter(|(k, _)| k == "directive")
        .map(|(_, v)| match v {
            Ev::Text(s) => s.as_str(),
            other => panic!("directive must be Text, got {other:?}"),
        })
        .collect()
}

/// (name, header lines, expected directives, expected max_age)
type Case = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
    Option<i64>,
);

#[test]
fn cache_control_directives_are_listed_as_received() {
    let cases: &[Case] = &[
        (
            "C7 plain list",
            &[("cache-control", "public, max-age=3600, must-revalidate")],
            &["public", "max-age=3600", "must-revalidate"],
            Some(3600),
        ),
        (
            "C8 two lines, lowercased, order kept",
            &[
                ("cache-control", "Max-Age=60, s-maxage=600"),
                ("cache-control", "stale-while-revalidate=30, no-transform"),
            ],
            &[
                "max-age=60",
                "s-maxage=600",
                "stale-while-revalidate=30",
                "no-transform",
            ],
            Some(60),
        ),
        (
            "C9 comma inside a quoted argument",
            &[("cache-control", "no-cache=\"set-cookie, x-trace\", private")],
            &["no-cache=\"set-cookie, x-trace\"", "private"],
            None,
        ),
        (
            "C10 empty members skipped",
            &[("cache-control", "public, , no-store,")],
            &["public", "no-store"],
            None,
        ),
        (
            "C11 no Cache-Control, no directive key",
            &[("etag", "\"abc\"")],
            &[],
            None,
        ),
    ];
    for (name, lines, want, max_age) in cases {
        let blocks = run(lines);
        let f = caching(&blocks);
        assert_eq!(directives(f), *want, "{name}: {f:?}");
        let got_max: Vec<&Ev> = f
            .iter()
            .filter(|(k, _)| k == "max_age")
            .map(|(_, v)| v)
            .collect();
        match max_age {
            Some(n) => assert_eq!(got_max, vec![&Ev::Int(*n)], "{name}: {f:?}"),
            None => assert!(got_max.is_empty(), "{name}: {f:?}"),
        }
    }
}
