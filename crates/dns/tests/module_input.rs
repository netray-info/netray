// The DNS module applies prism's check-route input policy before any lookup: domain validation,
// `@system` refusal (`allow_system_resolvers = false`, SC17) and `max_servers`. Each refusal is an
// `Incomplete` carrying prism's `ApiError` text, issued before the module touches the network
// (every server named here would otherwise need a live resolver).
// Needs `netray-engine` and `toml` as dev-dependencies of netray-dns.

use std::time::{Duration, Instant};

use netray_dns::error::ApiError;
use netray_dns::{DnsModule, ModuleConfig};
use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome};

async fn run(config: &str, domain: &str) -> SectionOutcome {
    let cfg: ModuleConfig = toml::from_str(config).expect("config parses");
    let module = DnsModule::new(cfg).await.expect("module builds");
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new(domain),
        options: RunOptions::default(),
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
async fn system_resolver_is_refused_when_disallowed() {
    let r = reason(
        run(
            "servers = [\"system\"]\n[dns]\nallow_system_resolvers = false\n",
            "example.com",
        )
        .await,
    );
    assert!(
        r.contains(&ApiError::SystemResolversDisabled.to_string()),
        "{r}"
    );
    assert!(r.contains("system resolvers disabled"), "{r}");
}

#[tokio::test]
async fn invalid_domain_is_refused_with_prisms_text() {
    let cfg = "servers = [\"cloudflare\"]\n";

    let r = reason(run(cfg, "").await);
    assert!(r.contains("invalid domain: empty domain"), "empty: {r}");

    let long = format!("{}.example.com", "a".repeat(250));
    let r = reason(run(cfg, &long).await);
    assert!(
        r.contains("domain exceeds maximum length of 253 characters"),
        "too long: {r}"
    );
}

#[tokio::test]
async fn servers_over_max_servers_are_refused_with_prisms_text() {
    let r = reason(
        run(
            "servers = [\"cloudflare\", \"google\", \"quad9\"]\n[limits]\nmax_servers = 2\n",
            "example.com",
        )
        .await,
    );
    assert!(r.contains("too many servers: 3 exceeds limit of 2"), "{r}");
}
