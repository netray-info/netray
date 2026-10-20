//! V2 catalogue, requirement 3(e): over every module's golden run, no module emits an ID outside
//! its protocol, an ID twice, or misses a catalogue ID of its protocol, and no ID has two
//! emitters. Each module runs in-process on its contract golden, as the other lens tests do.

use crate::common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Facts, Module, RunContext, RunOptions, SectionOutcome, catalogue};
use netray_model::{CheckId, Protocol};

/// The IDs the module's golden run emitted (in emission order) and its declared `checks()`.
async fn emitted(module: &dyn Module) -> (Vec<CheckId>, Vec<CheckId>) {
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(30),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    let facts = Facts {
        a: vec!["8.8.8.8".parse().unwrap()],
        ..Facts::default()
    };
    let SectionOutcome::Measured { checks, .. } = module.run(&ctx, &facts).await else {
        panic!("{} golden run must be Measured", module.protocol().as_str());
    };
    (
        checks.into_iter().map(|c| c.id).collect(),
        module.checks().to_vec(),
    )
}

#[tokio::test]
async fn every_module_emits_exactly_its_protocols_catalogue_ids_and_no_id_has_two_emitters() {
    let modules: Vec<(Protocol, Box<dyn Module>)> = vec![
        (Protocol::Dns, common::dns_golden_raw("prism.sse")),
        (Protocol::Tls, common::tls_golden("tlsight-inspect.json")),
        (
            Protocol::Http,
            common::http_module(Some("spectra-inspect.json")),
        ),
        (Protocol::Email, common::email_golden("beacon.sse")),
        (Protocol::Ip, common::ip_golden("ifconfig-json.json")),
    ];

    let mut union: Vec<CheckId> = Vec::new();
    for (protocol, module) in &modules {
        let name = protocol.as_str();
        assert_eq!(module.protocol(), *protocol, "{name}: module protocol");
        let (ids, declared) = emitted(module.as_ref()).await;

        let wrong: Vec<String> = ids
            .iter()
            .filter(|id| id.protocol() != *protocol)
            .map(|id| id.to_string())
            .collect();
        assert!(
            wrong.is_empty(),
            "{name} emits IDs outside its protocol: {wrong:?}"
        );

        let set: BTreeSet<&CheckId> = ids.iter().collect();
        assert_eq!(set.len(), ids.len(), "{name} emits an ID twice: {ids:?}");

        let want: BTreeSet<&CheckId> = catalogue()
            .entries()
            .iter()
            .filter(|s| s.protocol == *protocol)
            .map(|s| &s.id)
            .collect();
        assert_eq!(set, want, "{name}: emitted IDs differ from the catalogue's");

        let declared_set: BTreeSet<&CheckId> = declared.iter().collect();
        assert_eq!(
            declared_set, want,
            "{name}: checks() differs from the catalogue's"
        );

        union.extend(ids);
    }

    let all: BTreeSet<&CheckId> = union.iter().collect();
    assert_eq!(all.len(), union.len(), "an ID has two emitters: {union:?}");
    let catalogue_ids: BTreeSet<&CheckId> = catalogue().entries().iter().map(|s| &s.id).collect();
    assert_eq!(catalogue_ids.len(), 57, "the catalogue has 57 checks");
    assert_eq!(
        all, catalogue_ids,
        "the union of emitted IDs is the whole catalogue"
    );
}
