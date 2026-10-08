// Contract: lens must understand the real ifconfig-rs `/json?ip=` body.
// The golden is written by `cargo test -p ifconfig-rs --lib contract_golden`
// (UPDATE_GOLDEN=1 regenerates it).

use lens::backends::ip::check_ip;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

#[tokio::test]
async fn check_ip_reads_real_ifconfig_json_body() {
    let golden: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    let want_type = golden["network"]["type"].as_str().unwrap().to_string();
    let want_org = golden["network"]["org"].as_str().unwrap().to_string();
    let want_geo = format!(
        "{}, {}",
        golden["location"]["city"].as_str().unwrap(),
        golden["location"]["country"].as_str().unwrap()
    );
    assert_ne!(want_type, "residential", "golden must carry a meaningful type");

    let app = axum::Router::new().route(
        "/json",
        axum::routing::get(|| async {
            (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                GOLDEN,
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    let ip: std::net::IpAddr = "203.0.113.42".parse().unwrap();
    let result = check_ip(
        &reqwest::Client::new(),
        &format!("http://{addr}"),
        &[ip],
        std::time::Duration::from_secs(5),
        &Default::default(),
    )
    .await
    .expect("check_ip");

    let info = &result.addresses[0];
    assert_eq!(info.network_type, want_type, "network_type must come from the golden");
    assert_eq!(info.org.as_deref(), Some(want_org.as_str()), "org must come from the golden");
    assert_eq!(info.geo.as_deref(), Some(want_geo.as_str()), "geo must be built from city, country");
}
