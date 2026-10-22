//! V2 spec v2-ui-polish, requirement 5 (C4, C12, C13): a refused run keeps the page's frame
//! (hidden h1, example chips) and tones its notice; an invalid-input notice carries no tone.
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use netray_web::pages::{
    InspectConfig, InspectDeps, InspectorJobs, Job, JobRequest, Refusal, Tool, inspect_router,
};
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Clone, Copy)]
enum Mode {
    RateLimited,
    Busy,
    TimedOut,
    InvalidStart,
    InvalidGate,
}

struct Stub {
    mode: Mode,
    hold: Arc<Notify>,
}

impl InspectorJobs for Stub {
    fn admit(&self, _: Tool, _: IpAddr, _: &str) -> Result<(), Duration> {
        match self.mode {
            Mode::RateLimited => Err(Duration::from_secs(20)),
            _ => Ok(()),
        }
    }

    fn start(&self, _: JobRequest) -> Result<Job, String> {
        match self.mode {
            Mode::InvalidStart => Err("not a valid host name".into()),
            Mode::InvalidGate => Ok(Job::new(vec![]).with_gate(Box::pin(async {
                Err(Refusal::Invalid("not a valid host name".into()))
            }))),
            Mode::TimedOut => {
                let hold = self.hold.clone();
                Ok(Job::new(vec![]).with_gate(Box::pin(async move {
                    hold.notified().await;
                    Ok(())
                })))
            }
            _ => {
                let hold = self.hold.clone();
                Ok(Job::new(vec![netray_web::pages::JobPart {
                    label: "handshake".into(),
                    output: Box::pin(async move {
                        hold.notified().await;
                        Ok(vec![])
                    }),
                }]))
            }
        }
    }
}

fn post() -> Request<Body> {
    Request::post("/inspect/tls/run")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("h=example.com"))
        .unwrap()
}

async fn text(res: axum::response::Response) -> String {
    String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

/// The HTML of the SSE fragment for `inspect-result`.
fn result_fragment(body: &str) -> String {
    for event in body.split("\n\n") {
        let Some(data) = event.lines().find_map(|l| l.strip_prefix("data: ")) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };
        if v["id"] == "inspect-result" {
            return v["html"].as_str().unwrap().to_string();
        }
    }
    panic!("no inspect-result fragment: {body}");
}

fn examples_list(html: &str) -> String {
    let start = html
        .find("<ul class=\"inspector-examples\">")
        .unwrap_or_else(|| panic!("no examples list: {html}"));
    let end = html[start..].find("</ul>").expect("list closes") + start + 5;
    html[start..end].to_string()
}

#[tokio::test]
async fn refused_run_keeps_frame_and_tones_the_notice() {
    let cases = [
        (
            Mode::RateLimited,
            "too many requests; try again in 20 s",
            true,
        ),
        (Mode::Busy, "busy, try again in a moment", true),
        (Mode::TimedOut, "timed out", true),
        (Mode::InvalidStart, "not a valid host name", false),
        (Mode::InvalidGate, "not a valid host name", false),
    ];

    let idle = {
        let stub = Arc::new(Stub {
            mode: Mode::Busy,
            hold: Arc::new(Notify::new()),
        });
        let app = inspect_router(InspectDeps::new(stub, InspectConfig::default()));
        let res = app
            .oneshot(Request::get("/inspect/tls").body(Body::empty()).unwrap())
            .await
            .unwrap();
        examples_list(&text(res).await)
    };
    assert!(
        idle.contains("class=\"inspector-examples__chip\""),
        "{idle}"
    );

    for (mode, message, warn) in cases {
        let hold = Arc::new(Notify::new());
        let stub = Arc::new(Stub {
            mode,
            hold: hold.clone(),
        });
        let mut config = InspectConfig::default();
        config.tls.max_concurrent = 1;
        config.tls.deadline_ms = 100;
        let app = inspect_router(InspectDeps::new(stub, config));

        // For Busy, a first job holds the slot while its stream stays open.
        let mut first_body = None;
        if matches!(mode, Mode::Busy) {
            let first = app.clone().oneshot(post()).await.unwrap();
            let mut body = first.into_body();
            let _ = body.frame().await.expect("first frame").unwrap();
            first_body = Some(body);
        }

        let res = app.clone().oneshot(post()).await.unwrap();
        let body = text(res).await;
        let html = result_fragment(&body);

        assert!(
            html.contains("<h1 class=\"visually-hidden\">TLS inspector</h1>"),
            "{message}: {html}"
        );
        let notice = if warn {
            format!(
                "<p class=\"inspector-notice\" role=\"status\" data-tone=\"warn\">{message}</p>"
            )
        } else {
            format!("<p class=\"inspector-notice\" role=\"status\">{message}</p>")
        };
        assert!(html.contains(&notice), "{message}: want {notice} in {html}");
        if warn {
            assert_eq!(examples_list(&html), idle, "{message}");
        } else {
            assert!(!html.contains("data-tone"), "{message}: {html}");
        }

        hold.notify_waiters();
        drop(first_body);
    }
}

#[test]
fn stylesheet_cards_the_notice_and_marks_the_warn_tone() {
    let css = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/static/inspector_shell.css"
    ))
    .unwrap();
    let rule = |selector: &str| -> String {
        let start = css
            .find(&format!("{selector} {{"))
            .unwrap_or_else(|| panic!("no rule for {selector}"));
        let end = css[start..].find('}').unwrap() + start;
        css[start..end].to_string()
    };
    let warn = rule(".inspector-notice[data-tone=\"warn\"]");
    assert!(
        warn.contains("border-inline-start") && warn.contains("var(--status-warn-mark)"),
        "{warn}"
    );
    let card = rule(".inspector-notice");
    assert!(
        card.contains("border:") && card.contains("var(--color-line)"),
        "{card}"
    );
    assert!(card.contains("background: var(--color-surface)"), "{card}");
}
