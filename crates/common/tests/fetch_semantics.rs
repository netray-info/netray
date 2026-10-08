//! Semantics of `netray_common::fetch` fixed by the phase-review amendments in
//! `specs/features/outbound-fetch/plan.md`: Location decoding, body handling, `Blocked`
//! details, the total deadline, redirect method semantics and hop URLs.
//!
//! The listener helpers are copied from `tests/fetch.rs`, which is protected.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use netray_common::fetch::{
    AtLimit, BlockReason, ClientSettings, FetchError, FetchOptions, Resolve, fetch,
};
use netray_common::target_policy::is_allowed_target;
use reqwest::Method;
use reqwest::header::{CONTENT_TYPE, HeaderValue};
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

/// Answers every lookup with `addrs` after `delay`.
struct SlowResolver {
    delay: Duration,
    addrs: Vec<IpAddr>,
}

impl Resolve for SlowResolver {
    fn resolve(&self, _host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let (delay, addrs) = (self.delay, self.addrs.clone());
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            addrs
        })
    }
}

#[derive(Debug, Clone)]
struct Req {
    method: String,
    path: String,
    content_type: Option<String>,
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

/// A redirect whose `Location` value is written to the socket as raw bytes.
fn raw_redirect(status: u16, location: &[u8], body: &str) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status} X\r\nLocation: ").into_bytes();
    out.extend_from_slice(location);
    out.extend_from_slice(
        format!(
            "\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    out
}

async fn bind4() -> TcpListener {
    TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener")
}

/// Accepts connections, counts them on accept, waits `delay`, answers with `handler(request)`.
fn serve_with(
    listener: TcpListener,
    delay: Duration,
    handler: impl Fn(&Req) -> Vec<u8> + Send + Sync + 'static,
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
                let header = |name: &str| {
                    head.lines().find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
                    })
                };
                let len = header("content-length")
                    .and_then(|v| v.parse::<usize>().ok())
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
                    content_type: header("content-type"),
                    body: String::from_utf8_lossy(&buf[head_end..]).to_string(),
                };
                r.lock().unwrap().push(req.clone());
                tokio::time::sleep(delay).await;
                let _ = sock.write_all(&handler(&req)).await;
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

fn serve(
    listener: TcpListener,
    handler: impl Fn(&Req) -> Vec<u8> + Send + Sync + 'static,
) -> Server {
    serve_with(listener, Duration::ZERO, handler)
}

fn allow_loopback(ip: IpAddr) -> bool {
    ip.is_loopback() || is_allowed_target(ip)
}

fn only_v4_loopback(ip: IpAddr) -> bool {
    ip == IpAddr::V4(Ipv4Addr::LOCALHOST)
}

const V4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const V6: IpAddr = IpAddr::V6(Ipv6Addr::LOCALHOST);

fn loopback_opts(method: Method) -> FetchOptions {
    let mut o = FetchOptions::new(method);
    o.allow = allow_loopback;
    o
}

fn no_resolver() -> StubResolver {
    StubResolver::new(&[])
}

// ---------------------------------------------------------------- 1-3: Location handling

#[tokio::test]
async fn raw_utf8_location_is_followed_below_the_limit() {
    let srv = serve(bind4().await, |req| match req.path.as_str() {
        "/start" => raw_redirect(301, b"/caf\xC3\xA9", ""),
        _ => response(200, &[], "end"),
    });
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 2;
    let url = format!("http://127.0.0.1:{}/start", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("raw UTF-8 Location is joined");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(res.url.path(), "/caf%C3%A9");
    assert_eq!(res.hops.len(), 1);
    assert_eq!(srv.requests().last().unwrap().path, "/caf%C3%A9");
}

#[tokio::test]
async fn non_ascii_location_at_limit_zero_is_returned_not_rejected() {
    let srv = serve(bind4().await, |_| raw_redirect(301, b"/caf\xC3\xA9", ""));
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 0;
    o.at_limit = AtLimit::ReturnLast;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("the limit check comes before Location parsing");
    assert_eq!(res.status.as_u16(), 301);
    assert!(res.limit_reached);
    assert!(res.hops.is_empty());
}

#[tokio::test]
async fn unjoinable_location_ends_the_chain_with_that_redirect() {
    let srv = serve(bind4().await, |_| redirect(302, "http://exa mple.com/"));
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 3;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("an unjoinable Location returns the 3xx");
    assert_eq!(res.status.as_u16(), 302);
    assert!(res.hops.is_empty());
    assert_eq!(srv.connections(), 1);
}

// ---------------------------------------------------------------- 4-6: body handling

#[tokio::test]
async fn truncate_body_cuts_at_the_cap() {
    let srv = serve(bind4().await, |_| {
        response(200, &[], "01234567890123456") // 17 bytes
    });
    let mut o = loopback_opts(Method::GET);
    o.body_cap = 16;
    o.truncate_body = true;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("truncation is not an error");
    assert_eq!(res.body.len(), 16);
    assert_eq!(&res.body[..], b"0123456789012345");
    assert!(res.body_truncated);
}

#[tokio::test]
async fn read_body_false_returns_an_empty_body() {
    let big = "x".repeat(1_000_000);
    let srv = serve(bind4().await, move |_| response(200, &[], &big));
    let mut o = loopback_opts(Method::GET);
    o.read_body = false;
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("an unread body is no error");
    assert_eq!(res.status.as_u16(), 200);
    assert!(res.body.is_empty());
    assert!(!res.body_truncated);
}

#[tokio::test]
async fn redirect_body_over_the_cap_is_never_read() {
    let big = "y".repeat(4096);
    let srv = serve(bind4().await, move |req| match req.path.as_str() {
        "/start" => response(302, &[("Location", "/end")], &big),
        _ => response(200, &[], "end"),
    });
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 2;
    o.body_cap = 16;
    let url = format!("http://127.0.0.1:{}/start", srv.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("a 3xx body does not count against the cap");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(res.url.path(), "/end");
    assert_eq!(res.hops.len(), 1);
}

// ---------------------------------------------------------------- 7: Blocked details

#[tokio::test]
async fn blocked_name_without_address_reports_no_address_at_hop_zero() {
    let resolver = StubResolver::new(&[("empty.invalid", vec![])]);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(resolver),
        "http://empty.invalid/",
        &FetchOptions::new(Method::GET),
    )
    .await;
    match result {
        Err(FetchError::Blocked {
            reason, hop, hops, ..
        }) => {
            assert!(matches!(reason, BlockReason::NoAddress), "{reason:?}");
            assert_eq!(hop, 0);
            assert!(hops.is_empty());
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn blocked_initial_address_reports_disallowed_at_hop_zero() {
    let srv = serve(bind4().await, |_| response(200, &[], "x"));
    let url = format!("http://127.0.0.1:{}/", srv.port);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &FetchOptions::new(Method::GET),
    )
    .await;
    match result {
        Err(FetchError::Blocked {
            reason, hop, hops, ..
        }) => {
            assert!(matches!(reason, BlockReason::Disallowed), "{reason:?}");
            assert_eq!(hop, 0);
            assert!(hops.is_empty());
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert_eq!(srv.connections(), 0);
}

#[tokio::test]
async fn blocked_second_redirect_target_reports_hop_two_and_followed_hops() {
    let listener = bind4().await;
    let p = listener.local_addr().unwrap().port();
    let srv = serve(listener, move |req| match req.path.as_str() {
        "/a" => redirect(302, &format!("http://b.invalid:{p}/b")),
        "/b" => redirect(302, &format!("http://c.invalid:{p}/c")),
        _ => response(200, &[], "end"),
    });
    let resolver = StubResolver::new(&[
        ("a.invalid", vec![V4]),
        ("b.invalid", vec![V4]),
        ("c.invalid", vec![V6]),
    ]);
    let mut o = FetchOptions::new(Method::GET);
    o.allow = only_v4_loopback;
    o.max_redirects = 5;
    let url = format!("http://a.invalid:{p}/a");
    let result = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o).await;
    match result {
        Err(FetchError::Blocked {
            reason, hop, hops, ..
        }) => {
            assert!(matches!(reason, BlockReason::Disallowed), "{reason:?}");
            assert_eq!(hop, 2);
            let followed: Vec<(String, u16)> = hops
                .iter()
                .map(|h| (h.url.to_string(), h.status.as_u16()))
                .collect();
            assert_eq!(
                followed,
                vec![
                    (format!("http://a.invalid:{p}/a"), 302),
                    (format!("http://b.invalid:{p}/b"), 302),
                ]
            );
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert_eq!(srv.connections(), 2);
}

// ---------------------------------------------------------------- 8-9: total deadline

/// `/a` answers 302 to `/b` after 200 ms, `/b` answers 200 after 200 ms.
fn slow_chain() -> impl Fn(&Req) -> Vec<u8> + Send + Sync + 'static {
    |req| match req.path.as_str() {
        "/a" => redirect(302, "/b"),
        _ => response(200, &[], "end"),
    }
}

#[tokio::test]
async fn timeout_is_one_deadline_for_the_whole_chain() {
    let srv = serve_with(bind4().await, Duration::from_millis(200), slow_chain());
    let url = format!("http://127.0.0.1:{}/a", srv.port);

    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 2;
    o.timeout = Duration::from_millis(300);
    let result = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await;
    assert!(matches!(result, Err(FetchError::Timeout)), "{result:?}");

    o.timeout = Duration::from_secs(1);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("the same chain fits into 1 s");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(res.hops.len(), 1);
}

#[tokio::test]
async fn timeout_covers_name_resolution() {
    let srv = serve(bind4().await, |_| response(200, &[], "x"));
    let resolver = SlowResolver {
        delay: Duration::from_millis(500),
        addrs: vec![V4],
    };
    let mut o = loopback_opts(Method::GET);
    o.timeout = Duration::from_millis(200);
    let url = format!("http://slow.invalid:{}/", srv.port);
    let start = Instant::now();
    let result = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o).await;
    let elapsed = start.elapsed();
    assert!(matches!(result, Err(FetchError::Timeout)), "{result:?}");
    assert!(elapsed < Duration::from_millis(400), "took {elapsed:?}");
}

// ---------------------------------------------------------------- 10: method semantics

/// Sends `method` (with `body` and `headers`) to a server answering `status`, and returns the
/// request that reached the redirect target.
async fn through_redirect(
    method: Method,
    body: Option<&'static [u8]>,
    content_type: Option<&'static str>,
    status: u16,
) -> Req {
    let second = serve(bind4().await, |_| response(200, &[], ""));
    let loc = format!("http://127.0.0.1:{}/target", second.port);
    let first = serve(bind4().await, move |_| redirect(status, &loc));
    let mut o = loopback_opts(method);
    o.body = body.map(Bytes::from_static);
    if let Some(ct) = content_type {
        o.headers.insert(CONTENT_TYPE, HeaderValue::from_static(ct));
    }
    o.max_redirects = 2;
    let url = format!("http://127.0.0.1:{}/", first.port);
    let res = fetch(
        &ClientSettings::default(),
        Arc::new(no_resolver()),
        &url,
        &o,
    )
    .await
    .expect("redirected request");
    assert_eq!(res.status.as_u16(), 200);
    let seen = second.requests();
    assert_eq!(seen.len(), 1);
    seen[0].clone()
}

#[tokio::test]
async fn redirect_method_and_content_type_follow_reqwest_semantics() {
    // 301 turns only POST into GET: PUT keeps method and body.
    let req = through_redirect(Method::PUT, Some(b"payload"), None, 301).await;
    assert_eq!((req.method.as_str(), req.body.as_str()), ("PUT", "payload"));

    // 302 turns POST into a body-less GET.
    let req = through_redirect(Method::POST, Some(b"payload"), None, 302).await;
    assert_eq!((req.method.as_str(), req.body.as_str()), ("GET", ""));

    // 303 turns DELETE into GET.
    let req = through_redirect(Method::DELETE, None, None, 303).await;
    assert_eq!(req.method, "GET");

    // 303 keeps HEAD.
    let req = through_redirect(Method::HEAD, None, None, 303).await;
    assert_eq!(req.method, "HEAD");

    // A conversion drops Content-Type.
    let req = through_redirect(
        Method::POST,
        Some(b"payload"),
        Some("application/ocsp-request"),
        301,
    )
    .await;
    assert_eq!(req.method, "GET");
    assert_eq!(req.content_type, None);

    // 307 keeps method, body and Content-Type.
    let req = through_redirect(
        Method::POST,
        Some(b"payload"),
        Some("application/ocsp-request"),
        307,
    )
    .await;
    assert_eq!(
        (req.method.as_str(), req.body.as_str()),
        ("POST", "payload")
    );
    assert_eq!(
        req.content_type.as_deref(),
        Some("application/ocsp-request")
    );
}

// ---------------------------------------------------------------- 11: hop URLs

#[tokio::test]
async fn hop_and_response_urls_carry_no_userinfo_and_location_is_absolute() {
    let srv = serve(bind4().await, |req| match req.path.as_str() {
        "/" => redirect(302, "/b"),
        _ => response(200, &[], "end"),
    });
    let p = srv.port;
    let resolver = StubResolver::new(&[("start.invalid", vec![V4])]);
    let mut o = loopback_opts(Method::GET);
    o.max_redirects = 2;
    let url = format!("http://u:p@start.invalid:{p}/");
    let res = fetch(&ClientSettings::default(), Arc::new(resolver), &url, &o)
        .await
        .expect("redirect with userinfo in the start URL");
    assert_eq!(res.status.as_u16(), 200);
    assert_eq!(res.hops.len(), 1);
    let hop = &res.hops[0];
    assert_eq!(hop.url.username(), "");
    assert_eq!(hop.url.password(), None);
    assert_eq!(hop.location, format!("http://start.invalid:{p}/b"));
    assert_eq!(res.url.username(), "");
    assert_eq!(res.url.password(), None);
    assert_eq!(res.url.path(), "/b");
}
