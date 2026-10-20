//! Review findings on v2-inspector-shell Phase 7 (requirement 24): the page and the job read the
//! email input one way (domain, then selectors split on spaces and commas), a leading separator is
//! no empty domain, and an IDN reaches the inspection in its UTS #46 form.
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, header};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_email::security::RateLimitState;
use netray_email::testing::{example_dns, stub_inspector};
use netray_email::{EmailInspect, EmailInspectRequest, EmailJob, EmailRefusal, InspectSlots};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};
use tower::ServiceExt;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
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

/// Forwards to the real stub inspector and records each request's domain and selectors.
struct Recording {
    inner: Box<dyn EmailInspect>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl EmailInspect for Recording {
    fn inspect(
        &self,
        req: &EmailInspectRequest,
        client: IpAddr,
        limits: &RateLimitState,
        slots: &InspectSlots,
    ) -> Result<EmailJob, EmailRefusal> {
        let job = self.inner.inspect(req, client, limits, slots)?;
        self.seen
            .lock()
            .unwrap()
            .push(format!("{} {:?}", req.domain, req.dkim_selectors));
        Ok(job)
    }
}

async fn run(body: &str) -> (Vec<String>, String) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns)).with_parts(ApiParts {
        email: Some(Arc::new(Recording {
            inner: Box::new(stub_inspector(example_dns())),
            seen: seen.clone(),
        })),
        ..Default::default()
    });
    let app = inspect_router(InspectDeps::new(
        Arc::new(InspectJobs::new(state)),
        InspectConfig::default(),
    ));
    let resp = app
        .oneshot(
            Request::post("/inspect/email/run")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).replace("\\\"", "\"");
    let seen = seen.lock().unwrap().clone();
    (seen, text)
}

#[tokio::test]
async fn a_comma_after_the_domain_is_a_selector_separator_everywhere() {
    let (seen, stream) = run("d=example.com%2Cs1").await;
    assert_eq!(seen, vec![r#"example.com ["s1"]"#.to_string()]);
    assert!(
        stream.contains(r#"class="inspector-backlink" href="/example.com""#),
        "{stream}"
    );
}

#[tokio::test]
async fn a_leading_separator_is_no_empty_domain() {
    let (seen, _) = run("d=%20example.com").await;
    assert_eq!(seen, vec!["example.com []".to_string()]);
}

#[tokio::test]
async fn an_idn_reaches_the_inspection_in_its_ascii_form() {
    let (seen, _) = run("d=b%C3%BCcher.example.com").await;
    assert_eq!(seen, vec!["xn--bcher-kva.example.com []".to_string()]);
}
