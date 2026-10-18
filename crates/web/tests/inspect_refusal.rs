//! C11: a refused admission answers 429 with `Retry-After` and a one-fragment event stream,
//! never starts the job, and hands the limiter the tool, the input and the derived client address.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_common::ip_extract::IpExtractor;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Tool, inspect_router,
};
use tower::ServiceExt;

#[derive(Default)]
struct Refusing {
    admits: Mutex<Vec<(Tool, IpAddr, String)>>,
    starts: Mutex<u32>,
}

impl InspectorJobs for Refusing {
    fn admit(&self, tool: Tool, client: IpAddr, input: &str) -> Result<(), Duration> {
        self.admits
            .lock()
            .unwrap()
            .push((tool, client, input.to_string()));
        Err(Duration::from_secs(20))
    }

    fn start(&self, _req: JobRequest) -> Result<Job, String> {
        *self.starts.lock().unwrap() += 1;
        Ok(Job::new(vec![]))
    }
}

struct Case {
    name: &'static str,
    trust_proxy: bool,
    forwarded_for: Option<&'static str>,
    want_client: &'static str,
}

#[tokio::test]
async fn refused_admission_answers_429_and_names_the_derived_client() {
    let cases = [
        Case {
            name: "no proxy trusted: the peer",
            trust_proxy: false,
            forwarded_for: None,
            want_client: "192.0.2.10",
        },
        Case {
            name: "trusted proxy: the forwarded client",
            trust_proxy: true,
            forwarded_for: Some("192.0.2.7"),
            want_client: "192.0.2.7",
        },
    ];

    for case in cases {
        let jobs = Arc::new(Refusing::default());
        let mut deps = InspectDeps::new(jobs.clone(), InspectConfig::default());
        if case.trust_proxy {
            deps = deps.with_ip_extractor(Arc::new(IpExtractor::new(&["192.0.2.10/32".into()])));
        }
        let mut req = Request::post("/inspect/dns/run")
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(xff) = case.forwarded_for {
            req = req.header("x-forwarded-for", xff);
        }
        let mut req = req.body(Body::from("q=example.com")).unwrap();
        let peer: SocketAddr = "192.0.2.10:40000".parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));

        let res = inspect_router(deps).oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS, "{}", case.name);
        assert_eq!(res.headers()["retry-after"], "20", "{}", case.name);
        assert_eq!(
            res.headers()["content-type"],
            "text/event-stream",
            "{}",
            case.name
        );
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        let events: Vec<&str> = body.split("\n\n").filter(|e| !e.is_empty()).collect();
        assert_eq!(events.len(), 2, "{}: {body}", case.name);
        assert!(events[0].starts_with("event: fragment"), "{}", case.name);
        assert!(
            events[0].contains("too many requests; try again in 20 s"),
            "{}: {body}",
            case.name
        );
        assert!(events[1].starts_with("event: end"), "{}", case.name);

        assert_eq!(*jobs.starts.lock().unwrap(), 0, "{}", case.name);
        let admits = jobs.admits.lock().unwrap();
        assert_eq!(
            *admits,
            vec![(
                Tool::Dns,
                case.want_client.parse::<IpAddr>().unwrap(),
                "example.com".to_string()
            )],
            "{}",
            case.name
        );
    }
}
