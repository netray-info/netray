// v2-correctness C2, C7-C9: an SPF record with no Warn or Fail sub-check is a Pass carrying
// `spf_ok`, whatever Info sub-checks it also carries; `email.spf` is then `pass`.
// Each row drives `run_all_checks` offline against an in-memory `TestDnsResolver`.

use std::net::IpAddr;
use std::sync::Arc;

use netray_email::checks::run_all_checks;
use netray_email::config::Config;
use netray_email::dns::FetchResolver;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::quality::types::{Category, SseEvent, Verdict};
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::Status;
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

struct Row {
    name: &'static str,
    dns: fn() -> TestDnsResolver,
    verdict: Verdict,
    present: &'static [&'static str],
    absent: &'static [&'static str],
    v2_status: Status,
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            name: "C7 two a: names on one address, Info overlap only",
            dns: || {
                TestDnsResolver::new()
                    .with_txt(
                        DOMAIN,
                        vec!["v=spf1 a:mail.example.com a:www.example.com -all"],
                    )
                    .with_ips("mail.example.com", vec![ip("192.0.2.10")])
                    .with_ips("www.example.com", vec![ip("192.0.2.10")])
            },
            verdict: Verdict::Pass,
            present: &["spf_ok", "overlapping_cidrs"],
            absent: &[],
            v2_status: Status::Pass,
        },
        Row {
            name: "C8 overlapping prefixes plus ptr:, Warn and no spf_ok",
            dns: || {
                TestDnsResolver::new().with_txt(
                    DOMAIN,
                    vec!["v=spf1 ip4:192.0.2.0/24 ip4:192.0.2.10 ptr -all"],
                )
            },
            verdict: Verdict::Warn,
            present: &["ptr_mechanism", "overlapping_cidrs"],
            absent: &["spf_ok"],
            v2_status: Status::Warn,
        },
        Row {
            name: "C9 clean record, Pass with spf_ok",
            dns: || TestDnsResolver::new().with_txt(DOMAIN, vec!["v=spf1 ip4:192.0.2.10 -all"]),
            verdict: Verdict::Pass,
            present: &["spf_ok"],
            absent: &["overlapping_cidrs", "ptr_mechanism"],
            v2_status: Status::Pass,
        },
    ]
}

fn config() -> Arc<Config> {
    let module: ModuleConfig =
        toml::from_str("[dnsbl]\nzones = [\"bl1.example.com\"]\ntimeout_ms = 1000\n").unwrap();
    Arc::new(Config::from_module(module))
}

async fn run_row(row: &Row) -> Vec<SseEvent> {
    let dns = Arc::new((row.dns)());
    let mut fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    fetch.allow = |_| true;
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
    events
}

#[test]
fn spf_info_only_is_pass_with_spf_ok() {
    let mut problems = Vec::new();
    for row in rows() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let events = runtime.block_on(run_row(&row));

        let spf = events.iter().find_map(|e| match e {
            SseEvent::Category(c) if c.category == Category::Spf => Some(c),
            _ => None,
        });
        let Some(spf) = spf else {
            problems.push(format!("{}: no SPF category event", row.name));
            continue;
        };
        if spf.verdict != row.verdict {
            problems.push(format!(
                "{}: category verdict expected {:?}, got {:?}",
                row.name, row.verdict, spf.verdict
            ));
        }
        let names: Vec<&str> = spf.sub_checks.iter().map(|s| s.name.as_str()).collect();
        for want in row.present {
            if !names.contains(want) {
                problems.push(format!(
                    "{}: sub-check {want} missing in {names:?}",
                    row.name
                ));
            }
        }
        for no in row.absent {
            if names.contains(no) {
                problems.push(format!(
                    "{}: sub-check {no} must be absent in {names:?}",
                    row.name
                ));
            }
        }
        if let Some(ok) = spf.sub_checks.iter().find(|s| s.name == "spf_ok")
            && ok.detail != "valid SPF with 1 authorized prefix(es)"
            && row.name.starts_with("C9")
        {
            problems.push(format!("{}: spf_ok detail {:?}", row.name, ok.detail));
        }

        match translate(&events) {
            SectionOutcome::Measured { checks, .. } => {
                match checks.iter().find(|c| c.id.to_string() == "email.spf") {
                    Some(c) if c.status == row.v2_status => {}
                    Some(c) => problems.push(format!(
                        "{}: email.spf expected {:?}, got {:?}",
                        row.name, row.v2_status, c.status
                    )),
                    None => problems.push(format!("{}: no email.spf", row.name)),
                }
            }
            other => problems.push(format!("{}: expected Measured, got {other:?}", row.name)),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
