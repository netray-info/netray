// v2-unmeasured, requirements 15 and 16 (contract status table), criteria C2, C18 for the
// email section.
//
// Two rows per `email.*` catalogue check: (a) its worst measured status, (b) Unmeasured with a
// non-empty finding. Each row drives beacon's whole pipeline (`run_all_checks`) offline against
// `TestDnsResolver` and reads the check through `translate`.
//
// Worst measured status: Fail for every check except `email.fcrdns`, whose only non-pass
// verdict is Warn (crates/email/src/checks/fcrdns.rs).
//
// Seam assumed beyond unmeasured_outcomes.rs (implement to match):
// - `TestDnsResolver::fail_ptr(ip)`: the PTR lookup of `ip` errors.
// - `TestDnsResolver::fail_tlsa(name)`: the TLSA lookup of `name` errors.
// plus `fail_mx`, `fail_txt`, `fail_a` as in unmeasured_outcomes.rs.
//
// Cargo needs on netray-email: `[[test]] name = "contract_status"`,
// `path = "tests/contract_status.rs"`, `required-features = ["testing"]`.

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use netray_email::checks::run_all_checks;
use netray_email::config::Config;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::dns::{FetchResolver, TlsaRecord};
use netray_email::quality::types::SseEvent;
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::{CheckResult, Status};
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX_HOST: &str = "mail.example.com";
const MX_IPS: [&str; 2] = ["192.0.2.10", "198.51.100.10"];
const ZONES: [&str; 3] = ["bl1.example.com", "bl2.example.com", "bl3.example.com"];

/// Catalogue ids that lack one of the two rows, with the reason. None today.
const EXCEPTIONS: &[(&str, &str)] = &[];

fn mx_ips() -> Vec<IpAddr> {
    MX_IPS.iter().map(|s| s.parse().unwrap()).collect()
}

fn dnsbl_name(ip: &str, zone: &str) -> String {
    let o: Vec<&str> = ip.split('.').collect();
    format!("{}.{}.{}.{}.{zone}", o[3], o[2], o[1], o[0])
}

/// A mail domain: one MX host with two IPv4 addresses in different /24s, an SPF record.
fn mail_domain() -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX_HOST)])
        .with_ips(MX_HOST, mx_ips())
        .with_txt(DOMAIN, vec!["v=spf1 mx -all"])
}

fn zone_fails(r: TestDnsResolver, zone: &str) -> TestDnsResolver {
    MX_IPS
        .iter()
        .fold(r, |r, ip| r.fail_a(&dnsbl_name(ip, zone)))
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Worst,
    Unmeasured,
}

struct Row {
    id: &'static str,
    kind: Kind,
    dns: fn() -> TestDnsResolver,
}

fn rows() -> Vec<Row> {
    use Kind::{Unmeasured, Worst};
    vec![
        // email.mx: Fail on an MX host that is a CNAME.
        Row {
            id: "email.mx",
            kind: Worst,
            dns: || mail_domain().with_cname(MX_HOST, vec!["alias.example.net"]),
        },
        Row {
            id: "email.mx",
            kind: Unmeasured,
            dns: || mail_domain().fail_mx(DOMAIN),
        },
        // email.spf: Fail without an SPF record.
        Row {
            id: "email.spf",
            kind: Worst,
            dns: || {
                TestDnsResolver::new()
                    .with_mx(DOMAIN, vec![(10, MX_HOST)])
                    .with_ips(MX_HOST, mx_ips())
            },
        },
        Row {
            id: "email.spf",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt(DOMAIN),
        },
        // email.dkim: Fail on a CNAME chain of six hops at the default selector.
        Row {
            id: "email.dkim",
            kind: Worst,
            dns: || {
                mail_domain()
                    .with_cname("default._domainkey.example.com", vec!["hop1.example.net"])
                    .with_cname("hop1.example.net", vec!["hop2.example.net"])
                    .with_cname("hop2.example.net", vec!["hop3.example.net"])
                    .with_cname("hop3.example.net", vec!["hop4.example.net"])
                    .with_cname("hop4.example.net", vec!["hop5.example.net"])
                    .with_cname("hop5.example.net", vec!["hop6.example.net"])
            },
        },
        Row {
            id: "email.dkim",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt("default._domainkey.example.com"),
        },
        // email.dmarc: Fail without a DMARC record.
        Row {
            id: "email.dmarc",
            kind: Worst,
            dns: mail_domain,
        },
        Row {
            id: "email.dmarc",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt("_dmarc.example.com"),
        },
        // email.mta_sts: Fail when the policy host does not resolve.
        Row {
            id: "email.mta_sts",
            kind: Worst,
            dns: || mail_domain().with_txt("_mta-sts.example.com", vec!["v=STSv1; id=1"]),
        },
        Row {
            id: "email.mta_sts",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt("_mta-sts.example.com"),
        },
        // email.tls_rpt: Fail on a record without rua.
        Row {
            id: "email.tls_rpt",
            kind: Worst,
            dns: || mail_domain().with_txt("_smtp._tls.example.com", vec!["v=TLSRPTv1"]),
        },
        Row {
            id: "email.tls_rpt",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt("_smtp._tls.example.com"),
        },
        // email.bimi: Fail on a logo URL that is not HTTPS.
        Row {
            id: "email.bimi",
            kind: Worst,
            dns: || {
                mail_domain().with_txt(
                    "default._bimi.example.com",
                    vec!["v=BIMI1; l=http://example.com/logo.svg"],
                )
            },
        },
        Row {
            id: "email.bimi",
            kind: Unmeasured,
            dns: || mail_domain().fail_txt("default._bimi.example.com"),
        },
        // email.dane: Fail on a TLSA record with an out-of-range usage.
        Row {
            id: "email.dane",
            kind: Worst,
            dns: || {
                mail_domain().with_tlsa(
                    "_25._tcp.mail.example.com",
                    vec![TlsaRecord {
                        usage: 9,
                        selector: 0,
                        matching_type: 1,
                    }],
                )
            },
        },
        Row {
            id: "email.dane",
            kind: Unmeasured,
            dns: || mail_domain().fail_tlsa("_25._tcp.mail.example.com"),
        },
        // email.dnsbl: Fail when a zone lists an MX address; Unmeasured when every zone errors.
        Row {
            id: "email.dnsbl",
            kind: Worst,
            dns: || {
                let r = MX_IPS.iter().fold(mail_domain(), |r, ip| {
                    r.with_a(
                        &dnsbl_name(ip, ZONES[0]),
                        vec!["127.0.0.2".parse().unwrap()],
                    )
                });
                zone_fails(zone_fails(r, ZONES[1]), ZONES[2])
            },
        },
        Row {
            id: "email.dnsbl",
            kind: Unmeasured,
            dns: || ZONES.iter().fold(mail_domain(), |r, z| zone_fails(r, z)),
        },
        // email.fcrdns: Warn without a PTR record (its only non-pass verdict).
        Row {
            id: "email.fcrdns",
            kind: Worst,
            dns: mail_domain,
        },
        Row {
            id: "email.fcrdns",
            kind: Unmeasured,
            dns: || {
                mx_ips()
                    .into_iter()
                    .fold(mail_domain(), |r, ip| r.fail_ptr(ip))
            },
        },
    ]
}

fn expected(id: &str, kind: Kind) -> Status {
    match (kind, id) {
        (Kind::Unmeasured, _) => Status::Unmeasured,
        (Kind::Worst, "email.fcrdns") => Status::Warn,
        (Kind::Worst, _) => Status::Fail,
    }
}

fn config() -> Arc<Config> {
    let zones = ZONES.map(|z| format!("\"{z}\"")).join(", ");
    let module: ModuleConfig =
        toml::from_str(&format!("[dnsbl]\nzones = [{zones}]\ntimeout_ms = 1000\n")).unwrap();
    Arc::new(Config::from_module(module))
}

async fn run_row(row: &Row) -> SectionOutcome {
    let dns = Arc::new((row.dns)());
    let fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    let (tx, mut rx) = mpsc::channel::<SseEvent>(64);
    let task = tokio::spawn(run_all_checks(
        DOMAIN.to_string(),
        Vec::new(),
        config(),
        dns.clone(),
        dns,
        fetch,
        None,
        tx,
    ));
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let done = matches!(event, SseEvent::Summary { .. });
        events.push(event);
        if done {
            break;
        }
    }
    task.abort();
    translate(&events)
}

fn check<'a>(checks: &'a [CheckResult], id: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.id.to_string() == id)
}

fn catalogue_ids(prefix: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/catalogue/checks.toml");
    let doc: toml::Table = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["check"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .filter(|id| id.starts_with(prefix))
        .collect()
}

#[test]
fn contract_status_table() {
    let mut problems = Vec::new();
    for row in rows() {
        let want = expected(row.id, row.kind);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let outcome = runtime.block_on(run_row(&row));
        let SectionOutcome::Measured { checks, .. } = outcome else {
            problems.push(format!(
                "{} {:?}: expected Measured, got {outcome:?}",
                row.id, row.kind
            ));
            continue;
        };
        let Some(c) = check(&checks, row.id) else {
            problems.push(format!(
                "{} {:?}: no such check in the section",
                row.id, row.kind
            ));
            continue;
        };
        if c.status != want {
            problems.push(format!(
                "{} {:?}: expected {want:?}, got {:?} {:?}",
                row.id, row.kind, c.status, c.findings
            ));
        }
        // Rule M: every unmeasured result names its cause.
        if want == Status::Unmeasured && c.findings.is_empty() {
            problems.push(format!(
                "{} {:?}: unmeasured without a finding",
                row.id, row.kind
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Guard: the table plus its exceptions cover exactly the catalogue's `email.*` ids, both rows.
#[test]
fn contract_table_covers_catalogue() {
    let catalogue = catalogue_ids("email.");
    let excepted: BTreeSet<String> = EXCEPTIONS.iter().map(|(id, _)| id.to_string()).collect();
    assert!(EXCEPTIONS.iter().all(|(_, why)| !why.is_empty()));
    for kind in [Kind::Worst, Kind::Unmeasured] {
        let ids: BTreeSet<String> = rows()
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.id.to_string())
            .chain(excepted.iter().cloned())
            .collect();
        assert_eq!(ids, catalogue, "{kind:?} rows vs catalogue email.* ids");
    }
}
