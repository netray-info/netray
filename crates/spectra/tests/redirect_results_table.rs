// Pinning table for `execute_request`'s redirect-following results, plus the verdict of the
// quality check that depends on the redirect outcome (`redirect_limit`).
//
// Each row records today's behaviour: final status, `redirect_limit_reached`, the hop list
// (URL, status, location) and the `redirect_limit` check as `assemble_response` emits it.
// The redirect policy in `inspect/request.rs` only inspects IP-literal hops; every other
// hop is followed. The rows pin exactly that.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::IntoResponse;
use spectra::inspect::assembler::RedirectHop;
use netray_common::fetch::Resolve;
use spectra::inspect::request::{Outbound, execute_request};
use spectra::inspect::{EnrichmentData, InspectResult, TaskResult, assemble_response};
use spectra::quality::types::CheckStatus;
use tokio::net::TcpListener;
use url::Url;

/// One canned response: status, optional Location, optional extra header.
#[derive(Clone)]
struct Canned {
    status: u16,
    location: Option<String>,
    header: Option<(&'static str, &'static str)>,
}

fn redirect(status: u16, location: String) -> Canned {
    Canned {
        status,
        location: Some(location),
        header: None,
    }
}

async fn bind() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

/// Serve `routes` (path -> response) on `listener`; returns a request counter.
fn serve(listener: TcpListener, routes: HashMap<String, Canned>) -> Arc<AtomicUsize> {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_h = Arc::clone(&hits);
    let app = Router::new().fallback(move |uri: Uri| {
        let hits = Arc::clone(&hits_h);
        let routes = routes.clone();
        async move {
            hits.fetch_add(1, Ordering::SeqCst);
            match routes.get(uri.path()) {
                Some(c) => {
                    let mut h = HeaderMap::new();
                    if let Some(loc) = &c.location {
                        h.insert("location", HeaderValue::from_str(loc).unwrap());
                    }
                    if let Some((k, v)) = c.header {
                        h.insert(HeaderName::from_static(k), HeaderValue::from_static(v));
                    }
                    (StatusCode::from_u16(c.status).unwrap(), h).into_response()
                }
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    });
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    hits
}

fn hop(url: &str, status: u16, location: &str) -> (String, u16, String) {
    (url.to_string(), status, location.to_string())
}

fn hop_tuples(hops: &[RedirectHop]) -> Vec<(String, u16, String)> {
    hops.iter()
        .map(|h| (h.url.clone(), h.status, h.location.clone().unwrap()))
        .collect()
}

/// Stub resolver: name -> addresses; unknown names resolve to nothing.
struct Stub(HashMap<String, Vec<IpAddr>>);

impl Resolve for Stub {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let ips = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { ips })
    }
}

fn outbound(names: &[(&str, Vec<IpAddr>)]) -> Outbound {
    Outbound {
        resolver: Arc::new(Stub(
            names
                .iter()
                .map(|(n, ips)| (n.to_string(), ips.clone()))
                .collect(),
        )),
        allow: |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

/// `localhost` -> 127.0.0.1, the only admitted address.
fn localhost_v4() -> Vec<(&'static str, Vec<IpAddr>)> {
    vec![("localhost", vec![v4(127, 0, 0, 1)])]
}

async fn run(
    url: &str,
    port: u16,
    max_redirects: usize,
    names: &[(&str, Vec<IpAddr>)],
) -> TaskResult {
    let resolved = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    execute_request(
        &outbound(names),
        Url::parse(url).unwrap(),
        resolved,
        max_redirects,
        Duration::from_secs(5),
        "test-agent",
        None,
    )
    .await
}

/// The `redirect_limit` check as the assembled response reports it: (status, message), or
/// None when the check is absent.
fn redirect_limit_check(result: TaskResult) -> Option<(CheckStatus, Option<String>)> {
    let empty = || TaskResult {
        final_url: "https://example.com/".into(),
        status: 200,
        http_version: "h1.1".into(),
        headers: reqwest::header::HeaderMap::new(),
        redirects: vec![],
        redirect_limit_reached: false,
        error: None,
    };
    let resp = assemble_response(
        &Url::parse("https://example.com/").unwrap(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 443),
        InspectResult {
            https: result,
            http_upgrade: None,
            cors: empty(),
        },
        EnrichmentData::default(),
        None,
        1,
    );
    resp.quality
        .checks
        .iter()
        .find(|c| c.name == "redirect_limit")
        .map(|c| (c.status.clone(), c.message.clone()))
}

#[tokio::test]
async fn redirect_results_table() {
    // Row 1: chain of 3 redirects ending in 200, max_redirects 10.
    // Each 3xx is followed and recorded as (requested URL, its status, absolute Location).
    // Limit not reached (3 < 10), so the `redirect_limit` check is absent.
    // Hops use the name `localhost`: loopback IP-literal hops are blocked by the guard
    // (error "Redirect destination blocked"), names are not checked.
    {
        let (l, p) = bind().await;
        let base = format!("http://localhost:{p}");
        let routes = HashMap::from([
            ("/r0".to_string(), redirect(301, format!("{base}/r1"))),
            ("/r1".to_string(), redirect(302, format!("{base}/r2"))),
            ("/r2".to_string(), redirect(307, format!("{base}/r3"))),
            (
                "/r3".to_string(),
                Canned {
                    status: 200,
                    location: None,
                    header: None,
                },
            ),
        ]);
        serve(l, routes);
        let r = run(&format!("{base}/r0"), p, 10, &localhost_v4()).await;
        assert_eq!(r.error, None, "row 1");
        assert_eq!(r.status, 200, "row 1");
        assert_eq!(r.final_url, format!("{base}/r3"), "row 1");
        assert!(!r.redirect_limit_reached, "row 1");
        assert_eq!(
            hop_tuples(&r.redirects),
            vec![
                hop(&format!("{base}/r0"), 301, &format!("{base}/r1")),
                hop(&format!("{base}/r1"), 302, &format!("{base}/r2")),
                hop(&format!("{base}/r2"), 307, &format!("{base}/r3")),
            ],
            "row 1"
        );
        assert_eq!(redirect_limit_check(r), None, "row 1");
    }

    // Row 2: redirect loop /a -> /b -> /a, max_redirects 10.
    // The policy has no loop detection; it follows until 10 hops are recorded, then stops on
    // the 11th 3xx response. That response (from /a, status 302) is the final result and is
    // not an error. The limit flag is set and the check warns with the hop count (10).
    {
        let (l, p) = bind().await;
        let base = format!("http://localhost:{p}");
        let routes = HashMap::from([
            ("/a".to_string(), redirect(302, format!("{base}/b"))),
            ("/b".to_string(), redirect(302, format!("{base}/a"))),
        ]);
        let hits = serve(l, routes);
        let r = run(&format!("{base}/a"), p, 10, &localhost_v4()).await;
        assert_eq!(r.error, None, "row 2");
        assert_eq!(r.status, 302, "row 2");
        assert_eq!(r.final_url, format!("{base}/a"), "row 2");
        assert!(r.redirect_limit_reached, "row 2");
        assert_eq!(hits.load(Ordering::SeqCst), 11, "row 2");
        let expected: Vec<_> = (0..10)
            .map(|i| {
                let (from, to) = if i % 2 == 0 {
                    ("/a", "/b")
                } else {
                    ("/b", "/a")
                };
                hop(&format!("{base}{from}"), 302, &format!("{base}{to}"))
            })
            .collect();
        assert_eq!(hop_tuples(&r.redirects), expected, "row 2");
        assert_eq!(
            redirect_limit_check(r),
            Some((
                CheckStatus::Warn,
                Some("Redirect limit reached (10)".to_string())
            )),
            "row 2"
        );
    }

    // Row 3: the initial target answers 302 to http://localhost:<p2>/ while the stub resolves
    // `localhost` to [::1], which `allow` refuses. The redirect is not followed: p2 (a
    // listener on [::1]) sees no connection. The result is the blocked shape: error
    // "Redirect destination blocked", status 0, the initial URL as final URL, no headers,
    // limit not reached. The hop list keeps the refused redirect; the `redirect_limit`
    // check is absent.
    {
        let (l1, p1) = bind().await;
        let l2 = TcpListener::bind("[::1]:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let target = format!("http://localhost:{p2}/");
        serve(
            l1,
            HashMap::from([("/".to_string(), redirect(302, target.clone()))]),
        );
        let hits2 = serve(
            l2,
            HashMap::from([(
                "/".to_string(),
                Canned {
                    status: 200,
                    location: None,
                    header: Some(("x-second-listener", "reached")),
                },
            )]),
        );
        let first = format!("http://127.0.0.1:{p1}/");
        let names = vec![("localhost", vec![IpAddr::V6(Ipv6Addr::LOCALHOST)])];
        let r = run(&first, p1, 10, &names).await;
        assert_eq!(
            r.error.as_deref(),
            Some("Redirect destination blocked"),
            "row 3"
        );
        assert_eq!(r.status, 0, "row 3");
        assert_eq!(r.final_url, first, "row 3");
        assert!(!r.redirect_limit_reached, "row 3");
        assert_eq!(hits2.load(Ordering::SeqCst), 0, "row 3: p2 was not reached");
        assert!(r.headers.get("x-second-listener").is_none(), "row 3");
        assert_eq!(
            hop_tuples(&r.redirects),
            vec![hop(&first, 302, &target)],
            "row 3"
        );
        assert_eq!(redirect_limit_check(r), None, "row 3");
    }

    // Row 4 (C15): the initial target answers 302 to http://mixed.test:<p2>/, and the stub
    // resolves `mixed.test` to [127.0.0.1, 10.0.0.1]. One refused address refuses the whole
    // name: blocked shape (error "Redirect destination blocked", status 0), and the
    // listener p2 on 127.0.0.1 sees no connection.
    {
        let (l1, p1) = bind().await;
        let (l2, p2) = bind().await;
        let target = format!("http://mixed.test:{p2}/");
        serve(
            l1,
            HashMap::from([("/".to_string(), redirect(302, target.clone()))]),
        );
        let hits2 = serve(
            l2,
            HashMap::from([(
                "/".to_string(),
                Canned {
                    status: 200,
                    location: None,
                    header: None,
                },
            )]),
        );
        let first = format!("http://127.0.0.1:{p1}/");
        let names = vec![("mixed.test", vec![v4(127, 0, 0, 1), v4(10, 0, 0, 1)])];
        let r = run(&first, p1, 10, &names).await;
        assert_eq!(
            r.error.as_deref(),
            Some("Redirect destination blocked"),
            "row 4"
        );
        assert_eq!(r.status, 0, "row 4");
        assert_eq!(hits2.load(Ordering::SeqCst), 0, "row 4: p2 was not reached");
    }
}
