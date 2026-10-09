#[cfg(test)]
mod tests {
    //! C12-C17: `[rate_limit] exempt_cidrs` skips the per-IP limiter by TCP peer address.

    use crate::Config;
    use crate::routes;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    async fn app(exempt: Option<&[&str]>, trusted: &[&str]) -> axum::Router {
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
        config.rate_limit.per_ip_burst = 1;
        config.rate_limit.per_ip_per_minute = 1;
        if let Some(e) = exempt {
            config.rate_limit.exempt_cidrs = e.iter().map(|s| s.to_string()).collect();
        }
        config.server.trusted_proxies = trusted.iter().map(|s| s.to_string()).collect();
        let state = AppState::new(&config).await;
        routes::router()
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::middleware::rate_limit,
            ))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::extractors::requester_info_middleware,
            ))
            .with_state(state)
    }

    async fn status(app: &axum::Router, peer: &str, xff: Option<&str>) -> StatusCode {
        let mut b = Request::builder().uri("/ip").header("accept", "application/json");
        if let Some(x) = xff {
            b = b.header("x-forwarded-for", x);
        }
        let mut req = b.body(Body::empty()).unwrap();
        let peer: SocketAddr = format!("{peer}:4000").parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        app.clone().oneshot(req).await.unwrap().status()
    }

    const EX: &[&str] = &["172.30.0.0/24"];
    const TRUSTED: &[&str] = &["172.30.0.0/24", "172.31.0.0/24"];

    #[tokio::test]
    async fn c13_exempt_peer_is_never_limited() {
        let app = app(Some(EX), TRUSTED).await;
        for i in 0..3 {
            let s = status(&app, "172.30.0.5", Some("198.51.100.1")).await;
            assert_ne!(s, StatusCode::TOO_MANY_REQUESTS, "request {i}");
        }
    }

    #[tokio::test]
    async fn c14_forwarded_header_cannot_grant_exemption() {
        let a = app(Some(EX), TRUSTED).await;
        assert_ne!(status(&a, "172.31.0.2", Some("172.30.0.5")).await, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            status(&a, "172.31.0.2", Some("172.30.0.5")).await,
            StatusCode::TOO_MANY_REQUESTS,
            "trusted, non-exempt peer"
        );
        let a = app(Some(EX), TRUSTED).await;
        assert_ne!(status(&a, "203.0.113.9", Some("172.30.0.5")).await, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            status(&a, "203.0.113.9", Some("172.30.0.5")).await,
            StatusCode::TOO_MANY_REQUESTS,
            "untrusted peer"
        );
    }

    #[tokio::test]
    async fn c15_default_has_no_exemption() {
        let a = app(None, &[]).await;
        assert_ne!(status(&a, "172.30.0.5", None).await, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(status(&a, "172.30.0.5", None).await, StatusCode::TOO_MANY_REQUESTS);
        assert!(Config::load(Some("ifconfig.dev.toml")).unwrap().rate_limit.exempt_cidrs.is_empty());
    }

    #[test]
    fn c16_non_cidr_exempt_entry_fails_validation() {
        let mut config = Config::load(Some("ifconfig.dev.toml")).expect("test config");
        config.rate_limit.exempt_cidrs = vec!["nope".to_string()];
        let err = config.validate().expect_err("non-CIDR entry must be rejected");
        assert!(err.to_string().contains("rate_limit.exempt_cidrs"), "message: {err}");
    }

    #[test]
    fn c17_production_fixture_carries_exempt_cidrs() {
        let path = "tests/fixtures/ifconfig.production.toml";
        let config = Config::load(Some(path)).expect("fixture loads");
        config.validate().expect("fixture validates");
        let raw: toml::Table = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let rl = raw["rate_limit"].as_table().expect("[rate_limit]");
        assert!(rl.contains_key("exempt_cidrs"), "fixture lacks rate_limit.exempt_cidrs");
    }
}
