// Requirement 3: only public addresses are enriched; at most four IPv4 and four
// IPv6, each family sorted ascending; "checked N of M addresses" appears only
// when N < M.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::Query;
use lens::backends::ip::check_ip;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

fn public_or_documentation(ip: IpAddr) -> bool {
    match netray_common::target_policy::refusal_reason(ip) {
        None => true,
        Some(r) => r.starts_with("documentation"),
    }
}

fn ips(list: &[&str]) -> Vec<IpAddr> {
    list.iter().map(|s| s.parse().unwrap()).collect()
}

struct Case {
    name: &'static str,
    input: Vec<IpAddr>,
    sent: Vec<&'static str>,
    message: Option<&'static str>,
}

#[tokio::test]
async fn ip_sampling_table() {
    let cases = vec![
        Case {
            name: "C7 ten A and two AAAA",
            input: ips(&[
                "192.0.2.9", "192.0.2.3", "192.0.2.10", "192.0.2.1", "2001:db8::9",
                "192.0.2.7", "192.0.2.5", "192.0.2.2", "192.0.2.8", "2001:db8::2",
                "192.0.2.6", "192.0.2.4",
            ]),
            sent: vec![
                "192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4", "2001:db8::2", "2001:db8::9",
            ],
            message: Some("checked 6 of 12 addresses"),
        },
        Case {
            name: "C8 five A",
            input: ips(&["192.0.2.5", "192.0.2.2", "192.0.2.4", "192.0.2.1", "192.0.2.3"]),
            sent: vec!["192.0.2.1", "192.0.2.2", "192.0.2.3", "192.0.2.4"],
            message: Some("checked 4 of 5 addresses"),
        },
        Case {
            name: "C9 three A",
            input: ips(&["192.0.2.3", "192.0.2.1", "192.0.2.2"]),
            sent: vec!["192.0.2.1", "192.0.2.2", "192.0.2.3"],
            message: None,
        },
        Case {
            name: "C10 private filtered",
            input: ips(&["10.0.0.5", "198.51.100.7"]),
            sent: vec!["198.51.100.7"],
            message: Some("checked 1 of 2 addresses"),
        },
    ];

    for case in cases {
        let seen: Arc<Mutex<Vec<String>>> = Arc::default();
        let rec = seen.clone();
        let app = axum::Router::new().route(
            "/json",
            axum::routing::get(
                move |Query(q): Query<std::collections::HashMap<String, String>>| {
                    let rec = rec.clone();
                    async move {
                        if let Some(ip) = q.get("ip") {
                            rec.lock().unwrap().push(ip.clone());
                        }
                        (
                            [(axum::http::header::CONTENT_TYPE, "application/json")],
                            GOLDEN,
                        )
                    }
                },
            ),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let result = check_ip(
            &reqwest::Client::new(),
            &format!("http://{addr}"),
            &case.input,
            Duration::from_secs(5),
            &Default::default(),
            public_or_documentation,
        )
        .await
        .unwrap_or_else(|e| panic!("{}: check_ip failed: {e:?}", case.name));

        let mut got = seen.lock().unwrap().clone();
        got.sort_by_key(|s| s.parse::<IpAddr>().unwrap());
        let mut want: Vec<IpAddr> = case.sent.iter().map(|s| s.parse().unwrap()).collect();
        want.sort();
        let want: Vec<String> = want.iter().map(|i| i.to_string()).collect();
        assert_eq!(got, want, "{}: addresses sent to /json?ip=", case.name);

        let rep = result
            .checks
            .iter()
            .find(|c| c.name == "reputation")
            .unwrap_or_else(|| panic!("{}: no reputation check", case.name));
        let hit = rep.messages.iter().any(|m| m.contains("checked"));
        match case.message {
            Some(msg) => assert!(
                rep.messages.iter().any(|m| m.contains(msg)),
                "{}: expected {msg:?} in {:?}",
                case.name,
                rep.messages
            ),
            None => assert!(!hit, "{}: unexpected 'checked' in {:?}", case.name, rep.messages),
        }
    }
}
