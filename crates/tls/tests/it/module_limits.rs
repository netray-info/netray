// V2 Phase 4, criterion C6: the TLS module keeps tlsight's per-target limit (keyed by host,
// cost = ports x addresses, `[limits]` keys as in tlsight's config) and tlsight's input parsing.
// Offline: the target is the IP literal 127.0.0.1. `parse_input` accepts it as an IP target, it
// resolves to itself without DNS (cost 1 per port), and the target policy refuses it, so a run
// that passes the limiter ends in "blocked target"; the over-limit run ends in
// "rate limited (per_target)" (AppError Display), and a parse refusal in "invalid hostname: ...".
// Needs `netray-engine` and `toml` as dev-dependencies of netray-tls.

use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_tls::{ModuleConfig, TlsModule};

fn ctx(domain: &str) -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new(domain),
        options: RunOptions::default(),
    }
}

async fn reason(module: &TlsModule, domain: &str) -> String {
    match module.run(&ctx(domain), &Facts::default()).await {
        SectionOutcome::Incomplete { reason } => reason,
        other => panic!("expected Incomplete for {domain}, got {other:?}"),
    }
}

#[tokio::test]
async fn third_run_for_one_host_hits_the_per_target_limit() {
    let cfg: ModuleConfig =
        toml::from_str("[limits]\nper_target_per_minute = 2\nper_target_burst = 2\n")
            .expect("ModuleConfig accepts tlsight's [limits] per-target keys");
    let module = TlsModule::new(cfg).await.expect("module builds");

    for n in 1..=2 {
        let r = reason(&module, "127.0.0.1").await;
        assert!(
            r.contains("blocked target"),
            "run {n} reaches the target policy: {r}"
        );
    }
    let third = reason(&module, "127.0.0.1").await;
    assert!(
        third.contains("rate limited (per_target)"),
        "third run is limited: {third}"
    );

    let other = reason(&module, "127.0.0.2").await;
    assert!(
        !other.contains("rate limited"),
        "a different host has its own budget: {other}"
    );
}

#[tokio::test]
async fn module_refuses_invalid_input_with_tlsight_messages() {
    let cfg: ModuleConfig = toml::from_str("[limits]\nmax_ports = 3").unwrap();
    let module = TlsModule::new(cfg).await.expect("module builds");

    let r = reason(&module, "localhost").await;
    assert!(
        r.contains("invalid hostname: single-label hostname not allowed"),
        "single label: {r}"
    );

    let r = reason(&module, "*.example.com").await;
    assert!(
        r.contains("invalid hostname: wildcards are not allowed"),
        "wildcard: {r}"
    );

    let r = reason(&module, "example.com:99999").await;
    assert!(r.contains("invalid port"), "port: {r}");

    let r = reason(&module, "example.com:1,2,3,4").await;
    assert!(r.contains("too many ports"), "max_ports: {r}");
}
