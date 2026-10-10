#[cfg(test)]
mod tests {
    //! A response built with `skip_dns` (`&dns=false`) must never be written to the `?ip=` cache,
    //! or a public `/json?ip=X` is later served the hostname-less entry.

    use crate::Config;
    use crate::routes;
    use crate::state::AppState;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode};
    use std::net::{IpAddr, SocketAddr};
    use tower::ServiceExt;

    #[tokio::test]
    async fn dns_false_response_is_not_cached_under_the_bare_ip() {
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
        config.cache.enabled = true;
        let state = AppState::new(&config).await;
        let app = routes::router()
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                crate::extractors::requester_info_middleware,
            ))
            .with_state(state.clone());

        let ip: IpAddr = "93.184.216.34".parse().unwrap();
        let mut req = Request::builder()
            .uri(format!("/json?ip={ip}&dns=false"))
            .header("accept", "application/json")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut()
            .insert(ConnectInfo("198.51.100.7:4000".parse::<SocketAddr>().unwrap()));
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        assert!(
            state.ip_cache.get(&ip).await.is_none(),
            "a skip_dns response must not be cached under the bare IP"
        );
    }
}
