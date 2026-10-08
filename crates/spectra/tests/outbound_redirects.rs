// Scenario tests for `execute_request` redirect handling through the outbound fetch seam:
// C10 (refused redirect targets never get a connection) and C11 (an allowed chain records
// exactly its hops).
//
// The test `Outbound` uses a stub resolver and an `allow` that admits only 127.0.0.1.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::IntoResponse;
use netray_common::fetch::Resolve;
use spectra::inspect::TaskResult;
use spectra::inspect::request::{Outbound, execute_request};
use tokio::net::TcpListener;
use url::Url;

struct Stub(HashMap<String, Vec<IpAddr>>);

impl Resolve for Stub {
    fn resolve(&self, host: &str) -> Pin<Box<dyn Future<Output = Vec<IpAddr>> + Send>> {
        let ips = self.0.get(host).cloned().unwrap_or_default();
        Box::pin(async move { ips })
    }
}

fn outbound(names: &[(&str, IpAddr)]) -> Outbound {
    Outbound {
        resolver: Arc::new(Stub(
            names
                .iter()
                .map(|(n, ip)| (n.to_string(), vec![*ip]))
                .collect(),
        )),
        allow: |ip| ip == IpAddr::V4(Ipv4Addr::LOCALHOST),
    }
}

const LO4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const LO6: IpAddr = IpAddr::V6(Ipv6Addr::LOCALHOST);

/// Serve `routes` (path -> (status, optional Location)) on `listener`; returns a hit counter.
fn serve(listener: TcpListener, routes: Vec<(&str, u16, Option<String>)>) -> Arc<AtomicUsize> {
    let routes: HashMap<String, (u16, Option<String>)> = routes
        .into_iter()
        .map(|(p, s, l)| (p.to_string(), (s, l)))
        .collect();
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_h = Arc::clone(&hits);
    let app = Router::new().fallback(move |uri: Uri| {
        let hits = Arc::clone(&hits_h);
        let routes = routes.clone();
        async move {
            hits.fetch_add(1, Ordering::SeqCst);
            match routes.get(uri.path()) {
                Some((status, loc)) => {
                    let mut h = HeaderMap::new();
                    if let Some(loc) = loc {
                        h.insert("location", HeaderValue::from_str(loc).unwrap());
                    }
                    (StatusCode::from_u16(*status).unwrap(), h).into_response()
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

async fn bind4() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = l.local_addr().unwrap().port();
    (l, p)
}

async fn run(url: &str, port: u16, names: &[(&str, IpAddr)]) -> TaskResult {
    execute_request(
        &outbound(names),
        Url::parse(url).unwrap(),
        SocketAddr::new(LO4, port),
        10,
        Duration::from_secs(5),
        "test-agent",
        None,
    )
    .await
}

#[tokio::test]
async fn c10_redirect_to_refused_target_is_blocked_without_connecting() {
    // The initial target is an allowed listener on 127.0.0.1 answering 302. Each Location
    // points at the [::1] listener p2, by name, by literal, and by an unresolvable-style
    // name that the stub maps to ::1.
    for (case, host) in [
        ("localhost", "localhost"),
        ("literal", "[::1]"),
        ("svc.invalid", "svc.invalid"),
    ] {
        let (l1, p1) = bind4().await;
        let l2 = TcpListener::bind("[::1]:0").await.unwrap();
        let p2 = l2.local_addr().unwrap().port();
        let hits2 = serve(l2, vec![("/", 200, None)]);
        serve(l1, vec![("/", 302, Some(format!("http://{host}:{p2}/")))]);

        let names = [("localhost", LO6), ("svc.invalid", LO6)];
        let r = run(&format!("http://127.0.0.1:{p1}/"), p1, &names).await;

        assert_eq!(
            r.error.as_deref(),
            Some("Redirect destination blocked"),
            "{case}"
        );
        assert_eq!(hits2.load(Ordering::SeqCst), 0, "{case}: p2 was reached");
    }
}

#[tokio::test]
async fn c11_allowed_chain_records_exactly_its_hops() {
    // A -301-> B -302-> C -200, all on 127.0.0.1 under stub names.
    let (la, pa) = bind4().await;
    let (lb, pb) = bind4().await;
    let (lc, pc) = bind4().await;
    let a = format!("http://a.test:{pa}/a");
    let b = format!("http://b.test:{pb}/b");
    let c = format!("http://c.test:{pc}/c");
    serve(la, vec![("/a", 301, Some(b.clone()))]);
    serve(lb, vec![("/b", 302, Some(c.clone()))]);
    serve(lc, vec![("/c", 200, None)]);

    let names = [("a.test", LO4), ("b.test", LO4), ("c.test", LO4)];
    let r = run(&a, pa, &names).await;

    assert_eq!(r.error, None);
    assert_eq!(r.status, 200);
    assert_eq!(r.final_url, c);
    assert!(!r.redirect_limit_reached);
    let hops: Vec<_> = r
        .redirects
        .iter()
        .map(|h| (h.url.clone(), h.status, h.location.clone()))
        .collect();
    assert_eq!(
        hops,
        vec![(a, 301, Some(b.clone())), (b, 302, Some(c))],
    );
}
