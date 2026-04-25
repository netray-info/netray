/// Live integration tests against real backends.
///
/// Every test is gated behind:
///   1. `#[ignore]` — skipped by default unless `cargo test -- --ignored`
///   2. `LENS_LIVE_TESTS=1` env var — checked at runtime inside each test
///
/// Run with:
///   LENS_LIVE_TESTS=1 cargo test --test reference_domains -- --ignored --nocapture
use lens::check::run_check;
use lens::config::{
    BackendsConfig, CacheConfig, Config, EcosystemConfig, RateLimitConfig, ScoringConfig,
    ServerConfig, SiteConfig,
};
use lens::state::AppState;

// ---------------------------------------------------------------------------
// Infrastructure
// ---------------------------------------------------------------------------

fn live_tests_enabled() -> bool {
    std::env::var("LENS_LIVE_TESTS").as_deref() == Ok("1")
}

fn live_state() -> AppState {
    let config = Config {
        server: ServerConfig {
            bind: ([0, 0, 0, 0], 8082).into(),
            metrics_bind: ([127, 0, 0, 1], 8090).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            dns: netray_common::backend::BackendConfig {
                url: Some("https://dns.netray.info".to_string()),
                timeout_ms: 20000,
                ..Default::default()
            },
            dns_servers: Vec::new(),
            tls: netray_common::backend::BackendConfig {
                url: Some("https://tls.netray.info".to_string()),
                timeout_ms: 20000,
                ..Default::default()
            },
            ip: netray_common::backend::BackendConfig {
                url: Some("https://ip.netray.info".to_string()),
                timeout_ms: 20000,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 200,
            global_burst: 40,
        },
        ecosystem: EcosystemConfig::default(),
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        telemetry: Default::default(),
    };
    AppState::new(config).expect("failed to build AppState for live tests")
}

// ---------------------------------------------------------------------------
// Public reference domains — assert grade range, not exact grade
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn google_com_scores_well() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "google.com").await;

    println!(
        "google.com grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );
    if let Some(Ok(dns)) = output.sections.get("dns") {
        println!("  dns checks: {} checks", dns.checks.len());
    }

    assert!(
        matches!(output.score.grade.as_str(), "A" | "A+"),
        "google.com expected A or A+, got {} (score={:.1})",
        output.score.grade,
        output.score.overall_percentage,
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn cloudflare_com_scores_well() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "cloudflare.com").await;

    println!(
        "cloudflare.com grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );

    assert!(
        matches!(output.score.grade.as_str(), "A" | "A+"),
        "cloudflare.com expected A or A+, got {} (score={:.1})",
        output.score.grade,
        output.score.overall_percentage,
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn github_com_scores_well() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "github.com").await;

    println!(
        "github.com grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );

    assert!(
        matches!(output.score.grade.as_str(), "A" | "A+"),
        "github.com expected A or A+, got {} (score={:.1})",
        output.score.grade,
        output.score.overall_percentage,
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn example_com_scores_below_a() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "example.com").await;

    println!(
        "example.com grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );
    if output.score.hard_fail_triggered {
        println!("  hard_fail_checks: {:?}", output.score.hard_fail_checks);
    }

    // IANA reference domain has minimal email/TLS config — must not score A or A+.
    assert!(
        !matches!(output.score.grade.as_str(), "A" | "A+"),
        "example.com must NOT score A or A+, got {} (score={:.1})",
        output.score.grade,
        output.score.overall_percentage,
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn letsencrypt_org_scores_well() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "letsencrypt.org").await;

    println!(
        "letsencrypt.org grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );

    assert!(
        matches!(output.score.grade.as_str(), "A" | "A+"),
        "letsencrypt.org expected A or A+, got {} (score={:.1})",
        output.score.grade,
        output.score.overall_percentage,
    );
}

// ---------------------------------------------------------------------------
// Own domains — baseline: complete without error, log grade for calibration
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn pustina_net_baseline() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "pustina.net").await;

    println!(
        "pustina.net grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );
    if let Some(Ok(dns)) = output.sections.get("dns") {
        println!("  dns checks: {} checks", dns.checks.len());
    }
    if output.score.hard_fail_triggered {
        println!("  hard_fail_checks: {:?}", output.score.hard_fail_checks);
    }

    // Baseline: check completes and returns a valid grade (not an internal error state).
    // Expected grade: TBD — fill in after first run.
    assert!(
        !output.score.grade.is_empty(),
        "pustina.net check must return a non-empty grade",
    );
    // All three backends must not time out simultaneously (at least one should respond).
    let all_timed_out = output.sections.values().all(|r| r.is_err());
    assert!(
        !all_timed_out,
        "pustina.net: all backends timed out or errored"
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn pustina_de_baseline() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "pustina.de").await;

    println!(
        "pustina.de grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );
    if let Some(Ok(dns)) = output.sections.get("dns") {
        println!("  dns checks: {} checks", dns.checks.len());
    }
    if output.score.hard_fail_triggered {
        println!("  hard_fail_checks: {:?}", output.score.hard_fail_checks);
    }

    assert!(
        !output.score.grade.is_empty(),
        "pustina.de check must return a non-empty grade",
    );
    let all_timed_out = output.sections.values().all(|r| r.is_err());
    assert!(
        !all_timed_out,
        "pustina.de: all backends timed out or errored"
    );
}

#[tokio::test]
#[ignore = "live test: requires LENS_LIVE_TESTS=1 and network access"]
async fn netray_info_baseline() {
    if !live_tests_enabled() {
        return;
    }
    let state = live_state();
    let output = run_check(&state, "netray.info").await;

    println!(
        "netray.info grade={} score={:.1} duration={}ms",
        output.score.grade, output.score.overall_percentage, output.duration_ms
    );
    if let Some(Ok(dns)) = output.sections.get("dns") {
        println!("  dns checks: {} checks", dns.checks.len());
    }
    if output.score.hard_fail_triggered {
        println!("  hard_fail_checks: {:?}", output.score.hard_fail_checks);
    }

    assert!(
        !output.score.grade.is_empty(),
        "netray.info check must return a non-empty grade",
    );
    let all_timed_out = output.sections.values().all(|r| r.is_err());
    assert!(
        !all_timed_out,
        "netray.info: all backends timed out or errored"
    );
}
