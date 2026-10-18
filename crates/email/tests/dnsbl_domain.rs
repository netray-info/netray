// V2 history, Phase 6, requirement 19: the domain-zone DNSBL rows move out of `email.dnsbl` into
// the compared block `email.dnsbl_domain`, keyed `<domain>@<zone>`, present only when a domain
// zone was queried; the `dnsbl` check references both blocks. Every case runs `run_all_checks`
// against `TestDnsResolver` (no network), then `translate`.
//
// Spec: specs/features/v2-history/spec.md (requirement 19, Phase 6).
//
// `dbl.spamhaus.org` is the one domain zone the DNSBL check knows (`DOMAIN_ZONES`).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use netray_email::checks::run_all_checks;
use netray_email::config::Config;
use netray_email::dns::FetchResolver;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::module::REGISTER;
use netray_email::quality::SseEvent;
use netray_email::state::OutboundFetch;
use netray_email::{EmailModule, ModuleConfig, translate};
use netray_engine::{Module, SectionOutcome};
use netray_model::diff::diff;
use netray_model::evidence::{
    Block, EvidenceRuleError, Shape, Value as Ev, check_evidence_rule, validate,
};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind, fingerprint};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX1: &str = "mx1.example.com";
const MX2: &str = "mx2.example.com";
const MX1_IP: &str = "192.0.2.10";
const MX2_IP: &str = "198.51.100.10";
const DMARC_REJECT: &str = "v=DMARC1; p=reject; rua=mailto:dmarc@example.com";

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

fn v4(s: &str) -> Ipv4Addr {
    s.parse().unwrap()
}

/// Two MX hosts with one address each and a confirming PTR, the given SPF and DMARC records.
fn mail_domain(spf: &str, dmarc: &str) -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX1), (20, MX2)])
        .with_ips(MX1, vec![ip(MX1_IP)])
        .with_ips(MX2, vec![ip(MX2_IP)])
        .with_ptr(ip(MX1_IP), vec![MX1])
        .with_ptr(ip(MX2_IP), vec![MX2])
        .with_txt(DOMAIN, vec![spf])
        .with_txt("_dmarc.example.com", vec![dmarc])
}

fn config(zones: &[&str]) -> Arc<Config> {
    let zones = zones
        .iter()
        .map(|z| format!("\"{z}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let module: ModuleConfig =
        toml::from_str(&format!("[dnsbl]\nzones = [{zones}]\ntimeout_ms = 1000\n")).unwrap();
    Arc::new(Config::from_module(module))
}

/// The checks and blocks of a run over `dns`.
async fn run(
    dns: TestDnsResolver,
    zones: &[&str],
    selectors: &[&str],
) -> (Vec<CheckResult>, Vec<Block>) {
    let dns = Arc::new(dns);
    let fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    let (tx, mut rx) = mpsc::channel::<SseEvent>(64);
    let task = tokio::spawn(run_all_checks(
        DOMAIN.to_string(),
        selectors.iter().map(|s| s.to_string()).collect(),
        config(zones),
        dns.clone(),
        dns,
        fetch,
        None,
        tx,
    ));
    let mut events = Vec::new();
    let collect = async {
        while let Some(event) = rx.recv().await {
            let done = matches!(event, SseEvent::Summary { .. });
            events.push(event);
            if done {
                break;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(60), collect)
        .await
        .expect("run_all_checks sends a Summary");
    task.abort();
    let SectionOutcome::Measured {
        checks, evidence, ..
    } = translate(&events)
    else {
        panic!("expected Measured");
    };
    validate("email", &REGISTER, &evidence).expect("blocks fit the register");
    (checks, evidence)
}
/// The evidence rule, leaving aside references to other modules' blocks, which this section
/// does not hold.
fn assert_rule(checks: &[CheckResult], evidence: &[Block]) {
    if let Err(errors) = check_evidence_rule(checks, evidence) {
        let own: Vec<_> = errors
            .into_iter()
            .filter(|e| !matches!(e, EvidenceRuleError::Dangling { block, .. } if block.module() != "email"))
            .collect();
        assert!(own.is_empty(), "evidence rule broken: {own:?}");
    }
}

fn check<'a>(checks: &'a [CheckResult], name: &str) -> &'a CheckResult {
    checks
        .iter()
        .find(|c| c.id.to_string() == format!("email.{name}"))
        .unwrap_or_else(|| panic!("no check email.{name}"))
}

fn refs(check: &CheckResult) -> Vec<String> {
    check.evidence.iter().map(|b| b.to_string()).collect()
}

fn assert_refs(checks: &[CheckResult], name: &str, status: Status, want: &[&str]) {
    let c = check(checks, name);
    assert_eq!(c.status, status, "email.{name}: {c:?}");
    let got = refs(c);
    for w in want {
        assert!(
            got.iter().any(|r| r == w),
            "email.{name} lacks {w}: {got:?}"
        );
    }
}

fn block<'a>(blocks: &'a [Block], id: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("no block {id}"))
}

/// The rows of a table block as column -> value maps, by their key column.
fn rows(b: &Block) -> HashMap<String, HashMap<String, Ev>> {
    let Shape::Table { key, columns, rows } = &b.shape else {
        panic!("{} is not a table", b.id);
    };
    rows.iter()
        .map(|row| {
            let map: HashMap<String, Ev> =
                columns.iter().cloned().zip(row.iter().cloned()).collect();
            let k = match &map[key] {
                Ev::Text(t) => t.clone(),
                Ev::Addr(a) => a.to_string(),
                other => panic!("{} key column {key} holds {other:?}", b.id),
            };
            (k, map)
        })
        .collect()
}

fn text(s: &str) -> Ev {
    Ev::Text(s.to_string())
}

const ZONES: [&str; 2] = ["bl1.example.com", "dbl.spamhaus.org"];

fn dnsbl_dns(domain_listed: bool) -> TestDnsResolver {
    let dns = mail_domain("v=spf1 mx -all", DMARC_REJECT)
        .with_a("10.2.0.192.bl1.example.com", vec![v4("127.0.0.2")]);
    if domain_listed {
        dns.with_a("example.com.dbl.spamhaus.org", vec![v4("127.0.1.2")])
    } else {
        dns
    }
}

// C1, C3: address rows stay in `email.dnsbl`, the domain row moves to `email.dnsbl_domain`, and
// the check references both.
#[tokio::test(flavor = "multi_thread")]
async fn domain_zone_row_moves_to_its_own_block() {
    let (checks, evidence) = run(dnsbl_dns(true), &ZONES, &[]).await;
    assert_rule(&checks, &evidence);
    assert_refs(
        &checks,
        "dnsbl",
        Status::Fail,
        &["email.dnsbl", "email.dnsbl_domain"],
    );

    let addr_rows = rows(block(&evidence, "email.dnsbl"));
    let mut keys: Vec<&str> = addr_rows.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "192.0.2.10@bl1.example.com",
            "198.51.100.10@bl1.example.com"
        ]
    );

    let domain_rows = rows(block(&evidence, "email.dnsbl_domain"));
    assert_eq!(domain_rows.len(), 1, "{domain_rows:?}");
    let row = domain_rows
        .get("example.com@dbl.spamhaus.org")
        .unwrap_or_else(|| panic!("no domain row: {domain_rows:?}"));
    assert_eq!(row["query"], text("example.com@dbl.spamhaus.org"));
    assert_eq!(row["zone"], text("dbl.spamhaus.org"));
    assert_eq!(row["target"], text("example.com"));
    assert_eq!(row["answered"], Ev::Bool(true));
    assert_eq!(row["answer"], text("127.0.1.2"));
}

// C2: no domain zone configured, no `email.dnsbl_domain` block.
#[tokio::test(flavor = "multi_thread")]
async fn no_domain_zone_means_no_domain_block() {
    let (_checks, evidence) = run(dnsbl_dns(false), &["bl1.example.com"], &[]).await;
    assert!(
        evidence
            .iter()
            .all(|b| b.id.to_string() != "email.dnsbl_domain"),
        "unexpected email.dnsbl_domain"
    );
}

fn run_of(checks: &[CheckResult], blocks: Vec<Block>) -> Run {
    Run {
        profile_version: 1,
        engine_version: "0.0.0".into(),
        evidence_schema_version: 1,
        grade: Grade::A,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Email,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: checks
            .iter()
            .map(|c| RunCheck {
                id: CheckId::parse(&c.id.to_string()).unwrap(),
                status: c.status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        blocks,
    }
}

// C4: the address is listed in both runs (check stays fail); the domain listing appears in the
// second. Fingerprint with the module's own volatile list differs and the diff names the block.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_domain_listing_changes_fingerprint_and_diff() {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg, None).await.expect("module builds");
    let volatile: Vec<&'static str> = module.volatile().iter().map(|p| p.0).collect();

    let (c1, b1) = run(dnsbl_dns(false), &ZONES, &[]).await;
    let (c2, b2) = run(dnsbl_dns(true), &ZONES, &[]).await;
    assert_eq!(check(&c1, "dnsbl").status, Status::Fail);
    assert_eq!(check(&c2, "dnsbl").status, Status::Fail);
    let (a, b) = (run_of(&c1, b1), run_of(&c2, b2));

    assert_ne!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
    let d = diff(&a, &b, &volatile, None);
    assert!(
        d.evidence
            .iter()
            .any(|c| c.block.to_string() == "email.dnsbl_domain"),
        "no email.dnsbl_domain change: {:?}",
        d.evidence
    );
}

// A domain zone that answers with a policy code (rate limit, refused resolver) is a failed lookup:
// its row keeps the code as returned, and the transition is recorded like any status transition
// into "not measured" (spec R19, branch review second pass).
#[tokio::test(flavor = "multi_thread")]
async fn a_policy_answer_from_the_domain_zone_keeps_its_code_and_is_recorded() {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg, None).await.expect("module builds");
    let volatile: Vec<&'static str> = module.volatile().iter().map(|p| p.0).collect();

    let (c1, b1) = run(dnsbl_dns(false), &ZONES, &[]).await;
    let policy =
        dnsbl_dns(false).with_a("example.com.dbl.spamhaus.org", vec![v4("127.255.255.254")]);
    let (c2, b2) = run(policy, &ZONES, &[]).await;
    let rows = rows(block(&b2, "email.dnsbl_domain"));
    let row = &rows["example.com@dbl.spamhaus.org"];
    assert_eq!(row["answered"], Ev::Bool(false), "{row:?}");
    assert_eq!(row["answer"], text("127.255.255.254"), "{row:?}");
    let (a, b) = (run_of(&c1, b1), run_of(&c2, b2));
    assert_ne!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
}

// A domain-zone lookup that fails (timeout, SERVFAIL) is a recorded transition, like a policy code.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_domain_zone_lookup_is_recorded() {
    let cfg: ModuleConfig = toml::from_str("[dkim]\nmax_user_selectors = 3").unwrap();
    let module = EmailModule::new(cfg, None).await.expect("module builds");
    let volatile: Vec<&'static str> = module.volatile().iter().map(|p| p.0).collect();

    let (c1, b1) = run(dnsbl_dns(false), &ZONES, &[]).await;
    let (c2, b2) = run(
        dnsbl_dns(false).fail_a("example.com.dbl.spamhaus.org"),
        &ZONES,
        &[],
    )
    .await;
    let rows = rows(block(&b2, "email.dnsbl_domain"));
    assert_eq!(
        rows["example.com@dbl.spamhaus.org"]["answered"],
        Ev::Bool(false)
    );
    let (a, b) = (run_of(&c1, b1), run_of(&c2, b2));
    assert_ne!(fingerprint(&a, &volatile), fingerprint(&b, &volatile));
}
