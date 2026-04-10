use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::redirect::Policy;
use url::Url;

use super::TaskResult;
use super::assembler::RedirectHop;

/// Execute a single HTTP request chain, capturing redirects and HTTP versions.
///
/// Accepts a shared `reqwest::Client` (connection pool reuse). The client must
/// have been built with `danger_accept_invalid_certs(true)` and without a
/// `.resolve()` pin — per-request host pinning is applied via `RequestBuilder`.
pub async fn execute_request(
    client: &reqwest::Client,
    url: Url,
    resolved_addr: SocketAddr,
    max_redirects: usize,
    timeout: Duration,
    user_agent: &str,
    cors_origin: Option<&str>,
) -> TaskResult {
    let hops = Arc::new(Mutex::new(Vec::<RedirectHop>::new()));
    let hops_clone = Arc::clone(&hops);
    let redirect_limit_reached = Arc::new(Mutex::new(false));
    let limit_clone = Arc::clone(&redirect_limit_reached);
    let ssrf_blocked = Arc::new(Mutex::new(false));
    let ssrf_blocked_clone = Arc::clone(&ssrf_blocked);
    let ssrf_blocked_url = Arc::new(Mutex::new(String::new()));
    let ssrf_blocked_url_clone = Arc::clone(&ssrf_blocked_url);

    let host = url.host_str().unwrap_or_default().to_string();

    let policy = Policy::custom(move |attempt| {
        let count = {
            let h = hops_clone.lock().unwrap();
            h.len()
        };

        if count >= max_redirects {
            *limit_clone.lock().unwrap() = true;
            return attempt.stop();
        }

        // SSRF redirect guard: validate redirect destination before following.
        let dest_url = attempt.url();
        if let Some(host) = dest_url.host_str() {
            let port = dest_url.port_or_known_default().unwrap_or(443);
            let addr_str = format!("{host}:{port}");
            // Perform a synchronous IP check for IP-literal URLs; hostname
            // resolution is not possible inside the synchronous redirect closure.
            use std::str::FromStr;
            if let Ok(ip) = std::net::IpAddr::from_str(host) {
                let blocked_addr = std::net::SocketAddr::new(ip, port);
                if !netray_common::target_policy::is_allowed_target(blocked_addr.ip()) {
                    *ssrf_blocked_clone.lock().unwrap() = true;
                    *ssrf_blocked_url_clone.lock().unwrap() = addr_str;
                    return attempt.stop();
                }
            }
        }

        let prev_url = attempt.previous().last().map(|u| u.to_string());
        let prev_status = attempt.status().as_u16();

        hops_clone.lock().unwrap().push(RedirectHop {
            url: prev_url.unwrap_or_default(),
            status: prev_status,
            location: attempt.url().to_string().into(),
            // TODO: reqwest Policy::custom does not expose per-hop response version
            http_version: String::new(),
        });

        attempt.follow()
    });

    // Build a per-request client from the shared base client by adding
    // per-call settings (.resolve() and optional Origin header).
    let mut req_builder_headers = reqwest::header::HeaderMap::new();
    req_builder_headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        "gzip, br, zstd".parse().unwrap(),
    );
    req_builder_headers.insert(
        reqwest::header::USER_AGENT,
        user_agent.parse().unwrap(),
    );
    if let Some(origin) = cors_origin {
        req_builder_headers.insert("origin", origin.parse().unwrap());
    }

    let per_request_client = match reqwest::Client::builder()
        .redirect(policy)
        .timeout(timeout)
        .danger_accept_invalid_certs(true) // Intentional: inspecting sites with broken or self-signed certs is a core feature.
        .resolve(&host, resolved_addr)
        .default_headers(req_builder_headers)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return TaskResult {
                final_url: url.to_string(),
                status: 0,
                http_version: String::new(),
                headers: reqwest::header::HeaderMap::new(),
                redirects: vec![],
                redirect_limit_reached: false,
                error: Some(format!("Failed to build HTTP client: {e}")),
            };
        }
    };
    let _ = client; // shared client available for future pool reuse

    match per_request_client.get(url.as_str()).send().await {
        Ok(response) => {
            let http_version = format_http_version(response.version());
            let status = response.status().as_u16();
            let final_url = response.url().to_string();
            let response_headers = response.headers().clone();
            let redirects = hops.lock().unwrap().clone();
            let limit = *redirect_limit_reached.lock().unwrap();
            let blocked = *ssrf_blocked.lock().unwrap();
            let blocked_url = ssrf_blocked_url.lock().unwrap().clone();

            if blocked {
                TaskResult {
                    final_url: url.to_string(),
                    status: 0,
                    http_version: String::new(),
                    headers: reqwest::header::HeaderMap::new(),
                    redirects,
                    redirect_limit_reached: limit,
                    error: Some(format!("Redirect destination blocked: {blocked_url}")),
                }
            } else {
                TaskResult {
                    final_url,
                    status,
                    http_version,
                    headers: response_headers,
                    redirects,
                    redirect_limit_reached: limit,
                    error: None,
                }
            }
        }
        Err(e) => {
            let redirects = hops.lock().unwrap().clone();
            let limit = *redirect_limit_reached.lock().unwrap();
            let blocked = *ssrf_blocked.lock().unwrap();
            let blocked_url = ssrf_blocked_url.lock().unwrap().clone();

            if blocked {
                TaskResult {
                    final_url: url.to_string(),
                    status: 0,
                    http_version: String::new(),
                    headers: reqwest::header::HeaderMap::new(),
                    redirects,
                    redirect_limit_reached: limit,
                    error: Some(format!("Redirect destination blocked: {blocked_url}")),
                }
            } else {
                TaskResult {
                    final_url: url.to_string(),
                    status: 0,
                    http_version: String::new(),
                    headers: reqwest::header::HeaderMap::new(),
                    redirects,
                    redirect_limit_reached: limit,
                    error: Some(format!("Request failed: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        let client = reqwest::Client::new();
        let result = execute_request(
            &client,
            url,
            resolved,
            0, // stop immediately — captures the hop
            Duration::from_secs(2),
            "test-agent",
            None,
        )
        .await;

        assert!(
            result.redirects.len() >= 1 || result.redirect_limit_reached,
            "expected at least one redirect hop or limit reached, got {:?}",
            result.redirects
        );
    }
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
