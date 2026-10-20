use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Fields, Shape, Value};
use netray_model::{CheckId, Protocol};

use crate::backend::TlsBackend;
use crate::config::{Config, ModuleConfig};
use crate::dns::CaaLookup;
use crate::dns::https_record::ech_in_presentation;
use crate::error::AppError;
use crate::module::{tls_checks, translate, translate_with_facts_at};
use crate::routes::{
    CaaInfo, CaaSource, DnsContext, InspectResponse, PortResult, compute_consistency,
};
use crate::state::AppState;
use crate::tls::ocsp::{OcspInfo, OcspRevocationResult};
use crate::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
use crate::tls::{CertInfo, InspectionError, IpInspectionResult, TlsParams};
use crate::validate::chain_trust::validate_chain;
use crate::validate::ct::{CtInfo, SctEntry, check_ct_status};
use crate::validate::{CheckStatus, ValidationResult, caa_compliance, summarize};

const INSPECT_FIXTURE: &str =
    include_str!("../../../tests/fixtures/contracts/tlsight-inspect.json");

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Tls
    }

    fn checks(&self) -> &'static [CheckId] {
        tls_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// A TLS module that answers every run with the translation of `contract_json`, a tlsight
/// inspect response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    let resp: InspectResponse = serde_json::from_str(contract_json)
        .expect("golden contract decodes as an inspect response");
    Box::new(GoldenModule(translate(&resp)))
}

/// A backend with no network: every name resolves to `93.184.215.14` unless `with_host` says
/// otherwise, and every handshake answers the first address of the tlsight contract fixture.
/// Each handshake is recorded.
pub struct StubTlsBackend {
    hosts: HashMap<String, Vec<IpAddr>>,
    calls: Mutex<Vec<(IpAddr, u16, Option<String>)>>,
    probes: Option<ProtocolProbes>,
    failed: Vec<IpAddr>,
    probe_calls: Mutex<Vec<(IpAddr, u16, String)>>,
}

impl Default for StubTlsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StubTlsBackend {
    pub fn new() -> Self {
        Self {
            hosts: HashMap::new(),
            calls: Mutex::new(Vec::new()),
            probes: None,
            failed: Vec::new(),
            probe_calls: Mutex::new(Vec::new()),
        }
    }

    /// `name` resolves to `ips`; an empty list fails the lookup.
    pub fn with_host(mut self, name: &str, ips: Vec<IpAddr>) -> Self {
        self.hosts.insert(name.to_string(), ips);
        self
    }

    /// Every probe answers `probes`.
    pub fn with_probes(mut self, probes: ProtocolProbes) -> Self {
        self.probes = Some(probes);
        self
    }

    /// The handshake with `ip` fails with `HANDSHAKE_FAILED`.
    pub fn with_failed_handshake(mut self, ip: IpAddr) -> Self {
        self.failed.push(ip);
        self
    }

    /// The probes so far: address, port, negotiated version.
    pub fn probe_calls(&self) -> Vec<(IpAddr, u16, String)> {
        self.probe_calls.lock().expect("probe calls lock").clone()
    }

    /// The handshakes so far: address, port, SNI.
    pub fn calls(&self) -> Vec<(IpAddr, u16, Option<String>)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl TlsBackend for StubTlsBackend {
    fn resolve<'a>(&'a self, hostname: &'a str) -> BoxFuture<'a, Result<Vec<IpAddr>, AppError>> {
        Box::pin(async move {
            match self.hosts.get(hostname) {
                Some(ips) if ips.is_empty() => Err(AppError::DnsResolutionFailed(format!(
                    "{hostname}: no stub answer"
                ))),
                Some(ips) => Ok(ips.clone()),
                None => Ok(vec![IpAddr::from([93, 184, 215, 14])]),
            }
        })
    }

    fn handshake<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        sni: Option<&'a str>,
        _timeout: Duration,
    ) -> BoxFuture<'a, IpInspectionResult> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock")
                .push((ip, port, sni.map(str::to_string)));
            if self.failed.contains(&ip) {
                return IpInspectionResult {
                    ip: ip.to_string(),
                    ip_version: if ip.is_ipv4() { "v4" } else { "v6" }.to_string(),
                    tls: None,
                    chain: None,
                    validation: None,
                    ct: None,
                    enrichment: None,
                    raw_certs: None,
                    probes: None,
                    error: Some(InspectionError {
                        code: "HANDSHAKE_FAILED".into(),
                        message: "connection refused".into(),
                    }),
                };
            }
            let doc: serde_json::Value =
                serde_json::from_str(INSPECT_FIXTURE).expect("fixture is JSON");
            let mut result: IpInspectionResult =
                serde_json::from_value(doc["ports"][0]["ips"][0].clone())
                    .expect("fixture decodes as an address result");
            result.ip = ip.to_string();
            result.ip_version = if ip.is_ipv4() { "v4" } else { "v6" }.to_string();
            if let Some(tls) = result.tls.as_mut() {
                tls.sni = sni.map(str::to_string);
                tls.starttls = None;
            }
            result.validation = None;
            result.ct = None;
            result.enrichment = None;
            result.raw_certs = None;
            result
        })
    }

    /// Canned probes cost what the live backend's do; without them the stub probes nothing.
    fn probe_connections(&self) -> u32 {
        if self.probes.is_some() {
            crate::tls::probe::ProbeKind::ALL.len() as u32
        } else {
            0
        }
    }

    fn probe<'a>(
        &'a self,
        ip: IpAddr,
        port: u16,
        _sni: Option<&'a str>,
        negotiated: &'a str,
        _timeout: Duration,
    ) -> BoxFuture<'a, Option<ProtocolProbes>> {
        Box::pin(async move {
            self.probe_calls.lock().expect("probe calls lock").push((
                ip,
                port,
                negotiated.to_string(),
            ));
            self.probes.clone()
        })
    }
}

/// The default module sections as a service config, without the HTTP probes.
pub fn stub_config() -> Config {
    let mut module: ModuleConfig =
        serde_json::from_str("{}").expect("empty sections take their defaults");
    module.quality.skip_http_checks = true;
    module.into_config().expect("default sections are valid")
}

/// An `AppState` over `backend`, without a DNS resolver.
pub fn stub_state(backend: Arc<StubTlsBackend>) -> AppState {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut state = AppState::new(&stub_config());
    state.backend = backend;
    state
}

const PORT: u16 = 443;

fn field<'a>(fields: &'a Fields, key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::Text(s) | Value::Time(s) => Some(s.clone()),
        _ => None,
    }
}

fn text_of(fields: &Fields, key: &str) -> Option<String> {
    field(fields, key).and_then(as_text)
}

fn bool_of(fields: &Fields, key: &str) -> Option<bool> {
    match field(fields, key)? {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn int_of(fields: &Fields, key: &str) -> Option<i64> {
    match field(fields, key)? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn instant(time: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(time)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn named<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Shape> {
    blocks
        .iter()
        .find(|b| b.id.to_string() == id)
        .map(|b| &b.shape)
}

fn at_address<'a>(blocks: &'a [Block], name: &str, ip: IpAddr) -> Option<&'a Shape> {
    blocks
        .iter()
        .find(|b| {
            b.id.module() == "tls"
                && b.id.name() == name
                && b.id
                    .qualifier()
                    .and_then(|q| q.parse::<SocketAddr>().ok())
                    .is_some_and(|a| a.ip() == ip && a.port() == PORT)
        })
        .map(|b| &b.shape)
}

/// The record data of the facts block `id`; `None` when it is absent or empty, as the module then
/// runs its own lookup.
fn facts_data(blocks: &[Block], id: &str) -> Option<Vec<String>> {
    let data: Vec<String> = record_rows(named(blocks, id)?)
        .into_iter()
        .map(|(_, _, d)| d)
        .collect();
    (!data.is_empty()).then_some(data)
}

/// The data of each record of a records block, with its owner and TTL.
fn record_rows(shape: &Shape) -> Vec<(String, u32, String)> {
    let Shape::Records(records) = shape else {
        return Vec::new();
    };
    records
        .iter()
        .map(|r| {
            let ttl = match r.ttl {
                Value::Int(n) => u32::try_from(n).unwrap_or(0),
                _ => 0,
            };
            (
                as_text(&r.owner).unwrap_or_default(),
                ttl,
                as_text(&r.data).unwrap_or_default(),
            )
        })
        .collect()
}

fn cert_info(fields: &Fields) -> CertInfo {
    let text = |k: &str| text_of(fields, k).unwrap_or_default();
    let (subject, issuer) = (text("subject"), text("issuer"));
    let (not_before, not_after) = (text("not_before"), text("not_after"));
    let lifetime_days = match (instant(&not_before), instant(&not_after)) {
        (Some(from), Some(to)) => (to - from).num_days(),
        _ => 0,
    };
    let days_remaining = int_of(fields, "days_remaining").unwrap_or(0);
    let key = text("key");
    let (key_type, key_size) = key
        .rsplit_once(' ')
        .and_then(|(t, n)| Some((t.to_string(), n.parse::<u32>().ok()?)))
        .unwrap_or_else(|| (key.clone(), 0));
    CertInfo {
        position: text("position"),
        sans: fields
            .iter()
            .filter(|(k, _)| k == "sans")
            .filter_map(|(_, v)| as_text(v))
            .collect(),
        serial: text("serial"),
        not_before,
        not_after,
        days_remaining,
        key_type,
        key_size,
        signature_algorithm: text("signature_algorithm"),
        fingerprint_sha256: text("fingerprint_sha256"),
        fingerprint_sha1: text("fingerprint_sha1"),
        lifetime_days,
        is_expired: days_remaining < 0,
        is_self_signed: bool_of(fields, "self_signed").unwrap_or(subject == issuer),
        cert_policy: text_of(fields, "policy").unwrap_or_else(|| "unknown".to_string()),
        ocsp_url: text_of(fields, "ocsp_url"),
        ca_issuers_url: text_of(fields, "ca_issuers_url"),
        subject,
        issuer,
    }
}

fn tls_params(fields: &Fields, ech_from_dns: Option<bool>) -> TlsParams {
    TlsParams {
        version: text_of(fields, "version").unwrap_or_default(),
        cipher_suite: text_of(fields, "cipher_suite").unwrap_or_default(),
        alpn: text_of(fields, "alpn"),
        sni: text_of(fields, "sni"),
        key_exchange_group: text_of(fields, "key_exchange_group"),
        ocsp: OcspInfo {
            stapled: bool_of(fields, "ocsp_stapled").unwrap_or(false),
            status: text_of(fields, "ocsp_status"),
            this_update: text_of(fields, "ocsp_this_update"),
            next_update: text_of(fields, "ocsp_next_update"),
        },
        ocsp_live: text_of(fields, "ocsp_live_status").map(|status| OcspRevocationResult {
            status,
            reason: text_of(fields, "ocsp_live_reason"),
            revoked_at: text_of(fields, "ocsp_live_revoked_at"),
            checked_at: String::new(),
            answered: true,
        }),
        handshake_ms: match field(fields, "handshake_ms") {
            Some(Value::Duration(ms)) => u32::try_from(*ms).unwrap_or(u32::MAX),
            _ => 0,
        },
        starttls: text_of(fields, "starttls"),
        ech_advertised: bool_of(fields, "ech").or(ech_from_dns),
    }
}

fn probes_of(fields: &Fields) -> ProtocolProbes {
    let outcome = |key: &str| {
        let detail = text_of(fields, &format!("{key}_answer")).unwrap_or_default();
        let hello = detail.strip_prefix("server_hello ").map(|rest| {
            let (version, suite) = rest.split_once(' ').unwrap_or((rest, ""));
            (
                version.to_string(),
                (!suite.is_empty()).then(|| suite.to_string()),
            )
        });
        let (version, cipher_suite) = match hello {
            Some((version, suite)) => (Some(version), suite),
            None => (None, None),
        };
        ProbeOutcome {
            verdict: match text_of(fields, key).as_deref() {
                Some("enabled" | "accepted") => Verdict::Accepted,
                Some("disabled" | "refused") => Verdict::Refused,
                _ => Verdict::Unanswered,
            },
            version,
            cipher_suite,
            detail,
        }
    };
    ProtocolProbes {
        ssl3: outcome("ssl3"),
        tls1_0: outcome("tls1_0"),
        tls1_1: outcome("tls1_1"),
        tls1_2: outcome("tls1_2"),
        tls1_3: outcome("tls1_3"),
        suites_insecure: outcome("suites_insecure"),
        suites_cbc: outcome("suites_cbc"),
        suites_static_rsa: outcome("suites_static_rsa"),
    }
}

fn ct_of(blocks: &[Block], ip: IpAddr) -> Option<CtInfo> {
    let Some(Shape::Table { rows, .. }) = named(blocks, "tls.ct") else {
        return None;
    };
    let row = rows
        .iter()
        .find(|row| matches!(row.first(), Some(Value::Addr(a)) if *a == ip))?;
    let count = match row.get(1) {
        Some(Value::Int(n)) => usize::try_from(*n).unwrap_or(0),
        _ => 0,
    };
    let logs = row.get(2).and_then(as_text).unwrap_or_default();
    (count > 0).then(|| CtInfo {
        sct_count: count,
        scts: logs
            .split(", ")
            .filter(|id| !id.is_empty())
            .map(|id| SctEntry {
                version: 0,
                log_id: id.to_string(),
                timestamp: String::new(),
            })
            .collect(),
    })
}

/// What the stored chain says about itself: trusted when in issuer order, ending self-signed and
/// led by a leaf that is not; at an anchor also when any certificate is self-signed; expiry from `days_remaining` and the run time `now`.
fn validation_of(chain: &[CertInfo], hostname: &str, now: DateTime<Utc>) -> ValidationResult {
    let mut v = validate_chain(chain, Some(hostname), &rustls::RootCertStore::empty(), &[]);
    let issued_leaf = chain.len() >= 2 && chain.first().is_some_and(|l| !l.is_self_signed);
    let anchored = v.chain_order_correct && v.terminates_at_self_signed && issued_leaf;
    v.chain_trusted = anchored;
    // `validate_chain` already holds a chain with a self-signed certificate as at an anchor.
    v.ends_at_anchor = v.ends_at_anchor || anchored;
    v.chain_trust_reason = (!anchored).then(|| {
        if issued_leaf {
            "chain is out of order or does not end self-signed".to_string()
        } else {
            "self-signed certificate".to_string()
        }
    });
    v.any_expired = chain.iter().any(|c| c.is_expired);
    v.any_not_yet_valid = chain
        .iter()
        .any(|c| instant(&c.not_before).is_some_and(|t| t > now));
    v
}

/// The TLS section a stored run's `tls.*` blocks and the facts blocks `dns.caa` and `dns.https`
/// stand for: the tlsight response of port 443 rebuilt from them, judged by the module's own
/// translation. The run time is the leaf's `not_after` less its `days_remaining`.
pub async fn from_evidence(domain: &str, blocks: &[Block]) -> SectionOutcome {
    let https = facts_data(blocks, "dns.https").map(|data| ech_in_presentation(&data));
    let rows = match named(blocks, "tls.addresses") {
        Some(Shape::Table { rows, .. }) => rows.as_slice(),
        _ => &[],
    };

    let mut ips: Vec<IpInspectionResult> = Vec::new();
    for row in rows {
        let (Some(Value::Addr(ip)), Some(outcome)) = (row.first(), row.get(2).and_then(as_text))
        else {
            continue;
        };
        let ip = *ip;
        let mut result = IpInspectionResult {
            ip: ip.to_string(),
            ip_version: if ip.is_ipv4() { "v4" } else { "v6" }.to_string(),
            tls: None,
            chain: None,
            validation: None,
            ct: None,
            enrichment: None,
            error: None,
            raw_certs: None,
            probes: None,
        };
        if outcome == "ok" {
            if let Some(Shape::Chain(items)) = at_address(blocks, "chain", ip) {
                result.chain = Some(items.iter().map(cert_info).collect());
            }
            if let Some(Shape::Fields(f)) = at_address(blocks, "handshake", ip) {
                result.tls = Some(tls_params(f, https));
            }
            if let Some(Shape::Fields(f)) = at_address(blocks, "probes", ip) {
                result.probes = Some(probes_of(f));
            }
            result.ct = ct_of(blocks, ip);
        } else {
            result.error = Some(InspectionError {
                code: outcome,
                message: row.get(3).and_then(as_text).unwrap_or_default(),
            });
        }
        ips.push(result);
    }

    let leaf = ips.iter().find_map(|r| r.chain.as_ref()?.first()).cloned();
    let now = leaf
        .as_ref()
        .and_then(|l| Some(instant(&l.not_after)? - chrono::Duration::days(l.days_remaining)))
        .unwrap_or_else(Utc::now);
    for r in &mut ips {
        r.validation = r.chain.as_ref().map(|c| validation_of(c, domain, now));
    }

    let mut lookup_errors = Vec::new();
    let (caa_lookup, caa_answered) = match (named(blocks, "tls.caa"), facts_data(blocks, "dns.caa"))
    {
        (Some(shape), _) => {
            let answered = record_rows(shape)
                .into_iter()
                .map(|(owner, ttl, data)| crate::dns::AnsweredRecord { owner, data, ttl })
                .collect::<Vec<_>>();
            let data: Vec<String> = answered.iter().map(|r| r.data.clone()).collect();
            (
                CaaLookup::from_presentation(&data),
                Some(CaaSource::Lookup(answered)),
            )
        }
        (None, Some(data)) => (CaaLookup::from_presentation(&data), Some(CaaSource::Facts)),
        (None, None) => {
            lookup_errors.push("caa".to_string());
            (CaaLookup { records: vec![] }, None)
        }
    };
    if https.is_none()
        && !ips
            .iter()
            .any(|r| r.tls.as_ref().is_some_and(|t| t.ech_advertised.is_some()))
    {
        lookup_errors.push("https".to_string());
    }

    let caa_status = match &leaf {
        Some(leaf) => caa_compliance::check_caa_compliance(&caa_lookup, &leaf.issuer, &leaf.sans),
        None if caa_lookup.is_empty() => CheckStatus::Pass,
        None => CheckStatus::Skip,
    };
    let issuer_allowed = leaf.as_ref().map(|_| caa_status == CheckStatus::Pass);
    let caa_info = CaaInfo {
        records: caa_lookup
            .records
            .iter()
            .map(|r| format!("{} \"{}\"", r.tag, r.value))
            .collect(),
        issuer_allowed,
        issuewild_present: caa_lookup.issuewild_present(),
        answered: caa_answered,
        lookup: Some(caa_lookup),
    };

    let consistency = compute_consistency(&ips);
    let first = ips.iter().find(|r| r.error.is_none());
    let ct_status = if named(blocks, "tls.ct").is_some() {
        check_ct_status(first.and_then(|r| r.ct.as_ref()))
    } else {
        CheckStatus::Skip
    };
    let summary = summarize(
        first.and_then(|r| r.validation.as_ref()),
        Some(domain),
        first.is_some_and(|r| r.tls.as_ref().is_some_and(|t| t.ocsp.stapled)),
        first.is_some_and(|r| {
            r.chain
                .as_ref()
                .and_then(|c| c.first())
                .is_some_and(|l| l.ocsp_url.is_some())
        }),
        consistency
            .as_ref()
            .is_some_and(|c| !c.mismatches.is_empty()),
        caa_status,
        CheckStatus::Skip,
        ct_status,
    );

    let resolved_ips = ips.iter().map(|r| r.ip.clone()).collect();
    let resp = InspectResponse {
        request_id: String::new(),
        hostname: domain.to_string(),
        input_mode: "hostname",
        summary,
        ports: vec![PortResult {
            port: PORT,
            ips,
            consistency,
            validation: None,
            tlsa: None,
            quality: None,
            error: None,
        }],
        dns: Some(DnsContext {
            caa: Some(caa_info),
            resolved_ips,
            lookup_errors,
        }),
        quality: None,
        warnings: Vec::new(),
        skipped_ips: Vec::new(),
        duration_ms: 0,
    };
    let facts: Vec<BlockId> = ["dns.caa", "dns.https"]
        .into_iter()
        .filter(|id| named(blocks, id).is_some())
        .filter_map(|id| BlockId::parse(id).ok())
        .collect();
    translate_with_facts_at(&resp, &facts, now)
}
