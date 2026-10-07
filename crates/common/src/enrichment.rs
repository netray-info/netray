//! IP enrichment client for the netray.info service ecosystem.
//!
//! Provides a shared client for fetching ASN, cloud provider, and threat-flag
//! metadata from an ifconfig-rs compatible API. [`EnrichmentMode::Backend`]
//! delegates HTTP transport, concurrency limiting, caching, and metrics to
//! [`crate::backend::BackendClient`].

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};

/// Cloud provider metadata from the enrichment API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct CloudInfo {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
}

/// Metadata about a single IP address from the enrichment API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct IpInfo {
    #[serde(default)]
    pub asn: Option<u32>,
    #[serde(default)]
    pub org: Option<String>,
    /// IP classification: "cloud", "datacenter", "residential", "vpn", etc.
    #[serde(default, rename = "type")]
    pub ip_type: Option<String>,
    #[serde(default)]
    pub cloud: Option<CloudInfo>,
    #[serde(default)]
    pub is_tor: bool,
    #[serde(default)]
    pub is_vpn: bool,
    #[serde(default)]
    pub is_datacenter: bool,
    #[serde(default)]
    pub is_spamhaus: bool,
    #[serde(default)]
    pub is_c2: bool,
    /// ASN network role from ipverse/as-metadata (e.g. "Midsize Transit", "Access Provider").
    #[serde(default)]
    pub network_role: Option<String>,
}

/// Returns `true` for any IP address that should not be sent to the enrichment
/// API — private, reserved, or special-purpose ranges including CGNAT
/// (100.64.0.0/10).
///
/// Uses the unified [`crate::target_policy`] blocklist.
pub fn is_private_ip(ip: IpAddr) -> bool {
    !crate::target_policy::is_allowed_target(ip)
}

/// Transport an [`EnrichmentClient`] uses, chosen by the caller at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentMode {
    /// Standalone `reqwest::Client` with the caller's `User-Agent`, no cache.
    Plain,
    /// [`crate::backend::BackendClient`] with concurrency limiting and metrics;
    /// a `cache_ttl_secs` of 0 disables the cache. Requires the `backend` feature.
    Backend { cache_ttl_secs: u64 },
}

enum Transport {
    Plain {
        client: reqwest::Client,
        metrics_label: Option<&'static str>,
    },
    #[cfg(feature = "backend")]
    Backend(crate::backend::BackendClient),
}

/// HTTP client for IP enrichment lookups against an ifconfig-rs compatible API.
///
/// The transport is selected by [`EnrichmentMode`] at construction time.
pub struct EnrichmentClient {
    transport: Transport,
    base_url: String,
}

impl EnrichmentClient {
    /// Returns the base URL of the enrichment API (without trailing slash).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Create a new enrichment client.
    ///
    /// - `base_url` -- ifconfig API base URL (e.g. `https://ip.netray.info`)
    /// - `timeout` -- per-request HTTP timeout
    /// - `user_agent` -- `User-Agent` header value sent in [`EnrichmentMode::Plain`]
    /// - `metrics_label` -- when `Some`, emit Prometheus counters tagged with
    ///   `service = <label>`. Pass `None` to skip metrics.
    /// - `mode` -- transport and cache selection
    ///
    /// # Panics
    ///
    /// On [`EnrichmentMode::Backend`] when built without the `backend` feature.
    pub fn new(
        base_url: &str,
        timeout: Duration,
        user_agent: &'static str,
        metrics_label: Option<&'static str>,
        mode: EnrichmentMode,
    ) -> Self {
        let trimmed = base_url.trim_end_matches('/').to_owned();

        let transport = match mode {
            EnrichmentMode::Plain => {
                let client = reqwest::Client::builder()
                    .timeout(timeout)
                    .user_agent(user_agent)
                    .pool_max_idle_per_host(5)
                    .pool_idle_timeout(Duration::from_secs(90))
                    .build()
                    .expect("failed to build enrichment HTTP client");
                Transport::Plain {
                    client,
                    metrics_label,
                }
            }
            #[cfg(feature = "backend")]
            EnrichmentMode::Backend { cache_ttl_secs } => {
                let config = crate::backend::BackendConfig {
                    url: Some(trimmed.clone()),
                    timeout_ms: timeout.as_millis() as u64,
                    max_concurrent: 20,
                    cache_ttl_secs,
                    cache_capacity: 1024,
                };
                let backend = crate::backend::BackendClient::new(
                    &config,
                    "ifconfig",
                    metrics_label.unwrap_or(""),
                )
                .expect("BackendClient::new returned None with Some(url)");
                Transport::Backend(backend)
            }
            #[cfg(not(feature = "backend"))]
            EnrichmentMode::Backend { .. } => {
                panic!("EnrichmentMode::Backend requires the netray-common `backend` feature")
            }
        };

        Self {
            transport,
            base_url: trimmed,
        }
    }

    /// Probe reachability with a HEAD request and a 2-second timeout.
    ///
    /// Returns `true` if the service responds with an HTTP status below 500.
    /// Non-fatal: network errors and timeouts return `false`.
    pub async fn is_reachable(&self) -> bool {
        match &self.transport {
            #[cfg(feature = "backend")]
            Transport::Backend(backend) => backend
                .head("/")
                .await
                .map(|s| s.as_u16() < 500)
                .unwrap_or(false),
            Transport::Plain { .. } => {
                let client = match reqwest::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                {
                    Ok(c) => c,
                    Err(_) => return false,
                };
                client
                    .head(&self.base_url)
                    .send()
                    .await
                    .map(|r| r.status().as_u16() < 500)
                    .unwrap_or(false)
            }
        }
    }

    /// Look up metadata for a single IP.
    ///
    /// Returns `None` for private/blocked IPs (no request sent) and on any
    /// HTTP or parse error (non-fatal).
    pub async fn lookup(&self, ip: IpAddr, request_id: Option<&str>) -> Option<IpInfo> {
        if is_private_ip(ip) {
            return None;
        }

        match &self.transport {
            #[cfg(feature = "backend")]
            Transport::Backend(backend) => {
                let path = format!("/network/json?ip={}", ip);
                match backend.get(&path, request_id).await {
                    Ok((_status, body)) => {
                        tracing::debug!(ip = %ip, service = "ifconfig", "enrichment lookup succeeded");
                        serde_json::from_slice::<IpInfo>(&body).ok()
                    }
                    Err(e) => {
                        tracing::warn!(ip = %ip, service = "ifconfig", error = %e, "enrichment lookup error");
                        None
                    }
                }
            }
            Transport::Plain {
                client,
                metrics_label,
            } => {
                if let Some(svc) = *metrics_label {
                    metrics::counter!("enrichment_requests_total", "service" => svc).increment(1);
                }

                let url = format!("{}/network/json?ip={}", self.base_url, ip);
                let mut req = client.get(&url);
                if let Some(rid) = request_id {
                    req = req.header("X-Request-Id", rid);
                }
                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::debug!(ip = %ip, service = "ifconfig", url = %url, "enrichment lookup succeeded");
                        resp.json::<IpInfo>().await.ok()
                    }
                    Ok(resp) => {
                        tracing::warn!(ip = %ip, service = "ifconfig", url = %url, status = %resp.status(), "enrichment lookup failed");
                        None
                    }
                    Err(e) => {
                        tracing::warn!(ip = %ip, service = "ifconfig", url = %url, error = %e, "enrichment lookup error");
                        None
                    }
                }
            }
        }
    }

    /// Look up metadata for multiple IPs concurrently.
    ///
    /// Deduplicates the input and silently skips private/blocked IPs.
    /// Returns only the IPs for which enrichment succeeded.
    pub async fn lookup_batch(
        &self,
        ips: &[IpAddr],
        request_id: Option<&str>,
    ) -> HashMap<IpAddr, IpInfo> {
        let rid = request_id.map(|s| s.to_owned());
        let mut seen = std::collections::HashSet::new();
        let futs: FuturesUnordered<_> = ips
            .iter()
            .copied()
            .filter(|ip| seen.insert(*ip))
            .map(|ip| {
                let rid = rid.clone();
                async move { (ip, self.lookup(ip, rid.as_deref()).await) }
            })
            .collect();

        futs.filter_map(|(ip, info)| async move { info.map(|i| (ip, i)) })
            .collect()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_full_response() {
        let json = r#"{
            "type": "cloud",
            "asn": 16509,
            "org": "Amazon.com, Inc.",
            "cloud": { "provider": "AWS", "region": "us-east-1", "service": "EC2" },
            "is_tor": false, "is_vpn": false, "is_datacenter": true,
            "is_spamhaus": false, "is_c2": false
        }"#;
        let info: IpInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, Some(16509));
        assert_eq!(info.ip_type.as_deref(), Some("cloud"));
        assert!(info.is_datacenter);
        assert!(!info.is_tor);
        let cloud = info.cloud.unwrap();
        assert_eq!(cloud.provider.as_deref(), Some("AWS"));
        assert_eq!(cloud.region.as_deref(), Some("us-east-1"));
    }

    #[test]
    fn deserializes_minimal_response() {
        let json = r#"{}"#;
        let info: IpInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.asn, None);
        assert_eq!(info.org, None);
        assert!(!info.is_tor);
    }

    #[test]
    fn lookup_skips_private_ips() {
        // is_blocked_ip covers all private ranges
        assert!(crate::ip_filter::is_blocked_ip(
            "127.0.0.1".parse().unwrap()
        ));
        assert!(crate::ip_filter::is_blocked_ip("10.0.0.1".parse().unwrap()));
        assert!(crate::ip_filter::is_blocked_ip("::1".parse().unwrap()));
        assert!(crate::ip_filter::is_blocked_ip("fc00::1".parse().unwrap()));
    }

    #[test]
    fn public_ips_not_blocked() {
        assert!(!crate::ip_filter::is_blocked_ip("8.8.8.8".parse().unwrap()));
        assert!(!crate::ip_filter::is_blocked_ip("1.1.1.1".parse().unwrap()));
        assert!(!crate::ip_filter::is_blocked_ip(
            "2606:4700::1".parse().unwrap()
        ));
    }

    #[test]
    fn is_private_ip_blocks_standard_ranges() {
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.1".parse().unwrap()));
        assert!(is_private_ip("172.16.0.1".parse().unwrap()));
        assert!(is_private_ip("::1".parse().unwrap()));
        assert!(is_private_ip("fc00::1".parse().unwrap()));
    }

    #[test]
    fn is_private_ip_blocks_cgnat() {
        assert!(is_private_ip("100.64.0.1".parse().unwrap()));
        assert!(is_private_ip("100.127.255.255".parse().unwrap()));
    }

    #[test]
    fn is_private_ip_allows_public() {
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip("2606:4700::1".parse().unwrap()));
    }

    // T5: EnrichmentClient calls /network/json with X-Request-Id (backend feature)
    #[cfg(feature = "backend")]
    #[tokio::test]
    async fn lookup_calls_network_json_with_request_id() {
        use std::sync::Arc;

        let received = Arc::new(tokio::sync::Mutex::new((String::new(), String::new())));
        let recv = received.clone();

        let app = axum::Router::new().route(
            "/network/json",
            axum::routing::get(
                move |headers: axum::http::HeaderMap,
                      axum::extract::Query(params): axum::extract::Query<
                    HashMap<String, String>,
                >| {
                    let recv = recv.clone();
                    async move {
                        let ip = params.get("ip").cloned().unwrap_or_default();
                        let rid = headers
                            .get("x-request-id")
                            .map(|v| v.to_str().unwrap().to_owned())
                            .unwrap_or_default();
                        *recv.lock().await = (ip, rid);
                        axum::Json(serde_json::json!({
                            "asn": 15169,
                            "org": "Google LLC",
                            "type": "cloud"
                        }))
                    }
                },
            ),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let client = EnrichmentClient::new(
            &format!("http://{addr}"),
            Duration::from_secs(5),
            "test",
            None,
            EnrichmentMode::Backend { cache_ttl_secs: 0 },
        );

        let result = client
            .lookup("8.8.8.8".parse().unwrap(), Some("req-abc-123"))
            .await;
        assert!(result.is_some(), "lookup should return Some for valid IP");

        let info = result.unwrap();
        assert_eq!(info.asn, Some(15169));
        assert_eq!(info.org.as_deref(), Some("Google LLC"));

        let (captured_ip, captured_rid) = &*received.lock().await;
        assert_eq!(captured_ip, "8.8.8.8", "should query for the requested IP");
        assert_eq!(captured_rid, "req-abc-123", "should propagate X-Request-Id");
    }

    // B3: behaviour is chosen at runtime by EnrichmentMode, not by Cargo features.
    #[tokio::test]
    async fn enrichment_mode_selects_user_agent_and_cache_at_runtime() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        // (mode, expected requests for two lookups of the same IP, expected User-Agent)
        let cases: Vec<(&str, EnrichmentMode, usize, Option<&str>)> = vec![
            ("plain", EnrichmentMode::Plain, 2, Some("spectra")),
            (
                "backend ttl 0",
                EnrichmentMode::Backend { cache_ttl_secs: 0 },
                2,
                None,
            ),
            (
                "backend ttl 300",
                EnrichmentMode::Backend {
                    cache_ttl_secs: 300,
                },
                1,
                None,
            ),
        ];

        for (name, mode, expected_requests, expected_ua) in cases {
            let count = Arc::new(AtomicUsize::new(0));
            let last_ua = Arc::new(tokio::sync::Mutex::new(String::new()));
            let (c, ua) = (count.clone(), last_ua.clone());

            let app = axum::Router::new().route(
                "/network/json",
                axum::routing::get(move |headers: axum::http::HeaderMap| {
                    let (c, ua) = (c.clone(), ua.clone());
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        *ua.lock().await = headers
                            .get("user-agent")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_owned();
                        axum::Json(serde_json::json!({
                            "asn": 15169,
                            "org": "Google LLC",
                            "type": "cloud"
                        }))
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                axum::serve(listener, app).await.ok();
            });

            let client = EnrichmentClient::new(
                &format!("http://{addr}"),
                Duration::from_secs(5),
                "spectra",
                None,
                mode,
            );
            let ip: IpAddr = "8.8.8.8".parse().unwrap();
            assert!(
                client.lookup(ip, None).await.is_some(),
                "{name}: first lookup"
            );
            assert!(
                client.lookup(ip, None).await.is_some(),
                "{name}: second lookup"
            );

            assert_eq!(
                count.load(Ordering::SeqCst),
                expected_requests,
                "{name}: request count for two lookups of the same IP"
            );
            if let Some(expected) = expected_ua {
                assert_eq!(&*last_ua.lock().await, expected, "{name}: User-Agent");
            }
        }
    }
}
