//! API v1 fresh runs (spec v2-api, requirement 14): `POST /domains/{domain}/runs` driven through
//! `netray_web::api::router` with a scripted `DomainRuns`, paths relative to the mount.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::{SectionEvent, SectionOutcome};
use netray_model::diff::{Diff, GradeChange};
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::Recorded;
use netray_web::api::{
    ApiConfig, ApiState, DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunRefusal, RunStart,
    RunTrigger,
};
use serde_json::{Value, json};
use tower::ServiceExt;

type Script = Box<dyn Fn() -> RunStart + Send + Sync>;
type Call = (String, IpAddr, RunTrigger, bool);

struct StubRuns {
    script: Script,
    calls: Mutex<Vec<Call>>,
}

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        let d = domain.trim().to_lowercase();
        let d = d.strip_suffix('.').unwrap_or(&d).to_string();
        if d.is_empty() || d.contains('*') {
            return Err(DomainRefusal {
                code: "DOMAIN_INVALID",
                message: format!("invalid domain: {domain}"),
            });
        }
        Ok(d)
    }
    fn start<'a>(
        &'a self,
        domain: &'a str,
        client: IpAddr,
        trigger: RunTrigger,
        prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        self.calls
            .lock()
            .unwrap()
            .push((domain.to_string(), client, trigger, prefetch));
        Box::pin(async move { (self.script)() })
    }
}

fn the_run() -> Run {
    Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec!["signed".into()],
            evidence: vec![],
        }],
        blocks: vec![],
    }
}

fn the_diff() -> Diff {
    Diff {
        checks: vec![],
        evidence: vec![],
        grade: GradeChange {
            from: Grade::B,
            rescored: None,
            to: Grade::A,
        },
        engine_changed: false,
    }
}

fn sections() -> Vec<SectionEvent> {
    vec![
        SectionEvent {
            protocol: Protocol::Dns,
            outcome: SectionOutcome::Measured {
                checks: vec![CheckResult {
                    id: CheckId::parse("dns.dnssec_valid").unwrap(),
                    status: Status::Pass,
                    findings: vec!["signed".into()],
                    evidence: vec![],
                }],
                evidence: vec![],
            },
        },
        SectionEvent {
            protocol: Protocol::Tls,
            outcome: SectionOutcome::NotApplicable {
                reason: "no web".into(),
            },
        },
    ]
}

fn admitted(recorded: Option<Recorded>) -> Script {
    Box::new(move || {
        let mut events: Vec<RunEvent> = sections().into_iter().map(RunEvent::Section).collect();
        events.push(RunEvent::Done(the_run()));
        events.push(RunEvent::Recorded(recorded.clone()));
        RunStart::Admitted(Box::pin(futures::stream::iter(events)))
    })
}

fn refusal(reason: RefusalReason, retry: u64) -> Script {
    Box::new(move || {
        RunStart::Refused(RunRefusal {
            reason,
            retry_after_s: retry,
            last_run_ago_s: None,
            last_incomplete: false,
        })
    })
}

fn setup(script: Script, config: ApiConfig) -> (Router, Arc<StubRuns>) {
    let stub = Arc::new(StubRuns {
        script,
        calls: Mutex::new(vec![]),
    });
    let app = netray_web::api::router(ApiState::new(config, stub.clone()));
    (app, stub)
}

struct Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn post(app: &Router, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut req = Request::post(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8(bytes.to_vec()).unwrap(),
    }
}

fn json_of(r: &Reply) -> Value {
    serde_json::from_str(&r.body).unwrap_or(Value::Null)
}

fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

#[tokio::test]
async fn post_runs_calls_start_with_the_validated_domain_and_api_trigger() {
    let (app, stub) = setup(admitted(None), ApiConfig::default());
    let r = post(&app, "/domains/Example.COM./runs", &[], "").await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = post(
        &app,
        "/domains/Example.COM./runs",
        &[("sec-purpose", "prefetch")],
        "",
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let r = post(
        &app,
        "/domains/Example.COM./runs",
        &[("content-type", "application/json")],
        r#"{"dkim_selectors":["s1"]}"#,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);

    let calls = stub.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    let shape: Vec<(&str, RunTrigger, bool)> = calls
        .iter()
        .map(|(d, _, t, p)| (d.as_str(), *t, *p))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("example.com", RunTrigger::Api, false),
            ("example.com", RunTrigger::Api, true),
            ("example.com", RunTrigger::Api, false),
        ]
    );
}

#[tokio::test]
async fn post_runs_json_answer_per_recorded_variant() {
    let diff = the_diff();
    let diff_json = serde_json::to_value(&diff).unwrap();
    let run_json = serde_json::to_value(the_run()).unwrap();
    let cases: Vec<(&str, Option<Recorded>, &str, Value, Value)> = vec![
        (
            "first",
            Some(Recorded::First {
                id: "Ab3dE6gH".into(),
            }),
            "first",
            json!("Ab3dE6gH"),
            Value::Null,
        ),
        (
            "identical",
            Some(Recorded::Identical {
                id: "Ab3dE6gH".into(),
            }),
            "identical",
            json!("Ab3dE6gH"),
            Value::Null,
        ),
        (
            "changed",
            Some(Recorded::Changed {
                id: "Ab3dE6gH".into(),
                diff: diff.clone(),
            }),
            "changed",
            json!("Ab3dE6gH"),
            diff_json.clone(),
        ),
        (
            "not_stored with diff",
            Some(Recorded::NotStored {
                diff: Some(diff.clone()),
            }),
            "not_stored",
            Value::Null,
            diff_json.clone(),
        ),
        (
            "recorded none",
            None,
            "not_stored",
            Value::Null,
            Value::Null,
        ),
    ];
    for (name, recorded, want_recorded, want_id, want_diff) in cases {
        let (app, _) = setup(admitted(recorded), ApiConfig::default());
        let r = post(&app, "/domains/example.com/runs", &[], "").await;
        assert_eq!(r.status, StatusCode::OK, "{name}: {}", r.body);
        assert_eq!(
            r.headers[header::CONTENT_TYPE].to_str().unwrap(),
            "application/json",
            "{name}"
        );
        let v = json_of(&r);
        assert_eq!(keys(&v), ["diff", "entry_id", "recorded", "run"], "{name}");
        assert_eq!(v["recorded"], want_recorded, "{name}");
        assert_eq!(v["entry_id"], want_id, "{name}");
        assert_eq!(v["diff"], want_diff, "{name}");
        assert_eq!(v["run"], run_json, "{name}");
    }
}

fn parse_sse(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .map(|block| {
            let mut event = String::new();
            let mut data = String::new();
            for line in block.lines() {
                if let Some(e) = line.strip_prefix("event:") {
                    event = e.trim().to_string();
                } else if let Some(d) = line.strip_prefix("data:") {
                    data.push_str(d.trim_start());
                }
            }
            (event, serde_json::from_str(&data).unwrap())
        })
        .collect()
}

#[tokio::test]
async fn post_runs_streams_sections_then_done_for_event_stream() {
    let (app, _) = setup(
        admitted(Some(Recorded::First {
            id: "Ab3dE6gH".into(),
        })),
        ApiConfig::default(),
    );
    let json_reply = post(&app, "/domains/example.com/runs", &[], "").await;
    let r = post(
        &app,
        "/domains/example.com/runs",
        &[("accept", "text/event-stream")],
        "",
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(
        r.headers[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let events = parse_sse(&r.body);
    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["section", "section", "done"]);
    for (_, data) in &events[..2] {
        assert_eq!(
            keys(data),
            ["blocks", "checks", "outcome", "protocol", "reason"]
        );
    }
    let first = &events[0].1;
    assert_eq!(first["protocol"], "dns");
    assert_eq!(first["outcome"], "measured");
    assert_eq!(first["reason"], Value::Null);
    let check = &first["checks"][0];
    assert_eq!(check["id"], "dns.dnssec_valid");
    assert_eq!(check["status"], "pass");
    assert_eq!(check["findings"], json!(["signed"]));
    assert!(check.get("evidence").is_none() || check["evidence"] == json!([]));
    let second = &events[1].1;
    assert_eq!(second["protocol"], "tls");
    assert_eq!(second["outcome"], "not_applicable");
    assert_eq!(second["reason"], "no web");
    assert_eq!(second["checks"], json!([]));
    assert_eq!(events[2].1, json_of(&json_reply));
}

#[tokio::test]
async fn post_runs_refusals_answer_429_with_code_and_retry_after() {
    let cases = [
        (RefusalReason::Throttle, 50, "THROTTLED", "50"),
        (RefusalReason::Budget, 60, "BUDGET_EXHAUSTED", "60"),
        (RefusalReason::IpLimit, 7, "RATE_LIMITED", "7"),
        (RefusalReason::GlobalLimit, 8, "RATE_LIMITED", "8"),
        (RefusalReason::Slots, 1, "BUSY", "1"),
    ];
    for (reason, retry, code, header_value) in cases {
        let (app, _) = setup(refusal(reason, retry), ApiConfig::default());
        let r = post(&app, "/domains/example.com/runs", &[], "").await;
        assert_eq!(
            r.status,
            StatusCode::TOO_MANY_REQUESTS,
            "{reason:?}: {}",
            r.body
        );
        assert_eq!(r.headers["retry-after"], header_value, "{reason:?}");
        let v = json_of(&r);
        assert_eq!(v["error"]["code"], code, "{reason:?}");
        assert!(v["error"]["message"].is_string(), "{reason:?}");
    }
}

#[tokio::test]
async fn post_runs_not_started_is_503_prefetch_refused_no_store() {
    let (app, _) = setup(Box::new(|| RunStart::NotStarted), ApiConfig::default());
    let r = post(
        &app,
        "/domains/example.com/runs",
        &[("sec-purpose", "prefetch")],
        "",
    )
    .await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    assert_eq!(json_of(&r)["error"]["code"], "PREFETCH_REFUSED");
    assert_eq!(r.headers[header::CACHE_CONTROL], "no-store");
}

#[tokio::test]
async fn post_runs_invalid_domain_is_400_and_never_starts() {
    let (app, stub) = setup(admitted(None), ApiConfig::default());
    let r = post(&app, "/domains/*/runs", &[], "").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    assert_eq!(json_of(&r)["error"]["code"], "DOMAIN_INVALID");
    assert!(stub.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn post_runs_is_not_behind_the_api_limiter() {
    let config = ApiConfig {
        per_ip_burst: 1,
        ..ApiConfig::default()
    };
    let (app, stub) = setup(admitted(None), config);
    for _ in 0..2 {
        let r = post(&app, "/domains/example.com/runs", &[], "").await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
    assert_eq!(stub.calls.lock().unwrap().len(), 2);
}
