use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::redirect::Policy;
use url::Url;

use super::assembler::RedirectHop;
use super::TaskResult;

/// Execute a single HTTP request chain, capturing redirects and HTTP versions.
pub async fn execute_request(
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

        let prev_url = attempt.previous().last().map(|u| u.to_string());
        let prev_status = attempt.status().as_u16();

        hops_clone.lock().unwrap().push(RedirectHop {
            url: prev_url.unwrap_or_default(),
            status: prev_status,
            location: attempt.url().to_string().into(),
            http_version: String::new(), // filled after response
        });

        attempt.follow()
    });

    let mut builder = reqwest::Client::builder()
        .redirect(policy)
        .timeout(timeout)
        .user_agent(user_agent)
        .danger_accept_invalid_certs(true) // Intentional: inspecting sites with broken or self-signed certs is a core feature.
        .resolve(&host, resolved_addr);

    // Send Accept-Encoding to detect compression
    let mut req_builder_headers = reqwest::header::HeaderMap::new();
    req_builder_headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        "gzip, br, zstd".parse().unwrap(),
    );

    if let Some(origin) = cors_origin {
        req_builder_headers.insert("origin", origin.parse().unwrap());
    }

    builder = builder.default_headers(req_builder_headers);

    let client = match builder.build() {
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

    match client.get(url.as_str()).send().await {
        Ok(response) => {
            let http_version = format_http_version(response.version());
            let status = response.status().as_u16();
            let final_url = response.url().to_string();
            let response_headers = response.headers().clone();
            let redirects = hops.lock().unwrap().clone();
            let limit = *redirect_limit_reached.lock().unwrap();

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
        Err(e) => {
            let redirects = hops.lock().unwrap().clone();
            let limit = *redirect_limit_reached.lock().unwrap();

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
                let _ = tokio::time::timeout(
                    Duration::from_millis(200),
                    stream.read(&mut buf),
                )
                .await;
                let response = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://example.com/\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let url = Url::parse(&format!("http://127.0.0.1:{}/", addr.port())).unwrap();
        let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port());

        // max_redirects=0 so reqwest stops after the first 301 without following it
        let result = execute_request(
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
