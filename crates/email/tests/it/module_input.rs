// The email module applies beacon's route-level input checks (domain shape, DKIM selector count
// and syntax) before any lookup, and refuses `max_concurrent = 0` at config parse time.
// Every case is refused before the module touches the network.

use std::time::{Duration, Instant};

use netray_email::{EmailModule, ModuleConfig};
use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};

async fn run(domain: &str, selectors: Option<Vec<String>>) -> SectionOutcome {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 5").unwrap();
    let module = EmailModule::new(cfg).await.expect("module builds");
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new(domain),
        options: RunOptions {
            dkim_selectors: selectors,
        },
    };
    module.run(&ctx, &Facts::default()).await
}

fn reason(outcome: SectionOutcome) -> String {
    match outcome {
        SectionOutcome::Incomplete { reason } => reason,
        other => panic!("expected Incomplete, got {other:?}"),
    }
}

#[tokio::test]
async fn module_refuses_invalid_input_with_beacon_messages() {
    let r = reason(run("localhost", None).await);
    assert!(
        r.contains("domain must have at least two labels"),
        "too few labels: {r}"
    );

    let r = reason(run("-a.example.com", None).await);
    assert!(r.contains("label starts with a hyphen"), "hyphen: {r}");

    let six: Vec<String> = (0..6).map(|i| format!("sel{i}")).collect();
    let r = reason(run("example.com", Some(six)).await);
    assert!(
        r.contains("too many DKIM selectors (max 5)"),
        "too many selectors: {r}"
    );

    let r = reason(run("example.com", Some(vec!["bad_sel!".to_string()])).await);
    assert!(r.contains("invalid DKIM selector"), "bad selector: {r}");
}

#[test]
fn module_config_refuses_zero_max_concurrent() {
    assert!(toml::from_str::<ModuleConfig>("[inspections]\nmax_concurrent = 0").is_err());
}
