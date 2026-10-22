// V2 history, Phase 4, requirement 17: http's volatile list covers the cache and timing headers
// and the first-hop server fields, so two runs that differ only there fingerprint equal.

use netray_engine::Module;
use netray_http::{HttpModule, ModuleConfig};
use netray_model::Grade;
use netray_model::evidence::{Block, BlockId, Shape, Value as EvValue};
use netray_model::run::{Run, fingerprint};

fn module() -> HttpModule {
    let cfg: ModuleConfig = toml::from_str("[inspect]\nrequest_timeout_secs = 5").unwrap();
    HttpModule::new(cfg, None).expect("module builds")
}

fn headers(variant: &str, hsts: &str) -> Block {
    let rows = [
        ("content-type", "text/html".to_string()),
        ("strict-transport-security", hsts.to_string()),
        ("expires", format!("Thu, 01 Jan 2026 00:00:0{variant} GMT")),
        ("cf-cache-status", format!("HIT-{variant}")),
        ("x-timer", format!("S1700000000.{variant},VS0")),
        ("x-served-by", format!("cache-fra-{variant}")),
        ("x-cache", format!("HIT, MISS {variant}")),
        ("x-cache-hits", variant.to_string()),
    ];
    Block {
        id: BlockId::parse("http.headers").unwrap(),
        title: "Response headers".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: vec!["name".into(), "value".into()],
            rows: rows
                .into_iter()
                .map(|(n, v)| vec![EvValue::Text(n.into()), EvValue::Text(v)])
                .collect(),
        },
    }
}

fn server(addr: &str, org: &str, network_type: &str) -> Block {
    Block {
        id: BlockId::parse("http.server").unwrap(),
        title: "Server".into(),
        shape: Shape::Fields(vec![
            (
                "final_url".into(),
                EvValue::Text("https://example.com/".into()),
            ),
            ("status_code".into(), EvValue::Int(200)),
            ("address".into(), EvValue::Addr(addr.parse().unwrap())),
            ("org".into(), EvValue::Text(org.into())),
            ("network_type".into(), EvValue::Text(network_type.into())),
        ]),
    }
}

fn run(blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![],
        checks: vec![],
        blocks,
    }
}

fn paths(m: &HttpModule) -> Vec<&'static str> {
    m.volatile().iter().map(|p| p.0).collect()
}

#[test]
fn volatile_headers_and_server_fields_do_not_change_the_fingerprint() {
    let m = module();
    let p = paths(&m);
    let a = run(vec![
        headers("1", "max-age=31536000"),
        server("192.0.2.1", "Example Org A", "hosting"),
    ]);
    let b = run(vec![
        headers("2", "max-age=31536000"),
        server("192.0.2.77", "Example Org B", "cdn"),
    ]);
    assert_eq!(fingerprint(&a, &p), fingerprint(&b, &p));
}

#[test]
fn stable_header_change_changes_the_fingerprint() {
    let m = module();
    let p = paths(&m);
    let a = run(vec![
        headers("1", "max-age=31536000"),
        server("192.0.2.1", "Example Org A", "hosting"),
    ]);
    let b = run(vec![
        headers("1", "max-age=0"),
        server("192.0.2.1", "Example Org A", "hosting"),
    ]);
    assert_ne!(fingerprint(&a, &p), fingerprint(&b, &p));
}
