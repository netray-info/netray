//! An environment proxy must not take the request: the helper builds a client without any
//! proxy. Its own test binary, so the environment change cannot race other tests.

use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use netray_common::fetch::{ClientSettings, FetchOptions, Resolve, fetch};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct StubResolver(HashMap<String, Vec<IpAddr>>);

impl Resolve for StubResolver {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let addrs = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { addrs })
    }
}

fn allow_loopback(ip: IpAddr) -> bool {
    ip.is_loopback() || is_allowed_target(ip)
}

/// Counts connections on accept, answers each with a 200.
async fn counting_listener(body: &'static str) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            c.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let out = format!(
                    "HTTP/1.1 200 X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(out.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (port, count)
}

#[test]
fn environment_proxy_is_ignored() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let (proxy_port, proxy_count) = rt.block_on(counting_listener("proxy"));
    let proxy = format!("http://127.0.0.1:{proxy_port}");
    // SAFETY: no other thread reads the environment yet; this binary holds one test.
    unsafe {
        std::env::set_var("HTTP_PROXY", &proxy);
        std::env::set_var("http_proxy", &proxy);
    }

    rt.block_on(async {
        let (port, real_count) = counting_listener("real").await;
        let resolver = StubResolver(HashMap::from([(
            "pinned.invalid".to_string(),
            vec![IpAddr::from([127, 0, 0, 1])],
        )]));
        let mut o = FetchOptions::new(Method::GET);
        o.allow = allow_loopback;
        let url = format!("http://pinned.invalid:{port}/");
        let res = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o)
            .await
            .expect("fetch reaches the real listener");

        assert_eq!(res.status.as_u16(), 200);
        assert_eq!(&res.body[..], b"real");
        assert_eq!(real_count.load(Ordering::SeqCst), 1);
        assert_eq!(proxy_count.load(Ordering::SeqCst), 0);
    });
}
