//! V2 spec v2-inspector-shell, requirement 22 (C2, C6, C7, C8): inspector runs carry IP badges
//! (organisation, network type, threat flags) beside every `addr` value, from `enrich` over
//! the process's IP lookup; a shared run shows the badges it was rendered with; without an IP
//! lookup nothing renders and nothing fails.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_ip::backend::Ifconfig;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, IpRefusal};
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::evidence::{Badge, BadgeTone};
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobPart, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

const A: &str = "192.0.2.24";
const B: &str = "192.0.2.25";

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the inspector")
    }
}

/// Answers 192.0.2.24 as an Example Org cloud address on a Tor exit.
struct Flagged(StubIpLookup);

impl IpLookup for Flagged {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            let mut i = self.0.lookup(ip).await?;
            i.network.org = Some("Example Org".into());
            i.network.network_type = "cloud".into();
            i.network.infra_type = "cloud".into();
            i.network.is_tor = true;
            i.network.is_vpn = false;
            i.network.is_spamhaus = false;
            i.network.is_c2 = false;
            Ok(i)
        })
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        ua: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.0.lookup_caller(remote, ua)
    }
    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.0.range(cidr)
    }
    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.0.asn(n)
    }
}

fn state(ip: bool) -> ApiState {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        ip: ip.then(|| Arc::new(Flagged(StubIpLookup::new())) as Arc<dyn IpLookup>),
        ..Default::default()
    })
}

fn tags() -> Vec<Badge> {
    vec![
        Badge {
            label: "Example Org".into(),
            tone: BadgeTone::Muted,
        },
        Badge {
            label: "cloud".into(),
            tone: BadgeTone::Muted,
        },
        Badge {
            label: "Tor".into(),
            tone: BadgeTone::Fail,
        },
    ]
}

fn addr(s: &str) -> Value {
    Value::Addr(s.parse().unwrap())
}

enum Kind {
    Http,
    Mx,
}

/// A stub whose part holds one address block; `enrich` is overridden only when `badges`.
struct Stub {
    kind: Kind,
    badges: bool,
    enriched: Arc<AtomicUsize>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        Ok(())
    }
    fn start(&self, _: JobRequest) -> Result<Job, String> {
        let block = match self.kind {
            Kind::Http => Block {
                id: BlockId::parse("http.server").unwrap(),
                title: "Server".into(),
                shape: Shape::Fields(vec![("address".into(), addr(A))]),
            },
            Kind::Mx => Block {
                id: BlockId::parse("email.mx_addresses").unwrap(),
                title: "MX addresses".into(),
                shape: Shape::Table {
                    key: "address".into(),
                    columns: vec!["address".into(), "host".into()],
                    rows: vec![
                        vec![addr(A), Value::Text("mx1.example.com".into())],
                        vec![addr(B), Value::Text("mx2.example.com".into())],
                    ],
                },
            },
        };
        let part = JobPart {
            label: "Part".into(),
            output: Box::pin(async move { Ok(vec![block]) }),
        };
        Ok(Job::new(vec![part]))
    }
    fn enrich(&self, addrs: Vec<IpAddr>) -> BoxFuture<'static, Vec<(IpAddr, Vec<Badge>)>> {
        if !self.badges {
            return Box::pin(async { Vec::new() });
        }
        self.enriched.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { addrs.into_iter().map(|a| (a, tags())).collect() })
    }
}

fn app(kind: Kind, badges: bool) -> (Router, Arc<AtomicUsize>) {
    let n = Arc::new(AtomicUsize::new(0));
    let jobs = Stub {
        kind,
        badges,
        enriched: n.clone(),
    };
    (
        inspect_router(InspectDeps::new(Arc::new(jobs), InspectConfig::default())),
        n,
    )
}

/// (event name, fragment id, fragment html) per event.
async fn post(app: &Router, tool: &str, body: &str) -> Vec<(String, String, String)> {
    let resp = app
        .clone()
        .oneshot(
            Request::post(format!("/inspect/{tool}/run"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let (mut name, mut data) = (String::new(), String::new());
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            }
            if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            name,
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn html<'a>(events: &'a [(String, String, String)], id: &str) -> &'a str {
    &events
        .iter()
        .find(|e| e.1 == id)
        .unwrap_or_else(|| panic!("no {id} in {events:?}"))
        .2
}

fn parts(events: &[(String, String, String)]) -> String {
    events
        .iter()
        .filter(|e| e.1.starts_with("inspect-part"))
        .map(|e| e.2.as_str())
        .collect()
}

fn key(events: &[(String, String, String)]) -> String {
    let share = html(events, "inspect-share");
    let a = share.find("data-share-key=\"").unwrap() + "data-share-key=\"".len();
    share[a..a + share[a..].find('"').unwrap()].to_owned()
}

fn chips(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = html;
    let open = r#"<span class="ip-badge" data-tone=""#;
    while let Some(i) = rest.find(open) {
        rest = &rest[i + open.len()..];
        let tone = &rest[..rest.find('"').unwrap()];
        let label = &rest[rest.find('>').unwrap() + 1..];
        out.push((
            tone.to_owned(),
            label[..label.find('<').unwrap()].to_owned(),
        ));
    }
    out
}

fn expected() -> Vec<(String, String)> {
    [
        ("muted", "Example Org"),
        ("muted", "cloud"),
        ("fail", "Tor"),
    ]
    .map(|(t, l)| (t.to_owned(), l.to_owned()))
    .to_vec()
}

#[tokio::test]
async fn c2_c6_enrich_over_the_ip_lookup_names_org_type_and_tor() {
    let jobs = InspectJobs::new(state(true));

    let got = jobs.enrich(vec![A.parse().unwrap()]).await;

    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, A.parse::<IpAddr>().unwrap());
    assert_eq!(got[0].1, tags());
}

#[tokio::test]
async fn c8_enrich_without_an_ip_lookup_is_empty() {
    let jobs = InspectJobs::new(state(false));

    assert!(jobs.enrich(vec![A.parse().unwrap()]).await.is_empty());
}

#[tokio::test]
async fn c2_c6_an_http_part_renders_the_address_badges_in_their_tones() {
    let (app, _) = app(Kind::Http, true);

    let events = post(&app, "http", "url=https%3A%2F%2Fexample.com%2F").await;

    let p = parts(&events);
    assert_eq!(p.matches(r#"class="ip-badges""#).count(), 1, "{p}");
    assert_eq!(chips(&p), expected(), "{p}");
}

#[tokio::test]
async fn c7_every_mx_address_carries_its_badges() {
    let (app, _) = app(Kind::Mx, true);

    let events = post(&app, "dns", "q=example.com+MX").await;

    let p = parts(&events);
    assert_eq!(p.matches(r#"class="ip-badges""#).count(), 2, "{p}");
    let mut both = expected();
    both.extend(expected());
    assert_eq!(chips(&p), both, "{p}");
}

#[tokio::test]
async fn c8_without_badges_the_run_still_ends_with_the_part_rendered() {
    let (app, _) = app(Kind::Http, false);

    let events = post(&app, "http", "url=https%3A%2F%2Fexample.com%2F").await;

    assert!(
        !events.iter().any(|e| e.2.contains("ip-badge")),
        "{events:?}"
    );
    assert!(parts(&events).contains(A), "{events:?}");
    assert_eq!(events.last().unwrap().0, "end", "{events:?}");
}

#[tokio::test]
async fn shared_page_shows_the_stored_badges_without_enriching_again() {
    let (app, enriched) = app(Kind::Http, true);
    let events = post(&app, "http", "url=https%3A%2F%2Fexample.com%2F").await;
    assert_eq!(enriched.load(Ordering::SeqCst), 1);

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/inspect/http?r={}", key(&events)))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let page = String::from_utf8_lossy(&bytes).into_owned();

    assert_eq!(chips(&page), expected(), "{page}");
    assert_eq!(enriched.load(Ordering::SeqCst), 1);
}
