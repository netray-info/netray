//! V2 spec v2-inspectors, Phase 5, requirement 9 (C16): the speculation rules prerender
//! `/inspect/*` pages and still no per-check route.
use netray_web::pages;

/// Prefix match up to the `*`; a pattern without `*` must equal the path.
fn matches(pattern: &str, path: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == path,
        Some((head, _)) => path.starts_with(head),
    }
}

fn patterns(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, x) in m {
                if k == "href_matches" {
                    out.push(x.as_str().unwrap().to_string());
                } else {
                    patterns(x, out);
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| patterns(x, out)),
        _ => {}
    }
}

#[test]
fn the_rules_prerender_inspect_pages_but_not_a_check_route() {
    let v: serde_json::Value = serde_json::from_str(pages::SPECULATION_RULES).unwrap();
    let mut pats = Vec::new();
    patterns(&v, &mut pats);
    assert!(
        pats.iter().any(|p| matches(p, "/inspect/dns")),
        "/inspect/dns must match one of {pats:?}"
    );
    assert!(
        !pats.iter().any(|p| matches(p, "/example.com/tls")),
        "/example.com/tls must match none of {pats:?}"
    );
}
