// V2 Phase 7, operator decision 2026-10-18 (DNS band admission): the inspector's DNS band runs
// under the inspector job's admission, so the band's module instance charges no per-target limit
// of its own and never refuses inside the 200 stream. A per-target burst below the check's cost
// refuses the registry's module before any lookup; the admitted instance goes on to resolve.

use std::time::{Duration, Instant};

use netray_dns::{DnsModule, ModuleConfig};
use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};

const CONFIG: &str = "servers = [\"cloudflare\"]\n[limits]\nper_target_burst = 1\n";

async fn run(module: &DnsModule) -> SectionOutcome {
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_millis(500),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    module.run(&ctx, &Facts::default()).await
}

fn refused(outcome: &SectionOutcome) -> bool {
    matches!(outcome, SectionOutcome::Incomplete { reason } if reason.contains("rate limit"))
}

#[tokio::test]
async fn the_registry_module_refuses_over_its_per_target_limit() {
    let cfg: ModuleConfig = toml::from_str(CONFIG).expect("config parses");
    let module = DnsModule::new(cfg).await.expect("module builds");

    let outcome = run(&module).await;

    assert!(refused(&outcome), "{outcome:?}");
}

#[tokio::test]
async fn the_admitted_band_module_charges_no_per_target_limit() {
    let cfg: ModuleConfig = toml::from_str(CONFIG).expect("config parses");
    let module = DnsModule::new(cfg)
        .await
        .expect("module builds")
        .admitted_by_caller();

    for _ in 0..3 {
        let outcome = run(&module).await;
        assert!(!refused(&outcome), "the band refused: {outcome:?}");
    }
}
