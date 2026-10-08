//! Behaviour that comes from reqwest's own redirect handling (plan.md, "Redesign after the
//! second pass"): a stalled 3xx body, Referer, cross-host header stripping, Location fragments.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use netray_common::fetch::{AtLimit, ClientSettings, FetchOptions, Resolve, fetch};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------- infrastructure

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

#[derive(Debug, Clone)]
struct Req {
    path: String,
    /// Header names lower-cased.
    headers: HashMap<String, String>,
}

/// What a listener does after reading a request.
enum Reply {
    /// Write the bytes and close.
    Full(Vec<u8>),
    /// Write the bytes, then keep the socket open and silent.
    Stall(Vec<u8>),
}

struct Server {
    port: u16,
    count: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Req>>>,
}

impl Server {
    fn connections(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn requests(&self) -> Vec<Req> {
        self.requests.lock().unwrap().clone()
    }
}

fn response(status: u16, extra_headers: &[(&str, &str)], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} X\r\n");
    for (k, v) in extra_headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));
    out.into_bytes()
}

fn redirect(status: u16, location: &str) -> Vec<u8> {
    response(status, &[("Location", location)], "")
}

async fn bind4() -> TcpListener {
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener")
}

/// Accepts connections, counts them on accept, records each request and answers with `handler`.
fn serve(listener: TcpListener, handler: impl Fn(&Req) -> Reply + Send + Sync + 'static) -> Server {
    let port = listener.local_addr().unwrap().port();
    let count = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (c, r) = (count.clone(), requests.clone());
    let handler = Arc::new(handler);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            c.fetch_add(1, Ordering::SeqCst);
            let (r, handler) = (r.clone(), handler.clone());
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                let head_end = loop {
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let mut lines = head.lines();
                let path = lines
                    .next()
                    .unwrap_or("")
                    .split(' ')
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                let headers = lines
                    .filter_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    })
                    .collect();
                let req = Req { path, headers };
                r.lock().unwrap().push(req.clone());
                match handler(&req) {
                    Reply::Full(bytes) => {
                        let _ = sock.write_all(&bytes).await;
                        let _ = sock.shutdown().await;
                    }
                    Reply::Stall(bytes) => {
                        let _ = sock.write_all(&bytes).await;
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                }
            });
        }
    });
    Server {
        port,
        count,
        requests,
    }
}

fn allow_loopback(ip: IpAddr) -> bool {
    ip.is_loopback() || is_allowed_target(ip)
}

const V4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

fn loopback_opts() -> FetchOptions {
    let mut o = FetchOptions::new(Method::GET);
    o.allow = allow_loopback;
    // The default follows no redirect; these cases follow one.
    o.max_redirects = 1;
    o
}

fn no_resolver() -> Arc<StubResolver> {
    Arc::new(StubResolver::new(&[]))
}

// ---------------------------------------------------------------- cases

#[tokio::test]
async fn final_3xx_with_stalled_body_is_returned_at_once() {
    let srv = serve(bind4().await, |_| {
        Reply::Stall(b"HTTP/1.1 301 X\r\nLocation: /x\r\nContent-Length: 100\r\n\r\n".to_vec())
    });
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let mut o = loopback_opts();
    o.max_redirects = 0;
    o.at_limit = AtLimit::ReturnLast;
    o.read_body = true;
    o.timeout = Duration::from_secs(2);
    let started = Instant::now();
    let res = fetch(&ClientSettings::default(), no_resolver(), &url, &o)
        .await
        .expect("a final 3xx is returned, not an error");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(res.status.as_u16(), 301);
    assert!(res.limit_reached);
}

#[tokio::test]
async fn referer_is_sent_on_a_followed_hop() {
    let b = serve(bind4().await, |_| Reply::Full(response(200, &[], "b")));
    let loc = format!("http://127.0.0.1:{}/b", b.port);
    let a = serve(bind4().await, move |_| Reply::Full(redirect(302, &loc)));
    let url_a = format!("http://127.0.0.1:{}/a", a.port);
    fetch(
        &ClientSettings::default(),
        no_resolver(),
        &url_a,
        &loopback_opts(),
    )
    .await
    .expect("follow the redirect");
    let seen = b.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].headers.get("referer"), Some(&url_a));
}

#[tokio::test]
async fn cross_host_redirect_drops_authorization() {
    let p2 = serve(bind4().await, |_| Reply::Full(response(200, &[], "b")));
    let loc = format!("http://localhost:{}/b", p2.port);
    let p1 = serve(bind4().await, move |_| Reply::Full(redirect(302, &loc)));
    let resolver = StubResolver::new(&[("a.invalid", vec![V4]), ("localhost", vec![V4])]);
    let mut o = loopback_opts();
    o.headers
        .insert(AUTHORIZATION, HeaderValue::from_static("Bearer x"));
    let url = format!("http://a.invalid:{}/", p1.port);
    fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o)
        .await
        .expect("follow the cross-host redirect");
    let first = p1.requests();
    assert_eq!(first.len(), 1);
    assert_eq!(
        first[0].headers.get("authorization").map(String::as_str),
        Some("Bearer x"),
        "the header must go out on the first hop"
    );
    assert_eq!(p2.connections(), 1);
    let second = p2.requests();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].headers.get("authorization"), None);
}

#[tokio::test]
async fn location_fragment_is_not_recorded() {
    let srv = serve(bind4().await, |req| {
        if req.path == "/b" {
            Reply::Full(response(200, &[], "b"))
        } else {
            Reply::Full(redirect(302, "/b#frag"))
        }
    });
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        no_resolver(),
        &url,
        &loopback_opts(),
    )
    .await
    .expect("follow the redirect");
    assert_eq!(res.hops.len(), 1);
    assert!(
        !res.hops[0].location.contains("#frag"),
        "location: {}",
        res.hops[0].location
    );
    assert_eq!(res.url.fragment(), None);
    assert_eq!(res.url.path(), "/b");
}
