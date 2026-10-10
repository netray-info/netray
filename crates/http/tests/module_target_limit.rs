// V2: the HTTP module keeps spectra's per-target limit (keyed by host, `[limits]` keys as in
// spectra's config). Fails until `ModuleConfig` accepts `limits` and `HttpModule` applies it.
// Offline: 127.0.0.1 is refused by the target policy, so a run that passes the limiter ends in
// a "blocked target" Incomplete, distinguishable from the "rate limited (per_target)" one.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};
use netray_http::{HttpModule, ModuleConfig};

fn ctx(domain: &str) -> RunContext {
    RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new(domain),
        options: RunOptions::default(),
    }
}

async fn reason(module: &HttpModule, domain: &str, facts: &Facts) -> String {
    match module.run(&ctx(domain), facts).await {
        SectionOutcome::Incomplete { reason } => reason,
        other => panic!("expected Incomplete for {domain}, got {other:?}"),
    }
}

#[tokio::test]
async fn third_run_for_one_domain_hits_the_per_target_limit() {
    let cfg: ModuleConfig =
        toml::from_str("[limits]\nper_target_per_minute = 2\nper_target_burst = 2\n")
            .expect("ModuleConfig accepts spectra's [limits] per-target keys");
    let module = HttpModule::new(cfg).expect("module builds");
    let facts = Facts {
        a: vec![Ipv4Addr::LOCALHOST],
        ..Facts::default()
    };

    for n in 1..=2 {
        let r = reason(&module, "example.com", &facts).await;
        assert!(
            r.contains("blocked target"),
            "run {n} reaches the target policy: {r}"
        );
    }
    let third = reason(&module, "example.com", &facts).await;
    assert!(
        third.contains("rate limited (per_target)"),
        "third run is limited: {third}"
    );

    let other = reason(&module, "example.org", &facts).await;
    assert!(
        !other.contains("rate limited"),
        "a different domain has its own budget: {other}"
    );
}
