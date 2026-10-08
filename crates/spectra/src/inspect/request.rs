use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use netray_common::fetch::{
    self, AtLimit, ClientSettings, FetchError, FetchOptions, Hop, Resolve, SystemResolver,
};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
use reqwest::header::{ACCEPT_ENCODING, HeaderMap, HeaderValue, USER_AGENT};
use url::Url;

use super::TaskResult;
use super::assembler::RedirectHop;

/// Name resolution and address predicate for outbound inspection requests.
#[derive(Clone)]
pub struct Outbound {
    pub resolver: Arc<dyn Resolve>,
    pub allow: fn(IpAddr) -> bool,
}

impl Outbound {
    /// The system resolver with [`is_allowed_target`].
    pub fn system() -> Self {
        Self {
            resolver: Arc::new(SystemResolver),
            allow: is_allowed_target,
        }
    }
}

/// Answers `host` with `ip` and every other name through `inner`.
struct Pinned {
    host: String,
    ip: IpAddr,
    inner: Arc<dyn Resolve>,
}

impl Resolve for Pinned {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        if host.eq_ignore_ascii_case(&self.host) {
            let ip = self.ip;
            Box::pin(async move { vec![ip] })
        } else {
            self.inner.resolve(host)
        }
    }
}

/// Execute a single HTTP request chain, capturing redirects and HTTP versions.
///
/// The initial host name connects to `resolved_addr`'s IP; every other name resolves through
/// `outbound.resolver`, and every address must pass `outbound.allow`.
pub async fn execute_request(
    outbound: &Outbound,
    url: Url,
    resolved_addr: SocketAddr,
    max_redirects: usize,
    timeout: Duration,
    user_agent: &str,
    cors_origin: Option<&str>,
) -> TaskResult {
    let mut default_headers = HeaderMap::new();
    default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, br, zstd"));
    default_headers.insert(USER_AGENT, user_agent.parse().unwrap());
    if let Some(origin) = cors_origin {
        default_headers.insert("origin", origin.parse().unwrap());
    }
    let settings = ClientSettings {
        accept_invalid_certs: true, // Intentional: inspecting sites with broken or self-signed certs is a core feature.
        default_headers,
        ..ClientSettings::default()
    };

    let resolver: Arc<dyn Resolve> = match url.domain() {
        Some(host) => Arc::new(Pinned {
            host: host.to_owned(),
            ip: resolved_addr.ip(),
            inner: Arc::clone(&outbound.resolver),
        }),
        None => Arc::clone(&outbound.resolver),
    };

    let opts = FetchOptions {
        max_redirects,
        at_limit: AtLimit::ReturnLast,
        read_body: false,
        timeout,
        allow: outbound.allow,
        ..FetchOptions::new(Method::GET)
    };

    let (result, hops) = fetch::fetch_traced(&settings, resolver, url.as_str(), &opts).await;
    match result {
        Ok(response) => TaskResult {
            final_url: response.url.to_string(),
            status: response.status.as_u16(),
            http_version: format_http_version(response.version),
            headers: response.headers,
            redirects: redirect_hops(response.hops),
            redirect_limit_reached: response.limit_reached,
            error: None,
        },
        Err(FetchError::Blocked { mut hops, hop, .. }) => {
            if hop >= 1 {
                hops.pop();
            }
            failed(
                &url,
                redirect_hops(hops),
                "Redirect destination blocked".to_string(),
            )
        }
        Err(e) => failed(&url, redirect_hops(hops), format!("Request failed: {e}")),
    }
}

fn failed(url: &Url, redirects: Vec<RedirectHop>, error: String) -> TaskResult {
    TaskResult {
        final_url: url.to_string(),
        status: 0,
        http_version: String::new(),
        headers: HeaderMap::new(),
        redirects,
        redirect_limit_reached: false,
        error: Some(error),
    }
}

fn redirect_hops(hops: Vec<Hop>) -> Vec<RedirectHop> {
    hops.into_iter()
        .map(|hop| RedirectHop {
            url: hop.url.to_string(),
            status: hop.status.as_u16(),
            location: Some(hop.location),
            // TODO: the redirect policy does not expose per-hop response version
            http_version: String::new(),
        })
        .collect()
}

fn format_http_version(version: reqwest::Version) -> String {
    match version {
        reqwest::Version::HTTP_09 => "h0.9".to_string(),
        reqwest::Version::HTTP_10 => "h1.0".to_string(),
        reqwest::Version::HTTP_11 => "h1.1".to_string(),
        reqwest::Version::HTTP_2 => "h2".to_string(),
        reqwest::Version::HTTP_3 => "h3".to_string(),
        _ => format!("{version:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Test seam: empty stub resolver, allow admits only 127.0.0.1.
    struct NoNames;

    impl netray_common::fetch::Resolve for NoNames {
        fn resolve(
            &self,
            _host: &str,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<IpAddr>> + Send>> {
            Box::pin(async { Vec::new() })
        }
    }

    fn test_outbound() -> Outbound {
        Outbound {
            resolver: Arc::new(NoNames),
            allow: |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST),
        }
    }

    #[tokio::test]
    async fn redirect_hops_are_captured() {
        // Bind a listener on an ephemeral port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a minimal HTTP server that responds with 301 -> example.com
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                // Drain the request
                let mut buf = [0u8; 1024];
                let _ =
                    tokio::time::timeout(Duration::from_millis(200), stream.read(&mut buf)).await;
                let response = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://example.com/\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());

        // max_redirects=0 so reqwest stops after the first 301 without following it
        let outbound = test_outbound();
        let result = execute_request(
            &outbound,
            url,
            resolved,
            0, // stop immediately — captures the hop
            Duration::from_secs(2),
            "test-agent",
            None,
        )
        .await;

        assert!(
            !result.redirects.is_empty() || result.redirect_limit_reached,
            "expected at least one redirect hop or limit reached, got {:?}",
            result.redirects
        );
    }
}
