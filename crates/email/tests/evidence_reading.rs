// V2 evidence, Phase 5 email row, the reading: the block a judged email check references holds
// the fact that decided its verdict and never contradicts it. Every case runs `run_all_checks`
// against `TestDnsResolver`, so the capture path is exercised, then `translate`.
//
// Spec: specs/features/v2-evidence/spec.md (requirements 4 and 6).
//
// `dbl.spamhaus.org` appears as a DNSBL zone because it is the one domain zone the DNSBL check
// knows (`DOMAIN_ZONES`); no lookup leaves the test resolver.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use netray_email::checks::run_all_checks;
use netray_email::config::Config;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::dns::{FetchResolver, TlsaRecord};
use netray_email::module::REGISTER;
use netray_email::quality::SseEvent;
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::evidence::{
    Block, EvidenceRuleError, Shape, Value as Ev, check_evidence_rule, validate,
};
use netray_model::{CheckResult, Status};
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

fn field(b: &Block, key: &str) -> Vec<Ev> {
    let Shape::Fields(fields) = &b.shape else {
        panic!("{} is not fields", b.id);
    };
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .collect()
}

fn text(s: &str) -> Ev {
    Ev::Text(s.to_string())
}

// 1. One row per DNSBL query, address and domain zones alike, with whether the zone answered.
#[tokio::test(flavor = "multi_thread")]
async fn dnsbl_has_a_row_per_query_with_its_answer() {
    let dns = mail_domain("v=spf1 mx -all", DMARC_REJECT)
        .with_a("10.2.0.192.bl1.example.com", vec![v4("127.0.0.2")])
        .fail_a("10.2.0.192.bl2.example.com")
        .with_a("10.2.0.192.bl3.example.com", vec![v4("127.255.255.254")]);
    let zones = [
        "bl1.example.com",
        "bl2.example.com",
        "bl3.example.com",
        "dbl.spamhaus.org",
    ];
    let (checks, evidence) = run(dns, &zones, &[]).await;
    assert_rule(&checks, &evidence);
    assert_refs(
        &checks,
        "dnsbl",
        Status::Fail,
        &["email.dnsbl", "email.dnsbl_domain", "email.mx"],
    );

    let address_rows = rows(block(&evidence, "email.dnsbl"));
    assert_eq!(
        address_rows.len(),
        6,
        "three address zones for two MX IPs: {address_rows:?}"
    );
    let domain_rows = rows(block(&evidence, "email.dnsbl_domain"));
    assert_eq!(
        domain_rows.len(),
        1,
        "the domain zone row only: {domain_rows:?}"
    );
    let want = [
        (
            "192.0.2.10@bl1.example.com",
            "192.0.2.10",
            true,
            "127.0.0.2",
        ),
        ("198.51.100.10@bl1.example.com", "198.51.100.10", true, ""),
        ("192.0.2.10@bl2.example.com", "192.0.2.10", false, ""),
        ("198.51.100.10@bl2.example.com", "198.51.100.10", true, ""),
        (
            "192.0.2.10@bl3.example.com",
            "192.0.2.10",
            false,
            "127.255.255.254",
        ),
        ("198.51.100.10@bl3.example.com", "198.51.100.10", true, ""),
    ];
    for (query, target, answered, answer) in want {
        let row = address_rows
            .get(query)
            .unwrap_or_else(|| panic!("no row {query}: {address_rows:?}"));
        assert_eq!(row["target"], text(target), "{query}");
        assert_eq!(row["answered"], Ev::Bool(answered), "{query}");
        assert_eq!(row["answer"], text(answer), "{query}");
    }
    let row = domain_rows
        .get("example.com@dbl.spamhaus.org")
        .unwrap_or_else(|| panic!("no domain row: {domain_rows:?}"));
    assert_eq!(row["target"], text("example.com"));
    assert_eq!(row["answered"], Ev::Bool(true));
    assert_eq!(row["answer"], text(""));
}

// 2. Two MX hosts on one address give one row per key; 7. the SPF prefixes repeat under the
// released key `authorized_prefixes`.
#[tokio::test(flavor = "multi_thread")]
async fn a_shared_mx_address_gives_one_row_and_prefixes_repeat() {
    let dns = TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX1), (20, MX2)])
        .with_ips(MX1, vec![ip(MX1_IP)])
        .with_ips(MX2, vec![ip(MX1_IP)])
        .with_ptr(ip(MX1_IP), vec![MX1])
        .with_txt(
            DOMAIN,
            vec!["v=spf1 ip4:192.0.2.0/24 ip4:198.51.100.0/24 -all"],
        )
        .with_txt("_dmarc.example.com", vec![DMARC_REJECT]);
    let (checks, evidence) = run(dns, &["bl1.example.com"], &[]).await;
    assert_rule(&checks, &evidence);

    let addresses = rows(block(&evidence, "email.mx_addresses"));
    assert_eq!(addresses.len(), 1, "{addresses:?}");
    assert_eq!(
        addresses[MX1_IP]["host"],
        text("mx1.example.com, mx2.example.com")
    );
    let fcrdns = rows(block(&evidence, "email.fcrdns"));
    assert_eq!(fcrdns.len(), 1, "{fcrdns:?}");
    assert_eq!(fcrdns[MX1_IP]["confirmed"], Ev::Bool(true));
    let dnsbl = rows(block(&evidence, "email.dnsbl"));
    assert_eq!(dnsbl.len(), 1, "{dnsbl:?}");
    assert!(
        dnsbl.contains_key("192.0.2.10@bl1.example.com"),
        "{dnsbl:?}"
    );

    let spf = block(&evidence, "email.spf");
    assert_eq!(
        field(spf, "authorized_prefixes"),
        vec![text("192.0.2.0/24"), text("198.51.100.0/24")]
    );
    assert!(field(spf, "authorized_prefix").is_empty());
}

// 3. One DKIM row per key record, keyed `<selector>#<n>`, revoked keys, CNAME loops, parse
// errors and records without a key included.
#[tokio::test(flavor = "multi_thread")]
async fn dkim_has_a_row_per_key_record_with_its_state() {
    let mut dns = mail_domain("v=spf1 mx -all", DMARC_REJECT)
        .with_txt(
            "ed._domainkey.example.com",
            vec!["v=DKIM1; k=ed25519; p=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="],
        )
        .with_txt(
            "multi._domainkey.example.com",
            vec!["v=DKIM1; p=", "v=DKIM1; p=AAAA"],
        )
        .with_txt(
            "default._domainkey.example.com",
            vec!["v=DKIM1; k=rsa; t=y"],
        );
    let chain = [
        "loop._domainkey.example.com",
        "c1.example.com",
        "c2.example.com",
        "c3.example.com",
        "c4.example.com",
        "c5.example.com",
        "c6.example.com",
    ];
    for pair in chain.windows(2) {
        dns = dns.with_cname(pair[0], vec![pair[1]]);
    }
    let (checks, evidence) = run(dns, &[], &["ed", "multi", "loop"]).await;
    assert_rule(&checks, &evidence);
    assert_refs(&checks, "dkim", Status::Fail, &["email.dkim"]);

    let rows = rows(block(&evidence, "email.dkim"));
    let want = [
        ("ed#1", "ed", "ok", "ed25519", "256"),
        ("multi#1", "multi", "revoked", "", ""),
        ("multi#2", "multi", "parse_error", "rsa", ""),
        ("loop#1", "loop", "cname_loop", "", ""),
        ("default#1", "default", "no_key", "rsa", ""),
    ];
    assert_eq!(rows.len(), want.len(), "{rows:?}");
    for (key, selector, state, key_type, bits) in want {
        let row = rows
            .get(key)
            .unwrap_or_else(|| panic!("no row {key}: {rows:?}"));
        assert_eq!(row["selector"], text(selector), "{key}");
        assert_eq!(row["state"], text(state), "{key}");
        assert_eq!(row["key_type"], text(key_type), "{key}");
        assert_eq!(row["bits"], text(bits), "{key}");
    }
}

// 4. A check a cross-validation sub-check is routed to references that sub-check's inputs;
// 5. the MTA-STS block names the fetch error.
#[tokio::test(flavor = "multi_thread")]
async fn routed_cross_validation_references_its_inputs() {
    let dns = mail_domain("v=spf1 ip4:203.0.113.0/24 -all", DMARC_REJECT)
        .with_txt("_mta-sts.example.com", vec!["v=STSv1; id=20260101"])
        .with_ips("mta-sts.example.com", vec![ip("10.0.0.1")])
        .with_tlsa(
            "_25._tcp.mx1.example.com",
            vec![TlsaRecord {
                usage: 3,
                selector: 1,
                matching_type: 1,
            }],
        );
    let (checks, evidence) = run(dns, &[], &[]).await;
    assert_rule(&checks, &evidence);

    // reject_no_dkim: DMARC p=reject, SPF -all, no DKIM key.
    assert_refs(
        &checks,
        "dkim",
        Status::Warn,
        &["email.dkim", "email.dmarc", "email.spf"],
    );
    // spf_mx_coverage: the MX addresses are outside the SPF prefix.
    assert_refs(
        &checks,
        "spf",
        Status::Warn,
        &["email.spf", "email.mx_addresses"],
    );
    // mta_sts_without_tls_rpt and dane_without_tls_rpt.
    assert_refs(
        &checks,
        "tls_rpt",
        Status::Warn,
        &["email.tls_rpt", "email.dane", "email.mx"],
    );
    // dane_without_dnssec: the DNSKEY presence, here and in the DNS section.
    assert_refs(
        &checks,
        "dane",
        Status::Fail,
        &["email.dane", "email.dnssec", "dns.dnskey", "email.mx"],
    );
    assert_eq!(
        field(block(&evidence, "email.dnssec"), "dnskey_present"),
        vec![Ev::Bool(false)]
    );

    assert_refs(&checks, "mta_sts", Status::Fail, &["email.mta_sts"]);
    let sts = block(&evidence, "email.mta_sts");
    assert_eq!(field(sts, "fetch_error"), vec![text("blocked")]);
    assert!(field(sts, "policy_status").is_empty(), "{sts:?}");
}

// 4. bimi_dmarc_policy references DMARC; 5. the BIMI block names the logo fetch outcome.
#[tokio::test(flavor = "multi_thread")]
async fn bimi_references_the_dmarc_policy_and_its_logo_fetch() {
    let dns = mail_domain(
        "v=spf1 mx -all",
        "v=DMARC1; p=none; rua=mailto:dmarc@example.com",
    )
    .with_txt(
        "default._bimi.example.com",
        vec!["v=BIMI1; l=https://logo.example.com/logo.svg"],
    )
    .with_ips("logo.example.com", vec![ip("10.0.0.1")]);
    let (checks, evidence) = run(dns, &[], &[]).await;
    assert_rule(&checks, &evidence);

    let bimi = check(&checks, "bimi");
    assert!(
        matches!(bimi.status, Status::Warn | Status::Fail),
        "{bimi:?}"
    );
    for want in ["email.bimi", "email.dmarc", "email.mx"] {
        assert!(
            refs(bimi).iter().any(|r| r == want),
            "email.bimi lacks {want}: {bimi:?}"
        );
    }
    assert_eq!(
        field(block(&evidence, "email.bimi"), "logo_fetch"),
        vec![text("blocked")]
    );
}

// 5. The MX CNAME, the SPF void lookups and the DMARC report authorisation sit in the blocks
// the checks reference.
#[tokio::test(flavor = "multi_thread")]
async fn deciding_facts_sit_in_the_referenced_blocks() {
    let dns = mail_domain(
        "v=spf1 include:v1.example.com include:v2.example.com include:v3.example.com -all",
        "v=DMARC1; p=reject; rua=mailto:dmarc@reports.example.com",
    )
    .with_cname(MX1, vec!["relay.example.com"]);
    let (checks, evidence) = run(dns, &[], &[]).await;
    assert_rule(&checks, &evidence);

    assert_refs(&checks, "mx", Status::Fail, &["email.mx"]);
    let mx = rows(block(&evidence, "email.mx"));
    assert_eq!(mx[MX1]["cname"], text("relay.example.com"));
    assert_eq!(mx[MX2]["cname"], text(""));

    assert_refs(&checks, "spf", Status::Fail, &["email.spf"]);
    let spf = block(&evidence, "email.spf");
    assert_eq!(field(spf, "void_lookups"), vec![Ev::Int(3)]);

    assert_refs(&checks, "dmarc", Status::Warn, &["email.dmarc"]);
    let dmarc = block(&evidence, "email.dmarc");
    assert_eq!(field(dmarc, "rua_authorized"), vec![Ev::Bool(false)]);
    assert!(field(dmarc, "ruf_authorized").is_empty(), "{dmarc:?}");
}
