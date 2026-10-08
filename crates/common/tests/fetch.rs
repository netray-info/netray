//! Integration tests for `netray_common::fetch`.
//!
//! Every refused case asserts the typed error AND a zero connection count on the listener:
//! a request without any listener would also count zero, so the error proves the guard.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use netray_common::fetch::{AtLimit, ClientSettings, FetchError, FetchOptions, Resolve, fetch};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
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
    method: String,
    path: String,
    body: String,
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

fn redirect(status: u16, location: &str) -> String {
    response(status, &[("Location", location)], "")
}

async fn bind(addr: &str) -> TcpListener {
    TcpListener::bind(addr).await.expect("bind listener")
}

async fn bind4() -> TcpListener {
    bind("127.0.0.1:0").await
}

async fn bind6() -> TcpListener {
    bind("[::1]:0").await
}

/// Accepts connections, counts them on accept, answers each with `handler(request)`.
fn serve(
    listener: TcpListener,
    handler: impl Fn(&Req) -> String + Send + Sync + 'static,
) -> Server {
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
                let len = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        if k.eq_ignore_ascii_case("content-length") {
                            v.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                while buf.len() < head_end + len {
                    match sock.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let mut first = head.lines().next().unwrap_or("").split(' ');
                let req = Req {
                    method: first.next().unwrap_or("").to_string(),
                    path: first.next().unwrap_or("").to_string(),
                    body: String::from_utf8_lossy(&buf[head_end..]).to_string(),
                };
                r.lock().unwrap().push(req.clone());
                let _ = sock.write_all(handler(&req).as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    Server {
        port,
        count,
        requests,
    }
}

/// A server that answers every request with `200` and `body`.
fn ok_server(listener: TcpListener, body: &'static str) -> Server {
    serve(listener, move |_| response(200, &[], body))
}

/// A server on 127.0.0.1 answering `/0../{redirects-1}` with `302` to the next path, and
/// `/{redirects}` with `200`.
async fn chain_server(redirects: usize) -> Server {
    let listener = bind4().await;
    let port = listener.local_addr().unwrap().port();
    serve(listener, move |req| {
        let n: usize = req
            .path
            .trim_start_matches('/')
            .parse()
            .unwrap_or(usize::MAX);
        if n < redirects {
            redirect(302, &format!("http://127.0.0.1:{port}/{}", n + 1))
        } else {
            response(200, &[], "end")
        }
    })
}

fn allow_loopback(ip: IpAddr) -> bool {
    ip.is_loopback() || is_allowed_target(ip)
}

fn only_v4_loopback(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

const V4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const V6: IpAddr = IpAddr::V6(Ipv6Addr::LOCALHOST);

fn opts(method: Method) -> FetchOptions {
    FetchOptions::new(method)
}

fn loopback_opts(method: Method) -> FetchOptions {
    let mut o = FetchOptions::new(method);
    o.allow = allow_loopback;
    o
}

// ---------------------------------------------------------------- refused targets

#[tokio::test]
async fn loopback_ipv4_literal_is_refused() {
    let srv = ok_server(bind4().await, "x");
    let url = format!("https://127.0.0.1:{}/", srv.port);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(StubResolver::new(&[])),
        &url,
        &opts(Method::GET),
    )
    .await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(srv.connections(), 0);
}

#[tokio::test]
async fn userinfo_does_not_hide_the_host() {
    let srv = ok_server(bind4().await, "x");
    let url = format!("https://x@127.0.0.1:{}/", srv.port);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(StubResolver::new(&[])),
        &url,
        &opts(Method::GET),
    )
    .await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(srv.connections(), 0);
}

#[tokio::test]
async fn loopback_ipv6_literal_is_refused() {
    let srv = ok_server(bind6().await, "x");
    let url = format!("https://[::1]:{}/", srv.port);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(StubResolver::new(&[])),
        &url,
        &opts(Method::GET),
    )
    .await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(srv.connections(), 0);
}

#[tokio::test]
async fn empty_resolution_is_refused() {
    let resolver = StubResolver::new(&[("empty.invalid", vec![])]);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(resolver),
        "http://empty.invalid/",
        &opts(Method::GET),
    )
    .await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn one_disallowed_address_in_the_set_refuses_the_whole_set() {
    let srv = ok_server(bind4().await, "x");
    let public: IpAddr = "93.184.216.34".parse().unwrap();
    let resolver = StubResolver::new(&[("mixed.example.com", vec![public, V4])]);
    let url = format!("http://mixed.example.com:{}/", srv.port);
    let result = fetch(&ClientSettings::default(),Arc::new(resolver), &url, &opts(Method::GET)).await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(srv.connections(), 0);
}

#[tokio::test]
async fn https_only_refuses_plain_http() {
    let srv = ok_server(bind4().await, "x");
    let mut o = loopback_opts(Method::GET);
    o.https_only = true;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let result = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o).await;
    assert!(matches!(result, Err(FetchError::Scheme(_))), "{result:?}");
    assert_eq!(srv.connections(), 0);
}

// ---------------------------------------------------------------- pinning

#[tokio::test]
async fn connection_goes_to_the_checked_address() {
    // `.invalid` cannot resolve through the system, so success proves the pin.
    let srv = ok_server(bind4().await, "pinned");
    let resolver = StubResolver::new(&[("pinned.invalid", vec![V4])]);
    let url = format!("http://pinned.invalid:{}/", srv.port);
    let res = fetch(&ClientSettings::default(),Arc::new(resolver), &url, &loopback_opts(Method::GET))
        .await
        .expect("fetch through the pin");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(srv.connections(), 1);
}

// ---------------------------------------------------------------- redirect re-check

#[tokio::test]
async fn redirect_target_is_checked_again() {
    let second = ok_server(bind6().await, "x");
    let loc = format!("http://svc.invalid:{}/", second.port);
    let first = serve(bind4().await, move |_| redirect(302, &loc));
    let resolver = StubResolver::new(&[("start.invalid", vec![V4]), ("svc.invalid", vec![V6])]);
    let mut o = FetchOptions::new(Method::GET);
    o.allow = only_v4_loopback;
    o.max_redirects = 3;
    let url = format!("http://start.invalid:{}/", first.port);
    let result = fetch(&ClientSettings::default(),Arc::new(resolver), &url, &o).await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(first.connections(), 1);
    assert_eq!(second.connections(), 0);
}

#[tokio::test]
async fn redirect_to_localhost_name_is_checked_again() {
    let second = ok_server(bind6().await, "x");
    let loc = format!("http://localhost:{}/", second.port);
    let first = serve(bind4().await, move |_| redirect(302, &loc));
    let resolver = StubResolver::new(&[("start.invalid", vec![V4]), ("localhost", vec![V6])]);
    let mut o = FetchOptions::new(Method::GET);
    o.allow = only_v4_loopback;
    o.max_redirects = 3;
    let url = format!("http://start.invalid:{}/", first.port);
    let result = fetch(&ClientSettings::default(),Arc::new(resolver), &url, &o).await;
    assert!(
        matches!(result, Err(FetchError::Blocked { .. })),
        "{result:?}"
    );
    assert_eq!(first.connections(), 1);
    assert_eq!(second.connections(), 0);
}

// ---------------------------------------------------------------- redirect limit

#[tokio::test]
async fn chain_at_the_limit_is_followed() {
    let srv = chain_server(4).await;
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 4;
    o.at_limit = AtLimit::Fail;
    let url = format!("http://127.0.0.1:{}/0", srv.port);
    let res = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o)
        .await
        .expect("4 redirects within a limit of 4");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(res.hops.len(), 4);
    assert!(!res.limit_reached);
}

#[tokio::test]
async fn chain_beyond_the_limit_fails() {
    let srv = chain_server(5).await;
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 4;
    o.at_limit = AtLimit::Fail;
    let url = format!("http://127.0.0.1:{}/0", srv.port);
    let result = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o).await;
    assert!(
        matches!(result, Err(FetchError::TooManyRedirects)),
        "{result:?}"
    );
}

#[tokio::test]
async fn return_last_hands_back_the_unfollowed_redirect() {
    // Chain: /0 -> /1 -> /2 -> /3 (200). With max_redirects = 2 the loop follows /0 and /1
    // (2 hops recorded), then receives the 302 of /2, which it does not follow: that 302 is
    // returned with limit_reached set. Hops list followed redirects only, so len() == 2.
    let srv = chain_server(3).await;
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 2;
    o.at_limit = AtLimit::ReturnLast;
    let url = format!("http://127.0.0.1:{}/0", srv.port);
    let res = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o)
        .await
        .expect("ReturnLast does not fail at the limit");
    assert_eq!(res.status.as_u16(), 302);
    assert!(res.limit_reached);
    assert_eq!(res.hops.len(), 2);
    assert_eq!(res.url.path(), "/2");
    assert_eq!(srv.requests().len(), 3, "the 3xx of /2 was not followed");
}

// ---------------------------------------------------------------- hop recording

#[tokio::test]
async fn hops_record_url_status_and_location() {
    let listener = bind4().await;
    let p = listener.local_addr().unwrap().port();
    let srv = serve(listener, move |req| match req.path.as_str() {
        "/a" => redirect(301, &format!("http://127.0.0.1:{p}/b")),
        "/b" => redirect(302, &format!("http://127.0.0.1:{p}/c")),
        _ => response(200, &[], "end"),
    });
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 5;
    let url = format!("http://127.0.0.1:{}/a", srv.port);
    let res = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o)
        .await
        .expect("allowed chain");
    let hops: Vec<(String, u16, String)> = res
        .hops
        .iter()
        .map(|h| (h.url.to_string(), h.status.as_u16(), h.location.clone()))
        .collect();
    assert_eq!(
        hops,
        vec![
            (
                format!("http://127.0.0.1:{p}/a"),
                301,
                format!("http://127.0.0.1:{p}/b")
            ),
            (
                format!("http://127.0.0.1:{p}/b"),
                302,
                format!("http://127.0.0.1:{p}/c")
            ),
        ]
    );
    assert_eq!(res.status.as_u16(), 200);
}

// ---------------------------------------------------------------- redirect method semantics

async fn post_through_redirect(status: u16) -> Req {
    let second = ok_server(bind4().await, "done");
    let loc = format!("http://127.0.0.1:{}/target", second.port);
    let first = serve(bind4().await, move |_| redirect(status, &loc));
    let mut o = loopback_opts(Method::POST);
    o.body = Some(bytes::Bytes::from_static(b"payload"));
    o.max_redirects = 2;
    let url = format!("http://127.0.0.1:{}/", first.port);
    let res = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o)
        .await
        .expect("redirected POST");
    assert_eq!(res.status.as_u16(), 200);
    let seen = second.requests();
    assert_eq!(seen.len(), 1);
    seen[0].clone()
}

#[tokio::test]
async fn moved_permanently_turns_post_into_bodyless_get() {
    let req = post_through_redirect(301).await;
    assert_eq!(req.method, "GET");
    assert_eq!(req.body, "");
}

#[tokio::test]
async fn temporary_redirect_keeps_method_and_body() {
    let req = post_through_redirect(307).await;
    assert_eq!(req.method, "POST");
    assert_eq!(req.body, "payload");
}

// ---------------------------------------------------------------- body cap

#[tokio::test]
async fn body_over_the_cap_is_an_error() {
    let srv = ok_server(bind4().await, "01234567890123456"); // 17 bytes
    let mut o = loopback_opts(Method::GET);
    o.body_cap = 16;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let result = fetch(&ClientSettings::default(),Arc::new(StubResolver::new(&[])), &url, &o).await;
    assert!(
        matches!(result, Err(FetchError::BodyTooLarge)),
        "{result:?}"
    );
}
