//! `netray_common::fetch::fetch_traced` returns the hops followed before any outcome, so a
//! mid-chain network error keeps the redirects already followed
//! (`specs/features/outbound-fetch/plan.md`, "Fixes after the Phase 3 review").

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;

use netray_common::fetch::{AtLimit, ClientSettings, FetchOptions, Resolve, fetch_traced};
use reqwest::{Method, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct StubResolver(HashMap<String, Vec<IpAddr>>);

impl Resolve for StubResolver {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let addrs = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { addrs })
    }
}

fn loopback_v4(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

/// Answers every request with `301 Location: <location>`.
fn serve_redirect(listener: TcpListener, location: String) {
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let location = location.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let out = format!(
                    "HTTP/1.1 301 Moved\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = sock.write_all(out.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
}

/// Accepts every connection and closes it without a response.
fn serve_close(listener: TcpListener) {
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            drop(sock);
        }
    });
}

#[tokio::test]
async fn fetch_traced_keeps_hops_on_mid_chain_error() {
    let a = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let b = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (pa, pb) = (a.local_addr().unwrap().port(), b.local_addr().unwrap().port());
    let a_url = format!("http://a.test:{pa}/");
    let b_url = format!("http://b.test:{pb}/");
    serve_redirect(a, b_url.clone());
    serve_close(b);

    let resolver = Arc::new(StubResolver(HashMap::from([
        ("a.test".to_string(), vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
        ("b.test".to_string(), vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]),
    ])));
    let mut opts = FetchOptions::new(Method::GET);
    opts.max_redirects = 10;
    opts.at_limit = AtLimit::ReturnLast;
    opts.allow = loopback_v4;

    let (result, hops) =
        fetch_traced(&ClientSettings::default(), resolver, &a_url, &opts).await;

    assert!(result.is_err(), "B closes the connection: {result:?}");
    assert_eq!(hops.len(), 1, "hops: {hops:?}");
    assert_eq!(hops[0].url.as_str(), a_url);
    assert_eq!(hops[0].status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(hops[0].location, b_url);
}
