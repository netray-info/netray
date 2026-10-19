//! spec v2-ip-cli Phase 3, criteria C1 and C9-C22: the paths `ip_host` serves, answers by plain
//! text or format suffix, and the requests it hands back unchanged.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::backend::Ifconfig;
use netray_ip::format::OutputFormat;
use netray_ip::handlers;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, LookupOptions};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger, ip_host,
};
use serde_json::{Value, json};

const CALLER: &str = "192.0.2.10:40000";
const CURL: &str = "curl/8.5.0";
const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64) Firefox/130.0";
const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,*/*;q=0.8";
const PLAIN: &str = "text/plain; charset=utf-8";

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_lowercase())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

fn state() -> ApiState {
    ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(StubIpLookup::new())),
        ..Default::default()
    })
}

fn request(method: Method, path: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut req = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let mut req = req.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(CALLER.parse::<SocketAddr>().unwrap()));
    req
}

fn curl_headers() -> Vec<(&'static str, &'static str)> {
    vec![("user-agent", CURL), ("accept", "*/*")]
}

fn browser_headers() -> Vec<(&'static str, &'static str)> {
    vec![("user-agent", BROWSER), ("accept", BROWSER_ACCEPT)]
}

async fn call(
    method: Method,
    path: &str,
    headers: &[(&str, &str)],
) -> Result<(StatusCode, String, String), ()> {
    let sent = request(method.clone(), path, headers);
    let (m, u, h) = (
        sent.method().clone(),
        sent.uri().clone(),
        sent.headers().clone(),
    );
    match ip_host(&state(), sent).await {
        Ok(resp) => Ok(read(resp).await),
        Err(back) => {
            assert_eq!(back.method(), &m, "{path}: method changed");
            assert_eq!(back.uri(), &u, "{path}: uri changed");
            assert_eq!(back.headers(), &h, "{path}: headers changed");
            Err(())
        }
    }
}

async fn read(resp: Response) -> (StatusCode, String, String) {
    let status = resp.status();
    let ct = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, ct, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get(path: &str, headers: &[(&str, &str)]) -> (StatusCode, String, String) {
    call(Method::GET, path, headers)
        .await
        .unwrap_or_else(|()| panic!("{path}: ip_host handed the request back"))
}

async fn caller(ua: Option<&str>) -> Ifconfig {
    StubIpLookup::new()
        .lookup_caller_with(
            CALLER.parse().unwrap(),
            ua,
            LookupOptions {
                reverse_dns: true,
                lang: None,
            },
        )
        .await
        .unwrap()
}

fn filtered(headers: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut req = Request::get("/");
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let req = req.body(()).unwrap();
    let filters = netray_ip::extractors::header_filters(&ApiConfig::default().ip.filtered_headers);
    netray_ip::extractors::filter_headers(
        netray_ip::extractors::extract_headers(req.headers()),
        &filters,
    )
}

type Plain = fn(&Ifconfig) -> String;
type Json = fn(&Ifconfig) -> Option<Value>;

fn field_table() -> Vec<(&'static str, Plain, Json)> {
    vec![
        ("ip", handlers::ip::to_plain, handlers::ip::to_json),
        ("tcp", handlers::tcp::to_plain, handlers::tcp::to_json),
        (
            "location",
            handlers::location::to_plain,
            handlers::location::to_json,
        ),
        (
            "user_agent",
            handlers::user_agent::to_plain,
            handlers::user_agent::to_json,
        ),
        (
            "network",
            handlers::network::to_plain,
            handlers::network::to_json,
        ),
        ("host", handlers::host::to_plain, handlers::host::to_json),
        ("isp", handlers::isp::to_plain, handlers::isp::to_json),
        (
            "country",
            handlers::country::to_plain,
            handlers::country::to_json,
        ),
        ("city", handlers::city::to_plain, handlers::city::to_json),
        ("asn", handlers::asn::to_plain, handlers::asn::to_json),
        (
            "timezone",
            handlers::timezone::to_plain,
            handlers::timezone::to_json,
        ),
        (
            "latitude",
            handlers::latitude::to_plain,
            handlers::latitude::to_json,
        ),
        (
            "longitude",
            handlers::longitude::to_plain,
            handlers::longitude::to_json,
        ),
        (
            "region",
            handlers::region::to_plain,
            handlers::region::to_json,
        ),
    ]
}

#[tokio::test]
async fn every_v1_path_answers_for_get_and_head() {
    let mut paths: Vec<String> = [
        "/",
        "/json",
        "/yaml",
        "/toml",
        "/csv",
        "/ip/cidr",
        "/headers",
        "/headers/json",
        "/all",
        "/all/json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for (field, _, _) in field_table() {
        paths.push(format!("/{field}"));
        paths.push(format!("/{field}/json"));
    }
    let headers = curl_headers();
    for path in &paths {
        let (status, _, body) = get(path, &headers).await;
        assert_eq!(status, StatusCode::OK, "GET {path}: {body}");
        let (status, _, body) = call(Method::HEAD, path, &headers)
            .await
            .unwrap_or_else(|()| panic!("HEAD {path}: handed back"));
        assert_eq!(status, StatusCode::OK, "HEAD {path}");
        assert!(body.is_empty(), "HEAD {path} carried a body: {body}");
    }
    let (status, ct, _) = get("/", &headers).await;
    assert_eq!((status, ct.as_str()), (StatusCode::OK, PLAIN));
}

#[tokio::test]
async fn each_field_answers_its_plain_and_json_renderer() {
    let lookup = caller(Some(CURL)).await;
    for (field, plain, json) in field_table() {
        let (status, ct, body) = get(&format!("/{field}"), &curl_headers()).await;
        assert_eq!(status, StatusCode::OK, "/{field}: {body}");
        assert_eq!(ct, PLAIN, "/{field}");
        assert_eq!(body, plain(&lookup), "/{field}");

        let (status, ct, body) = get(&format!("/{field}/json"), &curl_headers()).await;
        assert_eq!(status, StatusCode::OK, "/{field}/json: {body}");
        assert!(ct.starts_with("application/json"), "/{field}/json: {ct}");
        let expected = json(&lookup).unwrap_or_else(|| panic!("{field}: stub has no json"));
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap(),
            expected,
            "/{field}/json"
        );
    }
}

#[tokio::test]
async fn curl_country_is_the_stub_country_and_a_newline() {
    let (status, ct, body) = get("/country", &curl_headers()).await;
    assert_eq!(
        (status, ct.as_str(), body.as_str()),
        (StatusCode::OK, PLAIN, "Germany\n")
    );
}

#[tokio::test]
async fn a_browser_on_a_field_path_gets_plain_text() {
    let (status, ct, body) = get("/country", &browser_headers()).await;
    assert_eq!(
        (status, ct.as_str(), body.as_str()),
        (StatusCode::OK, PLAIN, "Germany\n")
    );
}

#[tokio::test]
async fn country_with_ip_and_lang_answers_for_that_address() {
    let lookup = StubIpLookup::new()
        .lookup_with(
            "198.51.100.1".parse().unwrap(),
            LookupOptions {
                reverse_dns: true,
                lang: Some("de".into()),
            },
        )
        .await
        .unwrap();
    let (status, ct, body) = get("/country?ip=198.51.100.1&lang=de", &curl_headers()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ct, PLAIN);
    assert_eq!(body, handlers::country::to_plain(&lookup));
}

#[tokio::test]
async fn location_json_is_the_location_of_the_caller_lookup() {
    let lookup = caller(Some(CURL)).await;
    let expected = OutputFormat::Json
        .serialize_body(&handlers::location::to_json(&lookup).unwrap())
        .unwrap();
    let (status, ct, body) = get("/location/json", &curl_headers()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("application/json"), "{ct}");
    assert_eq!(body, expected);
}

#[tokio::test]
async fn json_with_fields_ip_holds_only_ip() {
    let lookup = caller(Some(CURL)).await;
    let (status, _, body) = get("/json?fields=ip", &curl_headers()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    let obj = v.as_object().unwrap();
    assert_eq!(obj.keys().collect::<Vec<_>>(), ["ip"], "{body}");
    assert_eq!(v["ip"], handlers::ip::to_json(&lookup).unwrap());
}

#[tokio::test]
async fn yaml_toml_and_csv_serve_the_document_in_that_form() {
    let lookup = caller(Some(CURL)).await;
    let doc = handlers::root::to_json(&lookup).unwrap();
    let cases = [
        ("/yaml", OutputFormat::Yaml, "application/yaml"),
        ("/toml", OutputFormat::Toml, "application/toml"),
        ("/csv", OutputFormat::Csv, "text/csv"),
    ];
    for (path, format, content_type) in cases {
        let (status, ct, body) = get(path, &curl_headers()).await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        assert!(ct.starts_with(content_type), "{path}: {ct}");
        assert_eq!(body, format.serialize_body(&doc).unwrap(), "{path}");
    }
}

#[tokio::test]
async fn curl_asn_is_the_stub_asn_and_a_newline() {
    let (status, _, body) = get("/asn", &curl_headers()).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "AS64496\n"));
}

#[tokio::test]
async fn asn_with_digits_comes_back_unchanged() {
    assert!(
        call(Method::GET, "/asn/15169", &curl_headers())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn ip_cidr_answers_for_the_caller_and_for_the_query_address() {
    let (status, _, body) = get("/ip/cidr", &curl_headers()).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "192.0.2.10/32\n"));
    let (status, _, body) = get("/ip/cidr?ip=198.51.100.1", &curl_headers()).await;
    assert_eq!(
        (status, body.as_str()),
        (StatusCode::OK, "198.51.100.1/32\n")
    );
}

#[tokio::test]
async fn all_is_the_plain_document_followed_by_the_request_headers() {
    let mut sent = curl_headers();
    sent.push(("x-test", "one"));
    let lookup = caller(Some(CURL)).await;
    let expected = format!(
        "{}{}",
        handlers::all::to_plain(&lookup),
        handlers::headers::to_plain(&filtered(&sent))
    );
    let (status, ct, body) = get("/all", &sent).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(ct, PLAIN);
    assert_eq!(body, expected);
    assert!(
        body.ends_with("x-test: one\n") || body.contains("x-test: one\n"),
        "{body}"
    );
}

#[tokio::test]
async fn headers_json_is_the_filtered_request_headers_object() {
    let mut sent = curl_headers();
    sent.push(("x-test", "one"));
    let (status, ct, body) = get("/headers/json", &sent).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(ct.starts_with("application/json"), "{ct}");
    let expected = handlers::headers::to_json_value_with_xff(&filtered(&sent), &[]);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), expected);

    sent.push(("x-forwarded-for", "203.0.113.9, 198.51.100.7"));
    let (_, _, body) = get("/headers/json", &sent).await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        v["x_forwarded_for_chain"],
        json!(["203.0.113.9", "198.51.100.7"])
    );
}

#[tokio::test]
async fn an_unknown_format_suffix_is_404_naming_the_formats() {
    let (status, _, body) = get("/country/xml", &curl_headers()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"]["code"], "NOT_FOUND", "{body}");
    let message = v["error"]["message"].as_str().unwrap();
    for format in ["json", "yaml", "toml", "csv"] {
        assert!(message.contains(format), "{message}");
    }
}

#[tokio::test]
async fn other_paths_come_back_unchanged() {
    for path in [
        "/range",
        "/batch",
        "/meta",
        "/nope",
        "/country/json/extra",
        "/ip/other",
    ] {
        assert!(
            call(Method::GET, path, &curl_headers()).await.is_err(),
            "GET {path} was served"
        );
    }
}

#[tokio::test]
async fn other_methods_come_back_unchanged() {
    for method in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH] {
        for path in ["/", "/country", "/json"] {
            assert!(
                call(method.clone(), path, &curl_headers()).await.is_err(),
                "{method} {path} was served"
            );
        }
    }
}
