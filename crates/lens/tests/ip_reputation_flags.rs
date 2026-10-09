// Contract: lens scores an address from ifconfig-rs's booleans. `is_spamhaus`, `is_c2`,
// `is_tor` -> Fail; `is_vpn` -> Warn; otherwise Pass. `network.type` is display only.
// A body without the boolean flags is not Pass: check_ip returns Err.
// (grade-integrity, requirement 3: C1, C3, C4.)

use std::net::IpAddr;
use std::time::Duration;

use lens::backends::ip::check_ip;
use lens::scoring::engine::CheckVerdict;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

fn public_or_documentation(ip: IpAddr) -> bool {
    match netray_common::target_policy::refusal_reason(ip) {
        None => true,
        Some(r) => r.starts_with("documentation"),
    }
}

/// The golden with the given `network.<key>` values set.
fn golden_with(sets: &[(&str, serde_json::Value)]) -> String {
    let mut v: serde_json::Value = serde_json::from_str(GOLDEN).unwrap();
    for (k, val) in sets {
        v["network"][*k] = val.clone();
    }
    v.to_string()
}

async fn reputation(body: String) -> Result<CheckVerdict, String> {
    let app = axum::Router::new().route(
        "/json",
        axum::routing::get(move || {
            let body = body.clone();
            async move { ([(axum::http::header::CONTENT_TYPE, "application/json")], body) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    let ip: IpAddr = "203.0.113.42".parse().unwrap();
    let r = check_ip(
        &reqwest::Client::new(),
        &format!("http://{addr}"),
        &[ip],
        Duration::from_secs(5),
        &Default::default(),
        public_or_documentation,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    Ok(r
        .checks
        .iter()
        .find(|c| c.name == "reputation")
        .expect("reputation check")
        .verdict
        .clone())
}

#[tokio::test]
async fn reputation_is_scored_from_ifconfig_flags() {
    use serde_json::json;
    let t = json!(true);
    let rows: Vec<(&str, String, Result<CheckVerdict, ()>)> = vec![
        ("golden unchanged", golden_with(&[]), Ok(CheckVerdict::Pass)),
        ("is_spamhaus", golden_with(&[("is_spamhaus", t.clone())]), Ok(CheckVerdict::Fail)),
        ("is_c2", golden_with(&[("is_c2", t.clone())]), Ok(CheckVerdict::Fail)),
        ("is_tor", golden_with(&[("is_tor", t.clone())]), Ok(CheckVerdict::Fail)),
        ("is_vpn", golden_with(&[("is_vpn", t.clone())]), Ok(CheckVerdict::Warn)),
        // type is display only: "tor" with every flag false is Pass
        ("type tor, no flags", golden_with(&[("type", json!("tor"))]), Ok(CheckVerdict::Pass)),
        // a body without the boolean flags must not pass
        (
            "no flags",
            r#"{"network":{"type":"cloud","org":"X"},"location":{}}"#.to_string(),
            Err(()),
        ),
    ];

    let mut failures = Vec::new();
    for (name, body, want) in rows {
        let got = reputation(body).await;
        let ok = match (&got, &want) {
            (Ok(g), Ok(w)) => g == w,
            (Err(_), Err(())) => true,
            _ => false,
        };
        if !ok {
            failures.push(format!("{name}: want {want:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
