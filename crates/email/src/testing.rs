use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use base64::Engine;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, Row, Shape, Value as EvValue};
use netray_model::{CheckId, Protocol};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::checks::dnsbl::{reverse_ipv4, reverse_ipv6};
use crate::checks::run_all_checks;
use crate::checks::util::parse_tags;
use crate::config::ModuleConfig;
use crate::dns::test_support::TestDnsResolver;
use crate::dns::{AddressLookup, DnsLookup, FetchResolver, LookupError, TlsaRecord};
use crate::job::EmailInspector;
use crate::module::{email_checks, translate};
use crate::quality::types::SseEvent;
use crate::state::OutboundFetch;

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Email
    }

    fn checks(&self) -> &'static [CheckId] {
        email_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// An email module that answers every run with the translation of `contract_sse`, the `data:`
/// lines of a beacon SSE stream.
pub fn golden_module(contract_sse: &str) -> Box<dyn Module> {
    let events: Vec<SseEvent> = contract_sse
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| serde_json::from_str(d.trim()).expect("golden contract decodes as SSE events"))
        .collect();
    Box::new(GoldenModule(translate(&events)))
}

/// A resolver that answers `example.com` with one MX host, its address and PTR, an SPF and a
/// DMARC record, and every other name with a definite empty answer.
pub fn example_dns() -> TestDnsResolver {
    TestDnsResolver::new()
        .with_mx("example.com", vec![(10, "mx1.example.com")])
        .with_ips("mx1.example.com", vec!["192.0.2.10".parse().unwrap()])
        .with_ptr("192.0.2.10".parse().unwrap(), vec!["mx1.example.com"])
        .with_txt("example.com", vec!["v=spf1 mx -all"])
        .with_txt(
            "_dmarc.example.com",
            vec!["v=DMARC1; p=reject; rua=mailto:dmarc@example.com"],
        )
}

/// An email inspector over module defaults and `dns` for every lookup, offline.
pub fn stub_inspector(dns: TestDnsResolver) -> EmailInspector<TestDnsResolver> {
    let config: ModuleConfig = serde_json::from_str("{}").expect("module defaults");
    let dns = Arc::new(dns);
    let fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    EmailInspector::new(Arc::new(config), dns.clone(), dns, fetch, None)
}

fn shape<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Shape> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .map(|b| &b.shape)
}

fn fields<'a>(blocks: &'a [Block], id: &str) -> &'a [(String, EvValue)] {
    match shape(blocks, id) {
        Some(Shape::Fields(f)) => f,
        _ => &[],
    }
}

/// The rows of a table block with a cell reader by column name; no rows when absent.
fn rows<'a>(blocks: &'a [Block], id: &str) -> (&'a [String], &'a [Row]) {
    match shape(blocks, id) {
        Some(Shape::Table { columns, rows, .. }) => (columns, rows),
        _ => (&[], &[]),
    }
}

fn cell<'a>(columns: &[String], row: &'a [EvValue], name: &str) -> Option<&'a EvValue> {
    columns
        .iter()
        .position(|c| c == name)
        .and_then(|i| row.get(i))
}

fn as_text(value: Option<&EvValue>) -> String {
    match value {
        Some(EvValue::Text(t)) => t.clone(),
        Some(EvValue::Int(i)) => i.to_string(),
        Some(EvValue::Addr(a)) => a.to_string(),
        _ => String::new(),
    }
}

fn texts(fields: &[(String, EvValue)], key: &str) -> Vec<String> {
    fields
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| as_text(Some(v)))
        .collect()
}

fn text(fields: &[(String, EvValue)], key: &str) -> Option<String> {
    texts(fields, key).into_iter().next()
}

fn int(fields: &[(String, EvValue)], key: &str) -> Option<i64> {
    fields.iter().find_map(|(k, v)| match v {
        EvValue::Int(i) if k == key => Some(*i),
        _ => None,
    })
}

fn flag(fields: &[(String, EvValue)], key: &str) -> Option<bool> {
    fields.iter().find_map(|(k, v)| match v {
        EvValue::Bool(b) if k == key => Some(*b),
        _ => None,
    })
}

fn names(joined: &str) -> Vec<String> {
    joined
        .split(", ")
        .map(|n| n.trim_end_matches('.').to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// The report domains of a DMARC `rua`/`ruf` URI list, as the DMARC check reads them.
fn report_domains(uri_list: &str) -> Vec<String> {
    uri_list
        .split(',')
        .filter_map(|uri| {
            let uri = uri.trim();
            if let Some(addr) = uri.strip_prefix("mailto:") {
                addr.split('@').nth(1).map(|d| d.to_lowercase())
            } else {
                uri.strip_prefix("https://")
                    .and_then(|s| s.split('/').next())
                    .map(|d| d.to_lowercase())
            }
        })
        .collect()
}

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let len = content.len();
    let mut out = vec![tag];
    match len {
        0..0x80 => out.push(len as u8),
        0x80..0x100 => out.extend([0x81, len as u8]),
        _ => out.extend([0x82, (len >> 8) as u8, len as u8]),
    }
    out.extend_from_slice(content);
    out
}

/// An RSA SubjectPublicKeyInfo whose modulus has `bits` bits; only its size is read.
fn rsa_spki(bits: usize) -> Vec<u8> {
    const RSA_ENCRYPTION: [u8; 9] = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
    let mut modulus = vec![0x00, 0xc5];
    modulus.resize(bits / 8 + 1, 0x5b);
    let key = der(
        0x30,
        &[der(0x02, &modulus), der(0x02, &[0x01, 0x00, 0x01])].concat(),
    );
    let algorithm = der(
        0x30,
        &[der(0x06, &RSA_ENCRYPTION), vec![0x05, 0x00]].concat(),
    );
    der(
        0x30,
        &[algorithm, der(0x03, &[&[0x00][..], &key].concat())].concat(),
    )
}

/// The DKIM TXT record of an `email.dkim` row: a key of the recorded type and size, a revoked
/// key or a record without `p=`.
fn dkim_record(state: &str, key_type: &str, bits: Option<usize>) -> String {
    let k = if key_type.is_empty() {
        String::new()
    } else {
        format!("; k={key_type}")
    };
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    match state {
        "revoked" => format!("v=DKIM1{k}; p="),
        "no_key" => format!("v=DKIM1{k}"),
        "parse_error" => format!("v=DKIM1{k}; p=AAAA"),
        _ if key_type == "ed25519" => {
            format!(
                "v=DKIM1{k}; p={}",
                b64(&vec![0x42; bits.unwrap_or(256) / 8])
            )
        }
        _ => format!(
            "v=DKIM1; k={}; p={}",
            if key_type.is_empty() { "rsa" } else { key_type },
            b64(&rsa_spki(bits.unwrap_or(2048)))
        ),
    }
}

/// The recorded MTA-STS policy answer.
struct Policy {
    status: u16,
    content_type: Option<String>,
    body: String,
}

/// A loopback TLS listener answering every request with `policy`; returns its port.
async fn start_policy_listener(domain: &str, policy: Policy) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let port = listener.local_addr().expect("listener address").port();
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![format!("mta-sts.{domain}")])
            .expect("self-signed certificate");
    let key = rustls::pki_types::PrivateKeyDer::try_from(signing_key.serialize_der())
        .expect("certificate key");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(cert.der().to_vec())],
            key,
        )
        .expect("server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
    let reason = reqwest::StatusCode::from_u16(policy.status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("Unknown");
    let content_type = policy
        .content_type
        .map(|c| format!("content-type: {c}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {} {reason}\r\n{content_type}content-length: {}\r\nconnection: close\r\n\r\n{}",
        policy.status,
        policy.body.len(),
        policy.body
    );
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            let response = response.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    port
}

/// Fails every CNAME lookup and records its name; every other lookup answers empty.
#[derive(Default)]
struct NameProbe(std::sync::Mutex<Vec<String>>);

impl DnsLookup for NameProbe {
    async fn lookup_txt(&self, _: &str) -> Result<Vec<String>, LookupError> {
        Ok(Vec::new())
    }
    async fn lookup_mx(&self, _: &str) -> Result<Vec<(u16, String)>, LookupError> {
        Ok(Vec::new())
    }
    async fn lookup_ips(&self, _: &str) -> Result<AddressLookup, LookupError> {
        Ok(AddressLookup::default())
    }
    async fn lookup_cname(&self, name: &str) -> Result<Vec<String>, LookupError> {
        self.0.lock().expect("probe names").push(name.to_string());
        Err(LookupError {
            record_type: "CNAME",
            name: name.to_string(),
            cause: "probe".to_string(),
        })
    }
    async fn lookup_ptr(&self, _: IpAddr) -> Result<Vec<String>, LookupError> {
        Ok(Vec::new())
    }
    async fn lookup_a(&self, _: &str) -> Result<Vec<Ipv4Addr>, LookupError> {
        Ok(Vec::new())
    }
    async fn lookup_tlsa(&self, _: &str) -> Result<Vec<TlsaRecord>, LookupError> {
        Ok(Vec::new())
    }
    async fn lookup_exists(&self, _: &str) -> Result<bool, LookupError> {
        Ok(false)
    }
    async fn check_dnssec_signed(&self, _: &str) -> Result<bool, LookupError> {
        Ok(false)
    }
}

/// The DKIM names the check queries for `mx_hosts` without user selectors.
async fn dkim_candidates(
    domain: &str,
    mx_hosts: &[String],
    max_user_selectors: usize,
) -> Vec<String> {
    let probe = NameProbe::default();
    crate::checks::dkim::check_dkim_measured(
        domain,
        mx_hosts,
        &[],
        max_user_selectors,
        &probe,
        false,
    )
    .await;
    probe.0.into_inner().expect("probe names")
}

/// The DNSBL query name of `target`, an address or a domain, in `zone`.
fn dnsbl_query(target: &str, zone: &str) -> String {
    match target.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => format!("{}.{zone}", reverse_ipv4(v4)),
        Ok(IpAddr::V6(v6)) => format!("{}.{zone}", reverse_ipv6(v6)),
        Err(_) => format!("{target}.{zone}"),
    }
}

/// Rebuilds the email section from its `email.*` blocks: an offline resolver seeded with what
/// the blocks recorded, the recorded MTA-STS policy served from a loopback listener, the
/// module's inspection without caller selectors, as a recorded run has none, then `translate`. A block the module
/// writes on every answered lookup fails that lookup when absent; a name a present block does
/// not record answers with a definite empty answer.
pub async fn from_evidence(domain: &str, blocks: &[Block]) -> SectionOutcome {
    let config: ModuleConfig = serde_json::from_str("{}").expect("module defaults");
    let absent = |id: &str| shape(blocks, id).is_none();
    let mut dns = TestDnsResolver::new();
    let mut addresses: Vec<(String, Vec<IpAddr>)> = Vec::new();
    let mut add = |name: &str, ip: IpAddr| match addresses.iter_mut().find(|(n, _)| n == name) {
        Some((_, ips)) if ips.contains(&ip) => {}
        Some((_, ips)) => ips.push(ip),
        None => addresses.push((name.to_string(), vec![ip])),
    };

    let (columns, mx_rows) = rows(blocks, "email.mx");
    if absent("email.mx") {
        dns = dns.fail_mx(domain);
    } else {
        let mx: Vec<(u16, String)> = mx_rows
            .iter()
            .map(|r| {
                let preference = match cell(columns, r, "preference") {
                    Some(EvValue::Int(p)) => *p as u16,
                    _ => 0,
                };
                (preference, as_text(cell(columns, r, "host")))
            })
            .collect();
        dns = dns.with_mx(domain, mx.iter().map(|(p, h)| (*p, h.as_str())).collect());
        for (_, host) in &mx {
            let cname = mx_rows
                .iter()
                .find(|r| &as_text(cell(columns, r, "host")) == host)
                .map(|r| names(&as_text(cell(columns, r, "cname"))))
                .unwrap_or_default();
            if !cname.is_empty() {
                dns = dns.with_cname(
                    host.trim_end_matches('.'),
                    cname.iter().map(String::as_str).collect(),
                );
            }
        }
    }
    let mx_hosts: Vec<String> = mx_rows
        .iter()
        .map(|r| {
            as_text(cell(columns, r, "host"))
                .trim_end_matches('.')
                .to_string()
        })
        .filter(|h| !h.is_empty())
        .collect();
    let mut mx_ips: Vec<IpAddr> = Vec::new();
    let (columns, address_rows) = rows(blocks, "email.mx_addresses");
    for row in address_rows {
        if let Some(EvValue::Addr(ip)) = cell(columns, row, "address") {
            mx_ips.push(*ip);
            for host in names(&as_text(cell(columns, row, "host"))) {
                add(&host, *ip);
            }
        }
    }

    let policy_records = [
        ("email.spf", domain.to_string()),
        ("email.dmarc", format!("_dmarc.{domain}")),
        ("email.tls_rpt", format!("_smtp._tls.{domain}")),
        ("email.bimi", format!("default._bimi.{domain}")),
        ("email.mta_sts", format!("_mta-sts.{domain}")),
    ];
    for (id, name) in &policy_records {
        if absent(id) {
            dns = dns.fail_txt(name);
            continue;
        }
        let records = texts(fields(blocks, id), "record");
        if !records.is_empty() {
            dns = dns.with_txt(name, records.iter().map(String::as_str).collect());
        }
    }

    let dmarc = fields(blocks, "email.dmarc");
    for (tag, authorized) in [
        ("rua", flag(dmarc, "rua_authorized")),
        ("ruf", flag(dmarc, "ruf_authorized")),
    ] {
        let uris = text(dmarc, tag).or_else(|| {
            let records = texts(dmarc, "record");
            match records.as_slice() {
                [record] => parse_tags(record).get(tag).cloned(),
                _ => None,
            }
        });
        let external = uris
            .map(|u| report_domains(&u))
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d != domain);
        for report_domain in external {
            let name = format!("{domain}._report._dmarc.{report_domain}");
            dns = match authorized {
                Some(true) => dns.with_txt(&name, vec!["v=DMARC1"]),
                Some(false) => dns,
                None => dns.fail_txt(&name),
            };
        }
    }

    if absent("email.dnssec") {
        dns = dns.fail_dnskey(domain);
    } else if flag(fields(blocks, "email.dnssec"), "dnskey_present") == Some(true) {
        dns = dns.with_dnssec_signed(domain);
    }

    let mut selectors: Vec<(String, Vec<String>)> = Vec::new();
    let (columns, dkim_rows) = rows(blocks, "email.dkim");
    for row in dkim_rows {
        let selector = as_text(cell(columns, row, "selector"));
        let state = as_text(cell(columns, row, "state"));
        let key_type = as_text(cell(columns, row, "key_type"));
        let bits = as_text(cell(columns, row, "bits")).parse().ok();
        let name = format!("{selector}._domainkey.{domain}");
        if state == "cname_loop" {
            for hop in 0..6 {
                let from = if hop == 0 {
                    name.clone()
                } else {
                    format!("hop{hop}.{name}")
                };
                dns = dns.with_cname(&from, vec![&format!("hop{}.{name}", hop + 1)]);
            }
        }
        let record = (state != "cname_loop").then(|| dkim_record(&state, &key_type, bits));
        match selectors.iter_mut().find(|(s, _)| *s == selector) {
            Some((_, records)) => records.extend(record),
            None => selectors.push((selector, record.into_iter().collect())),
        }
    }
    for (selector, records) in &selectors {
        if !records.is_empty() {
            dns = dns.with_txt(
                &format!("{selector}._domainkey.{domain}"),
                records.iter().map(String::as_str).collect(),
            );
        }
    }
    if absent("email.dkim") {
        for name in dkim_candidates(domain, &mx_hosts, config.dkim.max_user_selectors).await {
            dns = dns.fail_txt(&name);
        }
    }

    let mut tlsa: Vec<(String, Vec<TlsaRecord>)> = Vec::new();
    let (columns, dane_rows) = rows(blocks, "email.dane");
    for row in dane_rows {
        let number = |name| match cell(columns, row, name) {
            Some(EvValue::Int(n)) => *n as u8,
            _ => 0,
        };
        let record = TlsaRecord {
            usage: number("usage"),
            selector: number("selector"),
            matching_type: number("matching_type"),
        };
        let name = format!(
            "_25._tcp.{}",
            as_text(cell(columns, row, "host")).trim_end_matches('.')
        );
        match tlsa.iter_mut().find(|(n, _)| *n == name) {
            Some((_, records)) => records.push(record),
            None => tlsa.push((name, vec![record])),
        }
    }
    for (name, records) in tlsa {
        dns = dns.with_tlsa(&name, records);
    }
    if absent("email.dane") {
        for host in &mx_hosts {
            dns = dns.fail_tlsa(&format!("_25._tcp.{host}"));
        }
    }

    let mut ptr_answered: Vec<IpAddr> = Vec::new();
    let (columns, fcrdns_rows) = rows(blocks, "email.fcrdns");
    for row in fcrdns_rows {
        let Some(EvValue::Addr(ip)) = cell(columns, row, "address") else {
            continue;
        };
        ptr_answered.push(*ip);
        let ptr = names(&as_text(cell(columns, row, "ptr")));
        if matches!(cell(columns, row, "confirmed"), Some(EvValue::Bool(true)))
            && let Some(first) = ptr.first()
        {
            add(first, *ip);
        }
        dns = dns.with_ptr(*ip, ptr.iter().map(String::as_str).collect());
    }
    for ip in mx_ips.iter().filter(|ip| !ptr_answered.contains(ip)) {
        dns = dns.fail_ptr(*ip);
    }

    for id in ["email.dnsbl", "email.dnsbl_domain"] {
        let (columns, dnsbl_rows) = rows(blocks, id);
        for row in dnsbl_rows {
            let zone = as_text(cell(columns, row, "zone"));
            let query = dnsbl_query(&as_text(cell(columns, row, "target")), &zone);
            let codes: Vec<Ipv4Addr> = as_text(cell(columns, row, "answer"))
                .split(", ")
                .filter_map(|c| c.parse().ok())
                .collect();
            let answered = matches!(cell(columns, row, "answered"), Some(EvValue::Bool(true)));
            dns = if !codes.is_empty() {
                dns.with_a(&query, codes)
            } else if !answered {
                dns.fail_a(&query)
            } else {
                dns
            };
        }
    }
    if !mx_ips.is_empty() {
        for zone in &config.dnsbl.zones {
            if absent("email.dnsbl_domain") {
                dns = dns.fail_a(&dnsbl_query(domain, zone));
            }
            if absent("email.dnsbl") {
                for ip in &mx_ips {
                    dns = dns.fail_a(&dnsbl_query(&ip.to_string(), zone));
                }
            }
        }
    }

    let sts = fields(blocks, "email.mta_sts");
    let policy = int(sts, "policy_status").map(|status| Policy {
        status: status as u16,
        content_type: text(sts, "content_type"),
        body: match shape(blocks, "email.mta_sts_policy") {
            Some(Shape::Text(body)) => body.clone(),
            _ => String::new(),
        },
    });
    if policy.is_some() {
        add(
            &format!("mta-sts.{domain}"),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        );
    }

    for (name, ips) in addresses {
        dns = dns.with_ips(&name, ips);
    }

    // A recorded run carries no caller selectors (lens records only selector-less runs), so the
    // module probes its own candidates; a row under any other selector is not reproduced.
    let user_selectors: Vec<String> = Vec::new();
    let dns = Arc::new(dns);
    let mut fetch = OutboundFetch::new(500, Arc::new(FetchResolver(dns.clone())));
    // 127.0.0.1 stands in for the policy host's public address; its certificate is self-signed.
    fetch.allow = |_| true;
    fetch.settings.accept_invalid_certs = true;
    if let Some(policy) = policy {
        fetch = fetch.with_mta_sts_port(start_policy_listener(domain, policy).await);
    }

    let (tx, mut rx) = mpsc::channel::<SseEvent>(64);
    tokio::spawn(run_all_checks(
        domain.to_string(),
        user_selectors,
        Arc::new(config),
        dns.clone(),
        dns,
        fetch,
        None,
        tx,
    ));
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    translate(&events)
}
