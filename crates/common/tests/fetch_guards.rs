//! Guards of `netray_common::fetch` fixed by "Fixes after the redesign review" in
//! `specs/features/outbound-fetch/plan.md`: IP-literal redirect targets, `https_only` on a
//! redirect hop, and the initial URL's fragment.
//!
//! Every refused case asserts the typed error AND a zero connection count on the listener.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use netray_common::fetch::{ClientSettings, FetchError, FetchOptions, Resolve, fetch};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const V4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

struct StubResolver(HashMap<String, Vec<IpAddr>>);

impl StubResolver {
    fn new(entries: &[(&str, Vec<IpAddr>)]) -> Self {
        Self(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }
}

impl Resolve for StubResolver {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let addrs = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { addrs })
    }
}

fn allow_loopback(ip: IpAddr) -> bool {
    ip.is_loopback() || is_allowed_target(ip)
}

fn only_v4_loopback(ip: IpAddr) -> bool {
    ip == V4
}

fn response(status: u16, extra_headers: &[(&str, &str)], body: &str) -> String {
    let mut out = format!("HTTP/1.1 {status} X\r\n");
    for (k, v) in extra_headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    out
}

fn redirect(location: &str) -> String {
    response(302, &[("Location", location)], "")
}

struct Server {
    port: u16,
    count: Arc<AtomicUsize>,
}

impl Server {
    fn connections(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

/// Counts connections on accept, answers each with `handler(request path)`.
fn serve(
    listener: TcpListener,
    handler: impl Fn(&str) -> String + Send + Sync + 'static,
) -> Server {
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let handler = Arc::new(handler);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            c.fetch_add(1, Ordering::SeqCst);
            let handler = handler.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("")
                    .to_string();
                let _ = sock.write_all(handler(&path).as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Server { port, count }
}

/// A TLS listener on 127.0.0.1 with a self-signed certificate (as in
/// `crates/email/src/checks/mta_sts_results_table.rs`), counting connections on accept.
async fn serve_tls(
    names: Vec<String>,
    handler: impl Fn(&str) -> String + Send + Sync + 'static,
) -> Server {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(names).unwrap();
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert.der().to_vec())],
            key_der,
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let handler = Arc::new(handler);
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            c.fetch_add(1, Ordering::SeqCst);
            let (acceptor, handler) = (acceptor.clone(), handler.clone());
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&buf).to_string();
                let path = head
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("")
                    .to_string();
                let _ = tls.write_all(handler(&path).as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    Server { port, count }
}

#[tokio::test]
async fn redirect_to_refused_ip_literal_is_blocked_at_hop_one() {
    let v6 = TcpListener::bind("[::1]:0").await.expect("bind [::1]");
    let target = serve(v6, |_| response(200, &[], "target"));
    let location = format!("http://[::1]:{}/", target.port);
    let start = serve(TcpListener::bind("127.0.0.1:0").await.unwrap(), move |_| {
        redirect(&location)
    });

    let resolver = StubResolver::new(&[("start.invalid", vec![V4])]);
    let mut o = FetchOptions::new(Method::GET);
    o.allow = only_v4_loopback;
    o.max_redirects = 1;
    let url = format!("http://start.invalid:{}/", start.port);
    let result = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o).await;

    match result {
        Err(FetchError::Blocked { hop, .. }) => assert_eq!(hop, 1),
        other => panic!("expected Blocked at hop 1, got {other:?}"),
    }
    assert_eq!(start.connections(), 1);
    assert_eq!(target.connections(), 0);
}

#[tokio::test]
async fn https_only_refuses_an_https_to_http_redirect_hop() {
    let plain = serve(TcpListener::bind("127.0.0.1:0").await.unwrap(), |_| {
        response(200, &[], "plain")
    });
    let location = format!("http://localhost:{}/", plain.port);
    let tls = serve_tls(vec!["start.invalid".to_string()], move |_| {
        redirect(&location)
    })
    .await;

    let resolver = StubResolver::new(&[("start.invalid", vec![V4]), ("localhost", vec![V4])]);
    let settings = ClientSettings {
        accept_invalid_certs: true,
        ..ClientSettings::default()
    };
    let mut o = FetchOptions::new(Method::GET);
    o.allow = allow_loopback;
    o.https_only = true;
    o.max_redirects = 1;
    let url = format!("https://start.invalid:{}/", tls.port);
    let result = fetch(&settings, Arc::new(resolver), &url, &o).await;

    assert!(matches!(result, Err(FetchError::Scheme(_))), "{result:?}");
    assert_eq!(
        tls.connections(),
        1,
        "the TLS first hop must have been reached"
    );
    assert_eq!(plain.connections(), 0);
}

#[tokio::test]
async fn initial_url_fragment_is_not_recorded_in_the_first_hop() {
    let srv = serve(TcpListener::bind("127.0.0.1:0").await.unwrap(), |path| {
        if path == "/b" {
            response(200, &[], "end")
        } else {
            redirect("/b")
        }
    });

    let resolver = StubResolver::new(&[("start.invalid", vec![V4])]);
    let mut o = FetchOptions::new(Method::GET);
    o.allow = allow_loopback;
    o.max_redirects = 1;
    let url = format!("http://start.invalid:{}/#top", srv.port);
    let res = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o)
        .await
        .expect("fetch succeeds");

    assert_eq!(res.hops.len(), 1);
    assert_eq!(res.hops[0].url.fragment(), None, "{}", res.hops[0].url);
    assert!(!res.hops[0].url.as_str().contains('#'));
}
