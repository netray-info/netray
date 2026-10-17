//! v2-admission C14: `Sec-Purpose` tokens `prefetch` and `prerender`, split on `;` or `,` and
//! compared case-insensitively, mark a request that must never start a run.

use axum::http::{HeaderMap, HeaderValue};
use netray_common::prefetch::is_prefetch;

fn headers(values: &[&str]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for v in values {
        map.append("sec-purpose", HeaderValue::from_str(v).unwrap());
    }
    map
}

#[test]
fn sec_purpose_tokens_decide_prefetch() {
    let rows: &[(&str, &[&str], bool)] = &[
        ("prefetch", &["prefetch"], true),
        ("prerender", &["prerender"], true),
        ("prefetch;prerender", &["prefetch;prerender"], true),
        ("mixed case with a second token", &["PreFetch, x"], true),
        (
            "token in a second header value",
            &["none", "prerender"],
            true,
        ),
        ("none", &["none"], false),
        ("prefix of a token", &["prefetching"], false),
        ("empty value", &[""], false),
        ("no header", &[], false),
    ];
    for (name, values, want) in rows {
        assert_eq!(is_prefetch(&headers(values)), *want, "row: {name}");
    }
}
