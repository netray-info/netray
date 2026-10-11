// V2 Phase 3, C1 (R3(a), DNS): the module reports exactly the 10 `dns.*` catalogue IDs, each
// once; IDs prism does not report are `not_applicable` with the finding "not reported".
//
// Cargo needs on netray-dns: `[[test]] name = "module_v3"`, `path = "tests/module_v3.rs"`,
// `required-features = ["testing"]`.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use netray_dns::testing;
use netray_engine::{Domain, Facts, RunContext, RunOptions, SectionOutcome, catalogue};
use netray_model::{Protocol, Status};

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/prism.sse");

fn catalogue_dns_ids() -> BTreeSet<String> {
    catalogue()
        .entries()
        .iter()
        .filter(|s| s.protocol == Protocol::Dns)
        .map(|s| s.id.to_string())
        .collect()
}

#[tokio::test]
async fn dns_module_reports_exactly_the_catalogue_ids() {
    let want = catalogue_dns_ids();
    assert_eq!(want.len(), 10, "catalogue holds 10 dns checks");

    let module = testing::golden_module(GOLDEN);
    let declared: Vec<String> = module.checks().iter().map(|c| c.to_string()).collect();
    assert_eq!(
        declared.len(),
        want.len(),
        "checks() has no duplicates: {declared:?}"
    );
    assert_eq!(
        declared.iter().cloned().collect::<BTreeSet<_>>(),
        want,
        "checks()"
    );

    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let SectionOutcome::Measured { checks, .. } = module.run(&ctx, &Facts::default()).await else {
        panic!("expected Measured");
    };

    let got: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    assert_eq!(got.len(), want.len(), "each ID once: {got:?}");
    assert_eq!(
        got.iter().cloned().collect::<BTreeSet<_>>(),
        want,
        "outcome IDs"
    );
    assert!(!got.iter().any(|id| id == "dns.infrastructure"));

    for c in &checks {
        if c.status == Status::NotApplicable {
            assert_eq!(c.findings, vec!["not reported".to_string()], "{}", c.id);
        }
    }
}
