//! Deadlines (spec grade-integrity, Phase 3, requirement 5; criteria C1, C3-C6).
//!
//! The DNS, TLS and email modules answer their committed golden at once, after a delay, or never.
//! The apex state is built from a default `ApexConfig` and an explicit section plan (lens's
//! `timeout_ms` per section, resolve budget 2000 ms), the check is driven through
//! `run_check_streaming` with the deadline under test. A timed-out section is
//! `Err(SectionError::Timeout)` and makes the score incomplete.
//!
//! Copied from crates/lens/tests/deadlines.rs (spec v2-cutover, Phase 6); nothing dropped
//! (lens's `run_check_with_deadline` is `netray_engine::domain_run::run_check_streaming` over
//! the apex state's registry, plan and resolve budget).

use std::time::{Duration, Instant};

use crate::common;

use common::{
    dns_golden_raw, email_golden, facts_golden_raw, http_module, ip_golden, registry_with, slow,
    tls_golden,
};
use netray_engine::Module;
use netray_engine::domain_run::{
    BackendResult, CheckInput, CheckOutput, SectionError, run_check_streaming,
};
use netray_model::Protocol;
use netray_web::admission::ApexState;
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;

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

fn state(s: Setup) -> ApexState {
    let t = s.timeouts_ms;
    let config = ApexConfig {
        cache_enabled: false,
        cache_window_seconds: 300,
        limits: AdmissionLimits {
            rate_limit_per_ip_per_minute: 10,
            rate_limit_per_ip_burst: 3,
            rate_limit_global_per_minute: 100,
            rate_limit_global_burst: 20,
            ..ApexConfig::default().limits
        },
        ..ApexConfig::default()
    };
    // Every section runs in-process; only its deadline comes from the configuration.
    let ms = Duration::from_millis;
    let plan = vec![
        (Protocol::Dns, ms(t[0])),
        (Protocol::Tls, ms(t[1])),
        (Protocol::Http, ms(t[2])),
        (Protocol::Email, ms(t[3])),
        (Protocol::Ip, ms(t[4])),
    ];
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
    ApexState::new(config, registry, plan, ms(2000), common::profile())
}

fn input() -> CheckInput {
    CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
    }
}

/// Run the check, guarded by an outer timeout so a hang fails instead of blocking.
async fn run(state: &ApexState, deadline: Duration, guard: Duration) -> (CheckOutput, Duration) {
    let start = Instant::now();
    let check = run_check_streaming(
        &state.registry,
        &state.plan,
        state.resolve_budget,
        &state.scoring_profile,
        input(),
        deadline,
        |_| {},
    );
    let (out, _) = tokio::time::timeout(guard, check)
        .await
        .unwrap_or_else(|_| panic!("check still running after {guard:?}: a deadline is missing"));
    (out, start.elapsed())
}

fn is_timeout(r: &Result<BackendResult, SectionError>) -> bool {
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
