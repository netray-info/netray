//! Redirects and 410 through the host router (spec v2-cutover, requirements 13 and 14). One
//! row per line: `(host, method, path_and_query, status, expected)`, where `host` is the whole
//! Host header value and `expected` is the absolute `Location` for a 301, the successor URL the
//! 410 body's `error.message` must contain, or a body substring for any other status.
//! `tests/repo/test_v1_route_coverage.sh` greps this table; keep one row per line.
use axum::Router;
use axum::body::Body;
use axum::http::{Request, header};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::host::{HostRouters, router};
use tower::ServiceExt;

fn stub(body: &'static str) -> Router {
    Router::new().route("/", get(move || async move { body }))
}

fn host_router(apex: &str) -> Router {
    router(HostRouters {
        apex: apex.to_string(),
        probes: Router::new()
            .route("/health", get(|| async { "probe" }))
            .route("/ready", get(|| async { "probe" })),
        apex_router: Router::new().fallback(|| async { "apex" }),
        dns: Some(stub("dns-inspector")),
        tls: Some(stub("tls-inspector")),
        http: Some(stub("http-inspector")),
        ip: Some(stub("ip-inspector")),
        ip_api: None,
    })
}

async fn send(app: &Router, host: &str, method: &str, path: &str) -> (u16, Option<String>, String) {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let location = res
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        location,
        String::from_utf8_lossy(&body).into_owned(),
    )
}

#[rustfmt::skip]
const ROWS: &[(&str, &str, &str, u16, &str)] = &[
    ("lens.netray.info", "GET", "/r/abcd1234?x=1", 301, "https://netray.info/r/abcd1234?x=1"),
    ("lens.netray.info", "HEAD", "/r/abcd1234?x=1", 301, "https://netray.info/r/abcd1234?x=1"),
    ("lens.netray.info", "GET", "/og/example.com.png", 301, "https://netray.info/og/example.com.png"),
    // The apex serves /r/<id> (entries, 13j) and /og/<domain>.png (the OG card) itself.
    ("netray.info", "GET", "/r/abcd1234", 200, "apex"),
    ("netray.info", "GET", "/r/abcd1234/tls", 200, "apex"),
    ("netray.info", "GET", "/og/example.com.png", 200, "apex"),
    ("lens.netray.info", "GET", "/x", 301, "https://netray.info/x"),
    ("lens.netray.info", "GET", "/docs", 301, "https://netray.info/docs"),
    ("lens.netray.info", "GET", "/api/check/example.com", 301, "https://netray.info/api/check/example.com"),
    ("lens.netray.info", "GET", "/api/check", 301, "https://netray.info/api/check"),
    ("lens.netray.info", "GET", "/api/meta", 301, "https://netray.info/api/meta"),
    ("lens.netray.info", "GET", "/api-docs/openapi.json", 301, "https://netray.info/api-docs/openapi.json"),
    ("lens.netray.info", "GET", "/health", 200, "probe"),
    ("lens.netray.info", "GET", "/ready", 200, "probe"),
    ("netray.info", "GET", "/?d=example.com", 301, "https://netray.info/example.com"),
    ("netray.info", "HEAD", "/?d=example.com", 301, "https://netray.info/example.com"),
    ("netray.info", "GET", "/?d=not%20a%20domain", 400, ""),
    ("email.netray.info", "GET", "/?q=example.com%20google%20selector1", 301, "https://netray.info/example.com/email"),
    ("email.netray.info", "GET", "/?q=example.com&domain=netray.info", 301, "https://netray.info/example.com/email"),
    ("email.netray.info", "GET", "/?domain=example.com", 301, "https://netray.info/example.com/email"),
    ("email.netray.info", "HEAD", "/?domain=example.com", 301, "https://netray.info/example.com/email"),
    ("email.netray.info", "GET", "/", 301, "https://netray.info/"),
    ("email.netray.info", "GET", "/?q=%20", 301, "https://netray.info/"),
    ("email.netray.info", "GET", "/x/y", 301, "https://netray.info/"),
    ("email.netray.info", "GET", "/health", 200, "probe"),
    ("email.netray.info", "POST", "/inspect/example.com", 410, "https://netray.info/api/v1/email/inspect"),
    ("email.netray.info", "GET", "/inspect/example.com", 410, "https://netray.info/api/v1/email/inspect"),
    ("email.netray.info", "GET", "/inspect", 410, "https://netray.info/api/v1/email/inspect"),
    ("email.netray.info", "POST", "/inspect", 410, "https://netray.info/api/v1/email/inspect"),
    ("dns.netray.info", "POST", "/api/check", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "DELETE", "/api/check", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/authcompare", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/compare", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/config", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/dnssec", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/parse", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/record-types", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/servers", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/trace", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/results/k1", 410, "https://netray.info/api/v1"),
    ("http.netray.info", "GET", "/api/config", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/inspect", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("dns.netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("dns.netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("dns.netray.info", "HEAD", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("dns.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("tls.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("tls.netray.info", "GET", "/api/inspect", 410, "https://netray.info/api/v1"),
    ("tls.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("tls.netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("tls.netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("tls.netray.info", "HEAD", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("tls.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("http.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("http.netray.info", "GET", "/api/inspect", 410, "https://netray.info/api/v1"),
    ("http.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("http.netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("http.netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("http.netray.info", "HEAD", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("http.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("email.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("email.netray.info", "GET", "/api/inspect", 410, "https://netray.info/api/v1"),
    ("email.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("email.netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("email.netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("email.netray.info", "HEAD", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("email.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("ip.netray.info", "GET", "/api/query", 410, "https://netray.info/api/v1"),
    ("ip.netray.info", "GET", "/api/inspect", 410, "https://netray.info/api/v1"),
    ("ip.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("ip.netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("ip.netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("ip.netray.info", "HEAD", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("ip.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("dns.netray.info", "GET", "/", 200, "dns-inspector"),
    ("dns.netray.info", "GET", "/?q=example.com", 200, "dns-inspector"),
    ("dns.netray.info", "GET", "/?q=example.com&r=k1", 200, "dns-inspector"),
    ("tls.netray.info", "GET", "/?h=example.com", 200, "tls-inspector"),
    ("http.netray.info", "GET", "/?url=https%3A%2F%2Fexample.com", 200, "http-inspector"),
    ("ip.netray.info", "GET", "/?ip=192.0.2.1", 200, "ip-inspector"),
    ("netray.info", "GET", "/api/check", 410, "https://netray.info/api/v1/domains/{domain}/runs"),
    ("netray.info", "POST", "/api/check", 410, "https://netray.info/api/v1/domains/{domain}/runs"),
    ("netray.info", "GET", "/api/check/example.com", 410, "https://netray.info/api/v1/domains/{domain}/runs"),
    ("netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1"),
    ("netray.info", "GET", "/api/v1/meta", 200, "apex"),
    // v2-site-nav: /tools is the inspector index, served by the pages (row l superseded).
    ("netray.info", "GET", "/tools", 200, "apex"),
    ("netray.info", "GET", "/tools/", 200, "apex"),
    ("netray.info", "GET", "/api/", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/index", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/dns", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/tls", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/http", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/email", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/ip", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/lens", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/index.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/dns.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/tls.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/http.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/email.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/ip.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api/lens.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "HEAD", "/api/dns.html", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/guide/lens", 301, "https://netray.info/guide/scoring"),
    ("netray.info", "GET", "/docs", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/docs/", 301, "https://netray.info/api/v1/docs"),
    ("netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
    ("ip.netray.info", "GET", "/json", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/yaml", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/toml", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/csv", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/all", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/all/json", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/ip", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/ip/json", 410, "https://netray.info/api/v1/ip"),
    ("ip.netray.info", "GET", "/ip/cidr", 410, "https://netray.info/api/v1/ip/range/{cidr}"),
    ("ip.netray.info", "GET", "/range", 410, "https://netray.info/api/v1/ip/range/{cidr}"),
    ("ip.netray.info", "GET", "/asn", 410, "https://netray.info/api/v1/asn/{n}"),
    ("ip.netray.info", "GET", "/asn/13335", 410, "https://netray.info/api/v1/asn/{n}"),
    ("ip.netray.info", "GET", "/tcp", 410, "https://netray.info/api/v1/ip?field=tcp"),
    ("ip.netray.info", "GET", "/tcp/json", 410, "https://netray.info/api/v1/ip?field=tcp"),
    ("ip.netray.info", "GET", "/location", 410, "https://netray.info/api/v1/ip?field=location"),
    ("ip.netray.info", "GET", "/location/json", 410, "https://netray.info/api/v1/ip?field=location"),
    ("ip.netray.info", "GET", "/user_agent", 410, "https://netray.info/api/v1/ip?field=user_agent"),
    ("ip.netray.info", "GET", "/user_agent/json", 410, "https://netray.info/api/v1/ip?field=user_agent"),
    ("ip.netray.info", "GET", "/network", 410, "https://netray.info/api/v1/ip?field=network"),
    ("ip.netray.info", "GET", "/network/json", 410, "https://netray.info/api/v1/ip?field=network"),
    ("ip.netray.info", "GET", "/host", 410, "https://netray.info/api/v1/ip?field=host"),
    ("ip.netray.info", "GET", "/host/json", 410, "https://netray.info/api/v1/ip?field=host"),
    ("ip.netray.info", "GET", "/isp", 410, "https://netray.info/api/v1/ip?field=isp"),
    ("ip.netray.info", "GET", "/isp/json", 410, "https://netray.info/api/v1/ip?field=isp"),
    ("ip.netray.info", "GET", "/country", 410, "https://netray.info/api/v1/ip?field=country"),
    ("ip.netray.info", "GET", "/country/json", 410, "https://netray.info/api/v1/ip?field=country"),
    ("ip.netray.info", "GET", "/city", 410, "https://netray.info/api/v1/ip?field=city"),
    ("ip.netray.info", "GET", "/city/json", 410, "https://netray.info/api/v1/ip?field=city"),
    ("ip.netray.info", "GET", "/timezone", 410, "https://netray.info/api/v1/ip?field=timezone"),
    ("ip.netray.info", "GET", "/timezone/json", 410, "https://netray.info/api/v1/ip?field=timezone"),
    ("ip.netray.info", "GET", "/latitude", 410, "https://netray.info/api/v1/ip?field=latitude"),
    ("ip.netray.info", "GET", "/latitude/json", 410, "https://netray.info/api/v1/ip?field=latitude"),
    ("ip.netray.info", "GET", "/longitude", 410, "https://netray.info/api/v1/ip?field=longitude"),
    ("ip.netray.info", "GET", "/longitude/json", 410, "https://netray.info/api/v1/ip?field=longitude"),
    ("ip.netray.info", "GET", "/region", 410, "https://netray.info/api/v1/ip?field=region"),
    ("ip.netray.info", "GET", "/region/json", 410, "https://netray.info/api/v1/ip?field=region"),
    ("ip.netray.info", "GET", "/headers", 410, "https://netray.info/api/v1/ip?field=headers"),
    ("ip.netray.info", "GET", "/headers/json", 410, "https://netray.info/api/v1/ip?field=headers"),
    ("ip.netray.info", "GET", "/ipv4", 410, "https://netray.info/api/v1/ip/{address}"),
    ("ip.netray.info", "GET", "/ipv4/json", 410, "https://netray.info/api/v1/ip/{address}"),
    ("ip.netray.info", "GET", "/ipv4", 410, "curl -4"),
    ("ip.netray.info", "GET", "/ipv6", 410, "https://netray.info/api/v1/ip/{address}"),
    ("ip.netray.info", "GET", "/ipv6/json", 410, "https://netray.info/api/v1/ip/{address}"),
    ("ip.netray.info", "GET", "/ipv6", 410, "curl -6"),
    ("ip.netray.info", "GET", "/batch", 410, "https://netray.info/api/v1/ip/batch"),
    ("ip.netray.info", "POST", "/batch", 410, "https://netray.info/api/v1/ip/batch"),
    ("ip.netray.info", "POST", "/batch/json", 410, "https://netray.info/api/v1/ip/batch"),
    ("ip.netray.info", "GET", "/diff", 410, "https://netray.info/api/v1/ip/diff"),
    ("ip.netray.info", "GET", "/meta", 410, "https://netray.info/api/v1/meta"),
    ("ip.netray.info", "GET", "/api/meta", 410, "https://netray.info/api/v1/meta"),
    ("ip.netray.info", "GET", "/api-docs/openapi.json", 410, "https://netray.info/api/v1/openapi.json"),
];

#[tokio::test]
async fn redirects_and_gone_rows() {
    let app = host_router("netray.info");
    let mut failures = Vec::new();
    for (host, method, path, status, expected) in ROWS {
        let (got, location, body) = send(&app, host, method, path).await;
        if got != *status {
            failures.push(format!(
                "{host} {method} {path}: status {got}, want {status}"
            ));
            continue;
        }
        match status {
            301 => {
                if location.as_deref() != Some(*expected) {
                    failures.push(format!(
                        "{host} {method} {path}: Location {location:?}, want {expected}"
                    ));
                }
            }
            410 => match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(json) => {
                    let message = json["error"]["message"].as_str().unwrap_or("");
                    if !message.contains(expected) {
                        failures.push(format!(
                            "{host} {method} {path}: message {message:?} lacks {expected}"
                        ));
                    }
                    let shaped = path.starts_with("/api") || path.starts_with("/inspect");
                    if shaped && json["error"]["code"] != "gone" {
                        failures.push(format!("{host} {method} {path}: code is not gone: {body}"));
                    }
                }
                Err(_) => {
                    failures.push(format!("{host} {method} {path}: 410 body not JSON: {body}"))
                }
            },
            _ => {
                if !body.contains(expected) {
                    failures.push(format!(
                        "{host} {method} {path}: body {body:?} lacks {expected:?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} rows fail:\n{}",
        failures.len(),
        ROWS.len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn lens_host_redirects_to_a_non_default_apex() {
    let app = host_router("localhost");
    let (status, location, _) = send(&app, "lens.localhost", "GET", "/x").await;
    assert_eq!(
        (status, location.as_deref()),
        (301, Some("https://localhost/x"))
    );
}
