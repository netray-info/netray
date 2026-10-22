//! V2 Phase 6, requirements 18 and 20: lens's check path runs through `netray_engine::run` with
//! one resolve stage. A request calls the registry's `FactsProvider` once; a failed resolve
//! leaves the section that needs addresses (IP only; HTTP and TLS do their own lookups) errored and the result `incomplete`
//! while DNS and email keep their results; the IP section samples the provider's A/AAAA, not
//! the DNS section's presentation.
//!
//! Copied from crates/lens/tests/engine_run.rs (spec v2-cutover, Phase 6); dropped as V1: the
//! sync `POST /api/check` JSON body (the run is admitted through `netray_web::admission::admit`
//! and its published output read: `summary.complete`/`grade` are the score's, a section's
//! `"error"` status is an `Err` section result) and lens's `BackendExtra` (the IP addresses and
//! the DNS presentation are read from the `ip.addresses`, `dns.a` and `dns.aaaa` evidence blocks
//! lens built those extras from).

use crate::common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{
    FailingFacts, dns_golden_raw, email_golden, facts_golden, facts_with_ips, http_module,
    ip_golden, registry_with, tls_golden,
};
use netray_engine::domain_run::{
    CheckInput, CheckOutput, HARD_DEADLINE, SectionError, run_check_streaming,
};
use netray_engine::{BoxFuture, Domain, Facts, FactsProvider, ResolveError, RunContext};
use netray_model::evidence::{Block, Shape, Value};
use netray_web::admission::{Admit, ApexState, Trigger, admit, until_done};

/// A resolve stage that counts its calls and delegates.
struct Counting {
    inner: Box<dyn FactsProvider>,
    calls: Arc<AtomicUsize>,
}

impl FactsProvider for Counting {
    fn resolve<'a>(
        &'a self,
        ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.resolve(ctx, domain)
    }
}

fn state_with(facts: Box<dyn FactsProvider>) -> ApexState {
    let mut config = common::production_config();
    config.cache_enabled = false;
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts,
        http_module(Some("spectra-inspect.json")),
        email_golden("beacon.sse"),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    );
    let plan = common::production_plan(&registry);
    common::state_with(config, registry, plan)
}

/// One admitted run of example.com; what it published.
async fn admitted_run(state: &ApexState) -> Arc<netray_web::admission::Published> {
    let client = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let Admit::Admitted(mut stream) = admit(state, "example.com", client, Trigger::Api, false)
    else {
        panic!("the run is not admitted");
    };
    until_done(&mut stream).await.expect("the run finishes")
}

/// One run of example.com under the hard deadline, outside admission.
async fn run_check(state: &ApexState) -> CheckOutput {
    let input = CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
    };
    run_check_streaming(
        &state.registry,
        &state.plan,
        state.resolve_budget,
        &state.scoring_profile,
        input,
        HARD_DEADLINE,
        |_| {},
    )
    .await
    .0
}

/// The `address` column of the `ip.addresses` table.
fn ip_addresses(evidence: &[Block]) -> Vec<IpAddr> {
    let Some(Shape::Table { columns, rows, .. }) = evidence
        .iter()
        .find(|b| b.id.to_string() == "ip.addresses")
        .map(|b| &b.shape)
    else {
        return Vec::new();
    };
    let at = columns
        .iter()
        .position(|c| c == "address")
        .expect("ip.addresses has an address column");
    rows.iter()
        .filter_map(|row| match row.get(at) {
            Some(Value::Addr(ip)) => Some(*ip),
            _ => None,
        })
        .collect()
}

/// The A and AAAA addresses of the `dns.a` and `dns.aaaa` records blocks, each once.
fn resolved_ips(evidence: &[Block]) -> Vec<IpAddr> {
    let mut ips: Vec<IpAddr> = Vec::new();
    for (id, rtype) in [("dns.a", "A"), ("dns.aaaa", "AAAA")] {
        let Some(Shape::Records(records)) = evidence
            .iter()
            .find(|b| b.id.to_string() == id)
            .map(|b| &b.shape)
        else {
            continue;
        };
        for r in records {
            if let (Value::Text(t), Value::Text(data)) = (&r.rtype, &r.data)
                && t == rtype
                && let Ok(ip) = data.parse()
                && !ips.contains(&ip)
            {
                ips.push(ip);
            }
        }
    }
    ips
}

#[tokio::test]
async fn one_check_request_calls_the_facts_provider_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let state = state_with(Box::new(Counting {
        inner: facts_golden("prism.sse"),
        calls: calls.clone(),
    }));

    let published = admitted_run(&state).await;

    assert!(
        published.output.score.complete,
        "control: {:?}",
        published.output.score
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the resolve stage runs once per check request"
    );
}

#[tokio::test]
async fn a_failed_resolve_errors_only_the_ip_section_and_keeps_the_others() {
    let state = state_with(Box::new(FailingFacts));

    let published = admitted_run(&state).await;

    let out = &published.output;
    let score = &out.score;
    assert_eq!(score.grade, "incomplete", "score: {score:?}");
    assert!(!score.complete, "score: {score:?}");
    assert!(
        matches!(
            &out.sections["ip"],
            Err(e) if !matches!(e, SectionError::NotApplicable { .. })
        ),
        "ip needs addresses and the resolve failed: {:?}",
        out.sections["ip"].as_ref().err()
    );
    for section in ["http", "tls", "dns", "email"] {
        let result = out.sections[section]
            .as_ref()
            .unwrap_or_else(|e| panic!("{section} needs no addresses: {e:?}"));
        assert!(
            !result.checks.is_empty(),
            "{section} keeps its checks: {result:?}"
        );
    }
}

#[tokio::test]
async fn the_ip_section_samples_the_facts_not_the_dns_presentation() {
    // The DNS golden presents 192.0.2.10; the resolve stage answers other public addresses.
    let state = state_with(facts_with_ips(&["8.8.8.8", "8.8.4.4", "2606:4700::2"]));

    let out = run_check(&state).await;

    let ip = out.sections["ip"]
        .as_ref()
        .unwrap_or_else(|e: &SectionError| panic!("ip section failed: {e:?}"));
    let mut got = ip_addresses(&ip.evidence);
    got.sort();
    let mut want: Vec<IpAddr> = ["8.8.8.8", "8.8.4.4", "2606:4700::2"]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    want.sort();
    assert_eq!(
        got, want,
        "the IP section's addresses are the facts' A/AAAA"
    );

    let Ok(dns) = &out.sections["dns"] else {
        panic!("dns section failed");
    };
    let presented = resolved_ips(&dns.evidence);
    assert!(
        presented.iter().all(|ip| !got.contains(ip)),
        "control: the DNS presentation {presented:?} differs from the facts {got:?}"
    );
}
