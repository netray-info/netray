//! Deadlines (spec grade-integrity, Phase 3, requirement 5; criteria C1, C3-C6).
//!
//! Real axum stubs serve the committed backend goldens, or stall on purpose; the email module
//! answers its golden at once, after a delay, or never. `AppState` is
//! built from a `Config` constructed here (not `Config::load`), the check is driven through
//! `run_check_with_deadline`. A timed-out section is `Err(SectionError::Timeout)` and makes
//! the score incomplete.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod common;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{StatusCode, header};
use axum::routing::{get, post};
use common::{email_golden, http_module, ip_golden, registry_with, slow};
use futures::StreamExt;
use lens::check::{CheckInput, CheckOutput, SectionError, run_check_with_deadline};
use lens::config::{
    BackendConfig, BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig,
    OgCardsConfig, RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;

#[derive(Clone, Copy)]
enum Behaviour {
    /// Serve the golden at once.
    Golden(&'static str),
    /// Serve the golden after a delay.
    GoldenAfter(&'static str, Duration),
    /// Never answer: no headers either.
    Never,
    /// Send headers and a first chunk, then stall forever.
    Stall(&'static str),
}

fn golden(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}")
}

async fn stub(
    path: &'static str,
    post_method: bool,
    content_type: &'static str,
    behaviour: Behaviour,
) -> String {
    let handler = move || async move {
        match behaviour {
            Behaviour::Golden(f) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type)],
                golden(f),
            )
                .into_response_(),
            Behaviour::GoldenAfter(f, d) => {
                tokio::time::sleep(d).await;
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, content_type)],
                    golden(f),
                )
                    .into_response_()
            }
            Behaviour::Never => {
                std::future::pending::<()>().await;
                unreachable!()
            }
            Behaviour::Stall(f) => {
                let full = golden(f);
                // Only a prefix: the first event (SSE) or the first bytes (JSON).
                let first: String = match full.find("\n\n") {
                    Some(i) if content_type == "text/event-stream" => full[..i + 2].to_string(),
                    _ => full.chars().take(20).collect(),
                };
                let stream =
                    futures::stream::once(
                        async move { Ok::<Bytes, Infallible>(Bytes::from(first)) },
                    )
                    .chain(futures::stream::pending());
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, content_type)],
                    Body::from_stream(stream),
                )
                    .into_response_()
            }
        }
    };
    let route = if post_method {
        post(handler)
    } else {
        get(handler)
    };
    serve(Router::new().route(path, route)).await
}

/// Tiny shim so all arms return the same type.
trait IntoResponse_ {
    fn into_response_(self) -> axum::response::Response;
}
impl<T: axum::response::IntoResponse> IntoResponse_ for T {
    fn into_response_(self) -> axum::response::Response {
        axum::response::IntoResponse::into_response(self)
    }
}

struct Setup {
    dns: Behaviour,
    tls: Behaviour,
    email: Behaviour,
    timeouts_ms: [u64; 5], // dns, tls, http, email, ip
}

impl Setup {
    fn fast() -> Self {
        Self {
            dns: Behaviour::Golden("prism.sse"),
            tls: Behaviour::Golden("tlsight-inspect.json"),
            email: Behaviour::Golden("beacon.sse"),
            timeouts_ms: [5000, 5000, 5000, 5000, 2000],
        }
    }
}

fn backend(url: String, timeout_ms: u64) -> BackendConfig {
    BackendConfig {
        url: Some(url),
        timeout_ms,
        ..Default::default()
    }
}

async fn state(s: Setup) -> AppState {
    let t = s.timeouts_ms;
    let config = Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: backend(
                stub("/api/check", true, "text/event-stream", s.dns).await,
                t[0],
            ),
            dns_servers: Vec::new(),
            tls: backend(
                stub("/api/inspect", false, "application/json", s.tls).await,
                t[1],
            ),
            // The IP section runs in-process; only its deadline comes from the config.
            ip: BackendConfig {
                timeout_ms: t[4],
                ..Default::default()
            },
            // The HTTP section runs in-process; only its deadline comes from the config.
            http: Some(BackendConfig {
                timeout_ms: t[2],
                ..Default::default()
            }),
            // The email section runs in-process; only its deadline comes from the config.
            email: Some(BackendConfig {
                timeout_ms: t[3],
                ..Default::default()
            }),
        },
        ecosystem: EcosystemConfig::default(),
        telemetry: Default::default(),
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 10,
            per_ip_burst: 3,
            global_per_minute: 100,
            global_burst: 20,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        modules: Default::default(),
        og_cards: OgCardsConfig::default(),
        snapshots: SnapshotsConfig::default(),
    };
    let email = match s.email {
        Behaviour::Golden(f) => email_golden(f),
        Behaviour::GoldenAfter(f, d) => slow(email_golden(f), Some(d)),
        Behaviour::Never => slow(email_golden("beacon.sse"), None),
        // In-process there is no first chunk: a stalled stream is a module that never finishes.
        Behaviour::Stall(f) => slow(email_golden(f), None),
    };
    let registry = registry_with(
        http_module(Some("spectra-inspect.json")),
        email,
        ip_golden("ifconfig-json.json"),
    );
    AppState::with_registry(config, registry).expect("state builds")
}

fn input() -> CheckInput {
    CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
        client_ip: None,
        request_id: None,
    }
}

/// Run the check, guarded by an outer timeout so a hang fails instead of blocking.
async fn run(state: &AppState, deadline: Duration, guard: Duration) -> (CheckOutput, Duration) {
    let start = Instant::now();
    let out = tokio::time::timeout(guard, run_check_with_deadline(state, input(), deadline))
        .await
        .unwrap_or_else(|_| panic!("check still running after {guard:?}: a deadline is missing"));
    (out, start.elapsed())
}

fn is_timeout(r: &Result<lens::backends::BackendResult, SectionError>) -> bool {
    matches!(r, Err(SectionError::Timeout))
}

/// C3: the hard deadline keeps the sections that finished and marks only the stuck one.
#[tokio::test]
async fn hard_deadline_keeps_finished_sections_and_times_out_the_stuck_one() {
    let mut s = Setup::fast();
    s.dns = Behaviour::GoldenAfter("prism.sse", Duration::from_millis(100));
    s.email = Behaviour::Never;
    let st = state(s).await;

    let (out, elapsed) = run(&st, Duration::from_secs(1), Duration::from_secs(8)).await;

    assert!(
        out.sections["dns"].is_ok(),
        "dns finished before the deadline"
    );
    assert!(
        is_timeout(&out.sections["email"]),
        "email timed out: {:?}",
        out.sections["email"].as_ref().err()
    );
    assert!(
        !out.score.complete,
        "a timed-out section makes the score incomplete"
    );
    assert!(
        elapsed < Duration::from_millis(1500),
        "returned in {elapsed:?}"
    );
}

/// C4: an email module that never finishes is bounded by one email budget (timeout_ms).
#[tokio::test]
async fn email_send_and_stream_share_one_timeout_budget() {
    let mut s = Setup::fast();
    s.timeouts_ms[3] = 1000;
    s.email = Behaviour::Stall("beacon.sse");
    let st = state(s).await;

    let (out, elapsed) = run(&st, Duration::from_secs(20), Duration::from_secs(5)).await;

    assert!(
        out.sections["email"].is_err(),
        "email without summary is an error"
    );
    assert!(
        is_timeout(&out.sections["email"]),
        "email stall reports Timeout: {:?}",
        out.sections["email"].as_ref().err()
    );
    assert!(
        elapsed < Duration::from_millis(1600),
        "email took {elapsed:?}, budget is 1 s"
    );
}

/// C5: a body that stalls after the headers is bounded by the tls timeout.
#[tokio::test]
async fn tls_body_that_stalls_after_headers_is_bounded_by_timeout() {
    let mut s = Setup::fast();
    s.timeouts_ms[1] = 1000;
    s.tls = Behaviour::Stall("tlsight-inspect.json");
    let st = state(s).await;

    let (out, elapsed) = run(&st, Duration::from_secs(20), Duration::from_secs(5)).await;

    assert!(out.sections["tls"].is_err(), "stalled tls body is an error");
    assert!(
        is_timeout(&out.sections["tls"]),
        "stalled tls body reports Timeout: {:?}",
        out.sections["tls"].as_ref().err()
    );
    assert!(
        elapsed < Duration::from_millis(1600),
        "tls took {elapsed:?}, budget is 1 s"
    );
}

/// C6: the email backend honours `timeout_ms` from config instead of a fixed 15 s.
#[tokio::test]
async fn email_backend_honours_configured_timeout_instead_of_fixed_15s() {
    let mut s = Setup::fast();
    s.timeouts_ms[3] = 1000;
    s.email = Behaviour::GoldenAfter("beacon.sse", Duration::from_secs(2));
    let st = state(s).await;

    let (out, elapsed) = run(&st, Duration::from_secs(20), Duration::from_secs(8)).await;

    assert!(
        is_timeout(&out.sections["email"]),
        "a 2 s answer exceeds timeout_ms = 1000: {:?}",
        out.sections["email"].as_ref().err()
    );
    assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
}
