#[cfg(test)]
mod tests {
    //! C2 / C11: one network-type classifier, same answer on /json, /network and /range.

    use crate::Config;
    use crate::backend::{NetworkFlags, classify_network_type};
    use crate::routes;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    fn flags(f: impl FnOnce(&mut NetworkFlags)) -> NetworkFlags {
        let mut n = NetworkFlags::default();
        f(&mut n);
        n
    }

    #[test]
    fn classify_network_type_priority_table() {
        let cases: Vec<(&str, NetworkFlags, &str)> = vec![
            ("none", NetworkFlags::default(), "residential"),
            (
                "internal+c2",
                flags(|f| {
                    f.is_internal = true;
                    f.is_c2 = true;
                }),
                "internal",
            ),
            (
                "c2+bot",
                flags(|f| {
                    f.is_c2 = true;
                    f.is_bot = true;
                }),
                "c2",
            ),
            (
                "bot+cloud",
                flags(|f| {
                    f.is_bot = true;
                    f.is_cloud = true;
                }),
                "bot",
            ),
            (
                "cloud+vpn",
                flags(|f| {
                    f.is_cloud = true;
                    f.is_vpn = true;
                }),
                "cloud",
            ),
            (
                "vpn+tor",
                flags(|f| {
                    f.is_vpn = true;
                    f.is_tor = true;
                }),
                "vpn",
            ),
            (
                "tor+spamhaus",
                flags(|f| {
                    f.is_tor = true;
                    f.is_spamhaus = true;
                }),
                "tor",
            ),
            (
                "spamhaus+datacenter",
                flags(|f| {
                    f.is_spamhaus = true;
                    f.is_datacenter = true;
                }),
                "spamhaus",
            ),
            ("datacenter", flags(|f| f.is_datacenter = true), "datacenter"),
        ];
        for (name, f, want) in cases {
            assert_eq!(classify_network_type(f), want, "case {name}");
        }
    }

    async fn get_json(app: &axum::Router, uri: &str) -> serde_json::Value {
        let mut req = Request::builder()
            .uri(uri)
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();
        let peer = "198.18.0.1:4000".parse::<SocketAddr>().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        // What `requester_info_middleware` inserts in the full app; the bare router has no
        // such layer, so without it the handlers see `/` and ignore `?ip=`.
        req.extensions_mut().insert(crate::extractors::RequesterInfo {
            remote: peer,
            user_agent: None,
            uri: uri.to_string(),
        });
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "GET {uri}");
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("GET {uri}: not JSON: {e}"))
    }

    #[tokio::test]
    async fn json_network_and_range_report_the_same_type() {
        let dir = std::env::temp_dir().join(format!("ifconfig-classify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tor = dir.join("tor.txt");
        std::fs::write(&tor, "192.0.2.10\n").unwrap();
        let bot = dir.join("bot.jsonl");
        std::fs::write(&bot, "{\"cidr\":\"192.0.2.20/32\",\"provider\":\"examplebot\"}\n").unwrap();

        let mut config = Config::load(Some("ifconfig.dev.toml")).expect("test config");
        config.geoip_city_db = None;
        config.geoip_asn_db = None;
        config.user_agent_regexes = None;
        config.cloud_provider_ranges = None;
        config.feodo_botnet_ips = None;
        config.cins_army_ips = None;
        config.vpn_ranges = None;
        config.datacenter_ranges = None;
        config.spamhaus_drop = None;
        config.asn_info = None;
        config.asn_patterns = None;
        config.tor_exit_nodes = Some(tor.to_string_lossy().into_owned());
        config.bot_ranges = Some(bot.to_string_lossy().into_owned());

        let app = routes::router().with_state(AppState::new(&config).await);

        // Each address sits in exactly one list; all three routes must agree.
        for (addr, want) in [("192.0.2.10", "tor"), ("192.0.2.20", "bot")] {
            let json = get_json(&app, &format!("/json?ip={addr}")).await;
            let network = get_json(&app, &format!("/network?ip={addr}")).await;
            let range = get_json(&app, &format!("/range?cidr={addr}/32")).await;
            assert_eq!(json["network"]["type"], want, "/json for {addr}");
            assert_eq!(network["type"], want, "/network for {addr}");
            assert_eq!(range["network_type"], want, "/range for {addr}");
        }
    }
}
