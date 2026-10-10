//! Deadlines (spec grade-integrity, Phase 3, requirement 5; criteria C1, C3-C6).
//!
//! The DNS, TLS and email modules answer their committed golden at once, after a delay, or never. `AppState` is
//! built from a `Config` constructed here (not `Config::load`), the check is driven through
//! `run_check_with_deadline`. A timed-out section is `Err(SectionError::Timeout)` and makes
//! the score incomplete.

use std::time::{Duration, Instant};

mod common;

use common::{
    dns_golden_raw, email_golden, facts_golden_raw, http_module, ip_golden, registry_with, slow,
    tls_golden,
};
use lens::check::{CheckInput, CheckOutput, SectionError, run_check_with_deadline};
use lens::config::{
    BackendConfig, BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig,
    OgCardsConfig, RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::state::AppState;
use netray_engine::Module;

#[derive(Clone, Copy)]
enum Behaviour {
    /// Answer with the golden at once.
    Golden(&'static str),
    /// Answer with the golden after a delay.
    GoldenAfter(&'static str, Duration),
    /// Never answer.
    Never,
    /// Never finish.
    Stall(&'static str),
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

fn state(s: Setup) -> AppState {
    let t = s.timeouts_ms;
    let config = Config {
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            resolve_timeout_ms: 2000,
            // The DNS section runs in-process; only its deadline comes from the config.
            dns: BackendConfig {
                timeout_ms: t[0],
                ..Default::default()
            },
            // The TLS section runs in-process; only its deadline comes from the config.
            tls: BackendConfig {
                timeout_ms: t[1],
                ..Default::default()
            },
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
    let module = |b: Behaviour, never: &str, golden: fn(&str) -> Box<dyn Module>| match b {
        Behaviour::Golden(f) => golden(f),
        Behaviour::GoldenAfter(f, d) => slow(golden(f), Some(d)),
        Behaviour::Never => slow(golden(never), None),
        // In-process there is no first chunk: a stalled stream is a module that never finishes.
        Behaviour::Stall(f) => slow(golden(f), None),
    };
    let registry = registry_with(
        module(s.dns, "prism.sse", dns_golden_raw),
        facts_golden_raw("prism.sse"),
        http_module(Some("spectra-inspect.json")),
        module(s.email, "beacon.sse", email_golden),
        ip_golden("ifconfig-json.json"),
        module(s.tls, "tlsight-inspect.json", tls_golden),
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

fn is_timeout(r: &Result<lens::modules::BackendResult, SectionError>) -> bool {
    matches!(r, Err(SectionError::Timeout))
}

/// C3: the hard deadline keeps the sections that finished and marks only the stuck one.
#[tokio::test]
async fn hard_deadline_keeps_finished_sections_and_times_out_the_stuck_one() {
    let mut s = Setup::fast();
    s.dns = Behaviour::GoldenAfter("prism.sse", Duration::from_millis(100));
    s.email = Behaviour::Never;
    let st = state(s);

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
    let st = state(s);

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

/// C5: a TLS module that never finishes is bounded by the tls timeout.
#[tokio::test]
async fn tls_module_that_never_finishes_is_bounded_by_timeout() {
    let mut s = Setup::fast();
    s.timeouts_ms[1] = 1000;
    s.tls = Behaviour::Stall("tlsight-inspect.json");
    let st = state(s);

    let (out, elapsed) = run(&st, Duration::from_secs(20), Duration::from_secs(5)).await;

    assert!(
        out.sections["tls"].is_err(),
        "a stalled tls module is an error"
    );
    assert!(
        is_timeout(&out.sections["tls"]),
        "a stalled tls module reports Timeout: {:?}",
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
    let st = state(s);

    let (out, elapsed) = run(&st, Duration::from_secs(20), Duration::from_secs(8)).await;

    assert!(
        is_timeout(&out.sections["email"]),
        "a 2 s answer exceeds timeout_ms = 1000: {:?}",
        out.sections["email"].as_ref().err()
    );
    assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
}
