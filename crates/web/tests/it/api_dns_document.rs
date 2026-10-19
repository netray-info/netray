//! v2-dns-api Phase 6 (requirements 16, 17): the new DNS operations and components are in the
//! v1 document, and the SDD's API section names them.
use netray_web::api::openapi;
use serde_json::Value;

const OPERATIONS: &[(&str, &[&str])] = &[
    ("/api/v1/dns/query", &["get", "post"]),
    ("/api/v1/dns/compare", &["get", "post"]),
    ("/api/v1/dns/authcompare", &["get", "post"]),
    ("/api/v1/dns/trace", &["get", "post"]),
    ("/api/v1/dns/dnssec", &["get", "post"]),
    ("/api/v1/dns/check", &["get", "post"]),
    ("/api/v1/dns/parse", &["post"]),
    ("/api/v1/dns/servers", &["get"]),
    ("/api/v1/dns/record-types", &["get"]),
    ("/api/v1/dns/results/{key}", &["get"]),
];

const COMPONENTS: &[&str] = &[
    "DnsJobSummary",
    "DnsTypeComparison",
    "DnsServerAnswer",
    "DnsServerSummary",
    "DnsRecordDetail",
    "DnsCachedResult",
    "DnsCheckResult",
];

// C1, C4
#[test]
fn the_document_has_every_new_dns_operation_and_component() {
    let doc: Value = serde_json::to_value(openapi()).unwrap();
    for (path, methods) in OPERATIONS {
        for method in *methods {
            assert!(
                doc["paths"][path][method].is_object(),
                "missing {method} {path}"
            );
        }
    }
    for name in COMPONENTS {
        assert!(
            doc["components"]["schemas"][name].is_object(),
            "missing component {name}"
        );
    }
}

fn sdd() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../specs/sdd/v2.md"
    ))
    .expect("the SDD")
}

fn section<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text.find(heading).unwrap_or_else(|| panic!("no {heading}"));
    let rest = &text[start + heading.len()..];
    let end = rest.find("\n### ").unwrap_or(rest.len());
    &rest[..end]
}

// C2, C5
#[test]
fn the_sdd_names_the_new_dns_endpoints() {
    let text = sdd();
    let api = section(&text, "### 7.1 Apex API v1");
    for needle in [
        "`GET /dns/query`",
        "`summary`",
        "/dns/check",
        "/dns/parse",
        "/dns/servers",
        "/dns/record-types",
        "/dns/results/{key}",
    ] {
        assert!(api.contains(needle), "§7.1 does not name {needle}");
    }
    let redirects = section(&text, "### 7.3 Redirect map");
    assert!(
        !redirects.contains("`/api/results/{key}` is cut"),
        "§7.3 still says the replay is cut"
    );
    assert!(
        text.contains("/api/v1/dns/check"),
        "§8 does not name the v1 check endpoint"
    );
}
