// V2 evidence, Phase 5 email row (requirements 4-6, criteria C4-C6): beacon sends its blocks as
// one additive `evidence` SSE message right before `summary`; `translate` builds the section's
// evidence from it; the blocks fit `netray_email::module::REGISTER`; every judged email check
// references its block; the `beacon-*.sse` goldens keep every pre-existing message
// byte-identical.
//
// Spec: specs/features/v2-evidence/spec.md. Seams: `run_all_checks` against `TestDnsResolver`
// then `translate` (as in contract_status.rs), and the golden encoding of contract_golden.rs.
//
// Assumed interface (absent today):
// - `netray_email::quality::SseEvent::Evidence { blocks: Vec<netray_model::evidence::Block> }`,
//   on the wire `{"type":"evidence","blocks":[...]}`, sent once by `run_all_checks` right before
//   `Summary`, never on the 30 s pipeline timeout path.
// - `netray_email::module::REGISTER`: `email.mx` table keyed `host` (`host` text, `preference`
//   int); `email.mx_addresses` table keyed `address`; `email.spf` fields (`record` text,
//   `authorized_prefixes` text, `lookup_count` int); `email.dnsbl` table keyed `query`; and the
//   other blocks of the spec's email row.
// - `email.mx` references `email.mx` and `email.mx_addresses`; each judged `email.<check>`
//   references `email.<check>`; judged MX dependents also reference `email.mx`.
//
// Cargo needs on netray-email: `[[test]] name = "evidence"`, `path = "tests/evidence.rs"`,
// `required-features = ["testing"]`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::response::IntoResponse;
use axum::response::sse::{Event, Sse};
use http_body_util::BodyExt;
use netray_email::checks::run_all_checks;
use netray_email::config::Config;
use netray_email::dns::FetchResolver;
use netray_email::dns::test_support::TestDnsResolver;
use netray_email::module::REGISTER;
use netray_email::quality::{Category, CheckResult, SseEvent, SubCheck, Verdict, compute_grade};
use netray_email::state::OutboundFetch;
use netray_email::{ModuleConfig, translate};
use netray_engine::SectionOutcome;
use netray_model::evidence::{Block, Shape, Value as Ev, check_evidence_rule, validate};
use netray_model::{CheckResult as ModelCheck, Status};
use serde_json::{Map, Value};
use tokio::sync::mpsc;

const DOMAIN: &str = "example.com";
const MX1: &str = "mx1.example.com";
const MX2: &str = "mx2.example.com";
const MX1_IP: &str = "192.0.2.10";
const MX2_IP: &str = "198.51.100.10";
const SPF: &str = "v=spf1 mx -all";
const DMARC: &str = "v=DMARC1; p=reject; rua=mailto:dmarc@example.com";
const ZONES: [&str; 3] = ["bl1.example.com", "bl2.example.com", "bl3.example.com"];

const MX_DEPENDENT: [&str; 6] = ["mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns"];

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

/// Two MX hosts with preferences 10 and 20, one address each with a confirming PTR, SPF,
/// DMARC p=reject; every DNSBL query is a definite empty answer (not listed).
fn healthy() -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx(DOMAIN, vec![(10, MX1), (20, MX2)])
        .with_ips(MX1, vec![ip(MX1_IP)])
        .with_ips(MX2, vec![ip(MX2_IP)])
        .with_ptr(ip(MX1_IP), vec![MX1])
        .with_ptr(ip(MX2_IP), vec![MX2])
        .with_txt(DOMAIN, vec![SPF])
        .with_txt("_dmarc.example.com", vec![DMARC])
}

fn config() -> Arc<Config> {
    let zones = ZONES.map(|z| format!("\"{z}\"")).join(", ");
    let module: ModuleConfig =
        toml::from_str(&format!("[dnsbl]\nzones = [{zones}]\ntimeout_ms = 1000\n")).unwrap();
    Arc::new(Config::from_module(module))
}

/// Every event `run_all_checks` sends, up to and including `Summary`.
async fn run_events(dns: TestDnsResolver) -> Vec<SseEvent> {
    let dns = Arc::new(dns);
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
    events
}

fn block<'a>(blocks: &'a [Block], id: &str) -> &'a Block {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .unwrap_or_else(|| panic!("no block {id} among {:?}", ids(blocks)))
}

fn ids(blocks: &[Block]) -> Vec<String> {
    blocks.iter().map(|b| b.id.to_string()).collect()
}

fn refs(check: &ModelCheck) -> Vec<String> {
    check.evidence.iter().map(|b| b.to_string()).collect()
}

fn judged(status: Status) -> bool {
    matches!(status, Status::Pass | Status::Warn | Status::Fail)
}

/// The rows of a table block as column -> value maps.
fn table_rows(b: &Block) -> Vec<HashMap<String, Ev>> {
    let Shape::Table { columns, rows, .. } = &b.shape else {
        panic!("{} is not a table: {:?}", b.id, b.shape);
    };
    rows.iter()
        .map(|row| columns.iter().cloned().zip(row.iter().cloned()).collect())
        .collect()
}

fn field<'a>(b: &'a Block, key: &str) -> Vec<&'a Ev> {
    let Shape::Fields(fields) = &b.shape else {
        panic!("{} is not fields: {:?}", b.id, b.shape);
    };
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn healthy_run_sends_one_evidence_message_whose_blocks_fit_and_are_referenced() {
    let events = run_events(healthy()).await;

    // C5: one evidence message, immediately before the summary that ends the stream.
    let positions: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, SseEvent::Evidence { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(positions.len(), 1, "exactly one evidence event");
    assert!(matches!(events.last(), Some(SseEvent::Summary { .. })));
    assert_eq!(
        positions[0],
        events.len() - 2,
        "evidence sits right before summary"
    );
    let wire = serde_json::to_value(&events[positions[0]]).unwrap();
    assert_eq!(wire["type"], "evidence");
    assert!(
        wire["blocks"].is_array(),
        "evidence carries `blocks`: {wire}"
    );

    // C4: translate builds the blocks; they fit the register and the checks reference them.
    let SectionOutcome::Measured {
        checks, evidence, ..
    } = translate(&events)
    else {
        panic!("expected Measured");
    };
    validate("email", &REGISTER, &evidence).expect("blocks fit the register");
    check_evidence_rule(&checks, &evidence).expect("evidence rule holds");

    let mx_rows = table_rows(block(&evidence, "email.mx"));
    let preference = |host: &str| {
        mx_rows
            .iter()
            .find(|r| matches!(&r["host"], Ev::Text(h) if h.trim_end_matches('.') == host))
            .map(|r| r["preference"].clone())
            .unwrap_or_else(|| panic!("no email.mx row for {host}: {mx_rows:?}"))
    };
    assert_eq!(mx_rows.len(), 2, "one email.mx row per host");
    assert_eq!(preference(MX1), Ev::Int(10));
    assert_eq!(preference(MX2), Ev::Int(20));

    let addresses: Vec<Ev> = table_rows(block(&evidence, "email.mx_addresses"))
        .into_iter()
        .map(|r| r["address"].clone())
        .collect();
    assert!(addresses.contains(&Ev::Addr(ip(MX1_IP))), "{addresses:?}");
    assert!(addresses.contains(&Ev::Addr(ip(MX2_IP))), "{addresses:?}");

    let spf = block(&evidence, "email.spf");
    assert_eq!(field(spf, "record"), vec![&Ev::Text(SPF.to_string())]);
    assert!(
        matches!(field(spf, "lookup_count").as_slice(), [Ev::Int(_)]),
        "email.spf carries one int lookup_count: {:?}",
        spf.shape
    );

    let dmarc = block(&evidence, "email.dmarc");
    assert_eq!(field(dmarc, "record"), vec![&Ev::Text(DMARC.to_string())]);

    let dnsbl_rows = table_rows(block(&evidence, "email.dnsbl"));
    for zone in ZONES {
        for addr in [MX1_IP, MX2_IP] {
            let query = Ev::Text(format!("{addr}@{zone}"));
            assert!(
                dnsbl_rows.iter().any(|r| r["query"] == query),
                "no email.dnsbl row {query:?}: {dnsbl_rows:?}"
            );
        }
    }

    // Each judged email check references its own block; MX dependents also email.mx.
    let check = |name: &str| {
        checks
            .iter()
            .find(|c| c.id.to_string() == format!("email.{name}"))
            .unwrap_or_else(|| panic!("no check email.{name}"))
    };
    for name in ["mx", "spf", "dmarc", "dnsbl"] {
        assert!(
            judged(check(name).status),
            "email.{name} is judged in the healthy scenario: {:?}",
            check(name)
        );
    }
    let mx_refs = refs(check("mx"));
    assert!(mx_refs.contains(&"email.mx".to_string()), "{mx_refs:?}");
    assert!(
        mx_refs.contains(&"email.mx_addresses".to_string()),
        "{mx_refs:?}"
    );
    for c in checks.iter().filter(|c| judged(c.status)) {
        let own = c.id.to_string();
        let r = refs(c);
        assert!(
            r.contains(&own),
            "{own} does not reference its block: {r:?}"
        );
        let name = own.trim_start_matches("email.");
        if MX_DEPENDENT.contains(&name) {
            assert!(
                r.contains(&"email.mx".to_string()),
                "{own} is MX-dependent and does not reference email.mx: {r:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// C6: the goldens. The healthy scenario below is contract_golden.rs's `scenario_healthy`, and
// `encode` its `produce` without the evidence message: the V1 bytes of `beacon.sse`.
// ---------------------------------------------------------------------------

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn sub(name: &str, verdict: Verdict, detail: &str) -> SubCheck {
    SubCheck {
        name: name.to_string(),
        verdict,
        detail: detail.to_string(),
    }
}

fn cat(category: Category, detail: &str, subs: Vec<SubCheck>) -> CheckResult {
    CheckResult::new(category, subs, detail.to_string())
}

fn scenario_healthy() -> Vec<CheckResult> {
    vec![
        cat(
            Category::Mx,
            "2 MX record(s), 4 IP(s)",
            vec![sub(
                "mx_ok",
                Verdict::Pass,
                "2 MX record(s), 4 IP(s) resolved",
            )],
        ),
        cat(
            Category::Spf,
            "SPF record: v=spf1 ip4:192.0.2.0/24 ip6:2001:db8::/32 -all",
            vec![sub(
                "spf_ok",
                Verdict::Pass,
                "valid SPF with 2 authorized prefix(es)",
            )],
        ),
        cat(
            Category::Dkim,
            "DKIM key(s) found",
            vec![
                sub(
                    "weak_rsa_key",
                    Verdict::Fail,
                    "selector 'default': RSA 512 bits < 1024",
                ),
                sub(
                    "rsa_key_ok",
                    Verdict::Pass,
                    "selector 'selector1': RSA 2048 bits",
                ),
            ],
        ),
        cat(
            Category::Dmarc,
            "DMARC: v=DMARC1; p=none; rua=mailto:dmarc@example.com",
            vec![
                sub("policy_none", Verdict::Warn, "no enforcement"),
                sub("no_ruf", Verdict::Info, "no forensic reports configured"),
            ],
        ),
        cat(
            Category::MtaSts,
            "MTA-STS id=20260101",
            vec![
                sub("mode", Verdict::Warn, "mode: testing"),
                sub(
                    "max_age_low",
                    Verdict::Warn,
                    "max_age 3600 < 86400; suggest >= 604800",
                ),
            ],
        ),
        cat(
            Category::TlsRpt,
            "TLS-RPT: v=TLSRPTv1; rua=mailto:tls@example.com",
            vec![sub("valid", Verdict::Pass, "valid TLS-RPT record")],
        ),
        cat(
            Category::Dane,
            "No DANE TLSA records",
            vec![sub(
                "absent",
                Verdict::Info,
                "no TLSA records on any MX host",
            )],
        ),
        cat(
            Category::Dnssec,
            "DNSSEC signed",
            vec![sub(
                "ad_set",
                Verdict::Pass,
                "DNSSEC signed (DNSKEY records present)",
            )],
        ),
        cat(
            Category::Bimi,
            "No BIMI record",
            vec![sub("absent", Verdict::Info, "no BIMI record")],
        ),
        cat(
            Category::Fcrdns,
            "FCrDNS checked for 4 IP(s)",
            vec![
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::10",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 192.0.2.11",
                ),
                sub(
                    "fcrdns_pass",
                    Verdict::Pass,
                    "FCrDNS confirmed for 2001:db8::11",
                ),
            ],
        ),
        cat(
            Category::Dnsbl,
            "checked 3 zone(s) for 4 IP(s)",
            vec![sub("clean", Verdict::Pass, "not listed in any DNSBL")],
        ),
        cat(
            Category::CrossValidation,
            "all cross-validation checks passed",
            vec![],
        ),
    ]
}

fn canonical(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<String> = m.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                let child = canonical(m[&k].clone());
                out.insert(k, child);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonical).collect()),
        other => other,
    }
}

fn canonicalise_wire(wire: &str) -> String {
    wire.split_inclusive('\n')
        .map(|line| match line.strip_prefix("data: ") {
            Some(rest) => {
                let v: Value = serde_json::from_str(rest.trim_end()).expect("data is JSON");
                format!("data: {}\n", serde_json::to_string(&canonical(v)).unwrap())
            }
            None => line.to_string(),
        })
        .collect()
}

/// The V1 wire bytes: category events then summary, through beacon's real encoding path.
async fn encode(results: Vec<CheckResult>) -> String {
    let verdict_list: Vec<Verdict> = results.iter().map(|r| r.verdict).collect();
    let verdicts: HashMap<String, Verdict> = results
        .iter()
        .map(|r| {
            let key = serde_json::to_value(&r.category)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            (key, r.verdict)
        })
        .collect();
    let mut events: Vec<SseEvent> = results.into_iter().map(SseEvent::Category).collect();
    events.push(SseEvent::Summary {
        grade: compute_grade(&verdict_list),
        verdicts,
        duration_ms: 1234,
    });
    let stream = futures::stream::iter(events.into_iter().map(|e| {
        let ev: Event = e.into();
        Ok::<_, Infallible>(ev)
    }));
    let body = Sse::new(stream).into_response().into_body();
    let bytes = body.collect().await.expect("body").to_bytes();
    canonicalise_wire(std::str::from_utf8(&bytes).expect("utf-8"))
}

/// The SSE frames of a wire string, each with its `\n\n` terminator.
fn frames(wire: &str) -> Vec<&str> {
    wire.split_inclusive("\n\n").collect()
}

fn frame_type(frame: &str) -> String {
    let data = frame
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .unwrap_or_else(|| panic!("frame without data: {frame:?}"));
    let v: Value = serde_json::from_str(data).expect("data is JSON");
    v["type"].as_str().unwrap_or_default().to_string()
}

fn read_golden(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {}", path.display()))
}

#[tokio::test]
async fn beacon_goldens_add_one_evidence_message_before_summary_and_keep_the_rest() {
    // Every contract_golden scenario: one evidence frame, right before summary, whose blocks
    // fit the register.
    for name in [
        "beacon.sse",
        "beacon-no-mx.sse",
        "beacon-mx-cname.sse",
        "beacon-null-mx.sse",
        "beacon-sending-no-dkim.sse",
        "beacon-partial.sse",
    ] {
        let golden = read_golden(name);
        let fs = frames(&golden);
        let types: Vec<String> = fs.iter().map(|f| frame_type(f)).collect();
        let evidence: Vec<usize> = (0..types.len())
            .filter(|&i| types[i] == "evidence")
            .collect();
        let summary = types
            .iter()
            .position(|t| t == "summary")
            .unwrap_or_else(|| panic!("{name}: no summary frame"));
        assert_eq!(
            evidence.len(),
            1,
            "{name}: exactly one evidence frame, types {types:?}"
        );
        assert_eq!(
            evidence[0] + 1,
            summary,
            "{name}: evidence right before summary"
        );

        let data = fs[evidence[0]].strip_prefix("data: ").unwrap().trim_end();
        let v: Value = serde_json::from_str(data).unwrap();
        let blocks: Vec<Block> =
            serde_json::from_value(v["blocks"].clone()).expect("blocks deserialise");
        validate("email", &REGISTER, &blocks)
            .unwrap_or_else(|e| panic!("{name}: blocks do not fit the register: {e:?}"));
    }

    // The pipeline-timeout golden carries no evidence.
    let timeout = read_golden("beacon-timeout.sse");
    assert!(
        frames(&timeout).iter().all(|f| frame_type(f) != "evidence"),
        "beacon-timeout.sse must not carry an evidence frame"
    );

    // beacon.sse minus its evidence frame is byte-identical to the V1 encoding.
    let golden = read_golden("beacon.sse");
    let without: String = frames(&golden)
        .into_iter()
        .filter(|f| frame_type(f) != "evidence")
        .collect();
    assert_eq!(
        without,
        encode(scenario_healthy()).await,
        "beacon.sse's pre-existing messages changed"
    );
}
