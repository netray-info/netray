//! The engine's protocol modules as lens sections: a `ModuleSection` runs the registry's module
//! in-process and maps its `SectionOutcome` onto lens's backend result.

use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netray_engine::{Domain, Registry, RunContext, RunOptions, RunReport, SectionOutcome};
use netray_model::evidence::{Block, BlockId, Fields, Shape, Value};
use netray_model::{Protocol, Status};
use tokio::sync::mpsc;

use crate::check::{HARD_DEADLINE, SectionError};
use crate::score::{CheckResult, CheckVerdict};

/// Section-specific extra data produced by each backend.
#[derive(Clone, Debug)]
pub enum BackendExtra {
    Dns {
        resolved_ips: Vec<IpAddr>,
        raw_headline: String,
        detail_url: String,
    },
    Tls {
        raw_headline: String,
        detail_url: String,
    },
    Http {
        raw_headline: String,
        detail_url: String,
        status_code: Option<u16>,
        http_version: Option<String>,
        response_duration_ms: Option<u64>,
        server_ip: Option<String>,
        server_org: Option<String>,
        server_network_type: Option<String>,
    },
    Ip {
        addresses: Vec<IpInfo>,
        raw_headline: String,
        detail_url: String,
    },
    Email {
        raw_headline: String,
        detail_url: String,
        grade: Option<String>,
    },
}

/// One enriched public address of the IP section.
#[derive(Clone, Debug)]
pub struct IpInfo {
    pub ip: IpAddr,
    pub org: Option<String>,
    pub geo: Option<String>,
    pub network_type: String,
}

#[derive(Clone, Debug)]
pub struct BackendResult {
    pub checks: Vec<CheckResult>,
    pub extra: BackendExtra,
    /// The section's blocks; the DNS section's also the facts blocks no section carries.
    pub evidence: Vec<Block>,
}

/// Per-request context passed to every section.
#[derive(Clone)]
pub struct BackendContext {
    /// Validated DKIM selectors forwarded to the email backend.
    pub dkim_selectors: Option<Vec<String>>,
    /// Request headers for an HTTP backend; every section runs in-process, so none is read.
    pub forward_headers: reqwest::header::HeaderMap,
}

/// Minimal percent-encoding for query string values (RFC 3986 unreserved set).
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

pub trait Backend: Send + Sync {
    /// The protocol name of the profile section this backend scores.
    fn section(&self) -> &'static str;

    /// Run the section on its own: one engine run with this section only.
    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    >;
}

/// The block `id` of `evidence`.
fn block<'a>(evidence: &'a [Block], id: &str) -> Option<&'a Block> {
    evidence.iter().find(|b| b.id.to_string() == id)
}

/// The fields of the fields block `id`.
fn fields_of<'a>(evidence: &'a [Block], id: &str) -> Option<&'a Fields> {
    match &block(evidence, id)?.shape {
        Shape::Fields(fields) => Some(fields),
        _ => None,
    }
}

fn field<'a>(fields: &'a Fields, key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(fields: &Fields, key: &str) -> Option<String> {
    match field(fields, key)? {
        Value::Text(v) => Some(v.clone()),
        _ => None,
    }
}

/// The rows of the table block `id`, each as (column, value) pairs.
fn table_rows(evidence: &[Block], id: &str) -> Vec<Fields> {
    match block(evidence, id).map(|b| &b.shape) {
        Some(Shape::Table { columns, rows, .. }) => rows
            .iter()
            .map(|row| columns.iter().cloned().zip(row.iter().cloned()).collect())
            .collect(),
        _ => Vec::new(),
    }
}

/// The A and AAAA addresses of the `dns.a` and `dns.aaaa` records blocks, each once.
fn resolved_ips(evidence: &[Block]) -> Vec<IpAddr> {
    let mut ips: Vec<IpAddr> = Vec::new();
    for (id, rtype) in [("dns.a", "A"), ("dns.aaaa", "AAAA")] {
        let Some(Shape::Records(records)) = block(evidence, id).map(|b| &b.shape) else {
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

/// The rows of `ip.addresses`; an empty text is an absent value.
pub(crate) fn ip_addresses(evidence: &[Block]) -> Vec<IpInfo> {
    let non_empty = |row: &Fields, key: &str| text(row, key).filter(|v| !v.is_empty());
    table_rows(evidence, "ip.addresses")
        .iter()
        .filter_map(|row| {
            let Some(Value::Addr(ip)) = field(row, "address") else {
                return None;
            };
            let geo = match (non_empty(row, "city"), non_empty(row, "country")) {
                (Some(city), Some(country)) => Some(format!("{city}, {country}")),
                (city, country) => city.or(country),
            };
            Some(IpInfo {
                ip: *ip,
                org: non_empty(row, "org"),
                geo,
                network_type: text(row, "network_type").unwrap_or_default(),
            })
        })
        .collect()
}

/// The server fields of `http.server`.
#[derive(Default)]
pub(crate) struct HttpServer {
    pub status_code: Option<u16>,
    pub http_version: Option<String>,
    pub inspection_ms: Option<u64>,
    pub address: Option<String>,
    pub org: Option<String>,
    pub network_type: Option<String>,
}

impl HttpServer {
    pub(crate) fn from_evidence(evidence: &[Block]) -> Self {
        let Some(f) = fields_of(evidence, "http.server") else {
            return Self::default();
        };
        Self {
            status_code: match field(f, "status_code") {
                Some(Value::Int(code)) => u16::try_from(*code).ok(),
                _ => None,
            },
            http_version: text(f, "http_version"),
            inspection_ms: match field(f, "inspection_ms") {
                Some(Value::Duration(ms)) => Some(*ms),
                _ => None,
            },
            address: match field(f, "address") {
                Some(Value::Addr(ip)) => Some(ip.to_string()),
                _ => None,
            },
            org: text(f, "org"),
            network_type: text(f, "network_type"),
        }
    }
}

fn verdict_of<'a>(checks: &'a [CheckResult], name: &str) -> Option<&'a CheckResult> {
    checks.iter().find(|c| c.name == name)
}

/// `DNSSEC ✓  CAA –  NS ✓  CNAME-apex ✓`: a failed check whose referenced records blocks are all
/// empty found nothing, and shows `–`; read from the evidence, never from a finding's text.
fn dns_headline(checks: &[CheckResult], evidence: &[Block]) -> String {
    let found_nothing = |c: &CheckResult| {
        !c.evidence.is_empty()
            && c.evidence.iter().all(|id| {
                evidence
                    .iter()
                    .find(|b| b.id == *id)
                    .is_some_and(|b| matches!(&b.shape, Shape::Records(r) if r.is_empty()))
            })
    };
    [
        ("dnssec", "DNSSEC"),
        ("caa", "CAA"),
        ("ns", "NS"),
        ("cname_apex", "CNAME-apex"),
    ]
    .iter()
    .map(|(name, label)| {
        let symbol = match verdict_of(checks, name) {
            Some(c) => match c.verdict {
                CheckVerdict::Pass => "\u{2713}",
                CheckVerdict::Warn => "~",
                CheckVerdict::Fail if found_nothing(c) => "\u{2013}",
                CheckVerdict::Fail => "\u{2717}",
                CheckVerdict::Unmeasured => "?",
                CheckVerdict::NotFound | CheckVerdict::Skip => "\u{2013}",
            },
            None => "\u{2013}",
        };
        format!("{label} {symbol}")
    })
    .collect::<Vec<_>>()
    .join("  ")
}

/// `TLSv1.3, expires in 60d` from the handshake and leaf of the first tested address.
fn tls_headline(evidence: &[Block]) -> String {
    let first = table_rows(evidence, "tls.addresses").into_iter().next();
    let addr = first.and_then(|row| match (field(&row, "address"), field(&row, "port")) {
        (Some(Value::Addr(ip)), Some(Value::Int(port))) => {
            Some(SocketAddr::new(*ip, u16::try_from(*port).ok()?))
        }
        _ => None,
    });
    let version = addr
        .and_then(|a| fields_of(evidence, &format!("tls.handshake@{a}")))
        .and_then(|f| text(f, "version"))
        .unwrap_or_else(|| "TLS".to_string());
    let days = addr.and_then(
        |a| match &block(evidence, &format!("tls.chain@{a}"))?.shape {
            Shape::Chain(certs) => match field(certs.first()?, "days_remaining")? {
                Value::Int(days) => Some(*days),
                _ => None,
            },
            _ => None,
        },
    );
    match days {
        Some(days) => format!("{version}, expires in {days}d"),
        None => "TLS inspection complete".to_string(),
    }
}

/// `HTTPS ✓  HSTS ✓  CSP ✗  CORS ✓`: each symbol follows its check's verdict, so the headline
/// never contradicts the check items (`https_redirect` passes when port 80 redirected to HTTPS,
/// even if a later hop failed).
fn http_headline(checks: &[CheckResult], _evidence: &[Block]) -> String {
    let symbol = |name: &str| match verdict_of(checks, name).map(|c| &c.verdict) {
        Some(CheckVerdict::Pass) => "\u{2713}",
        Some(CheckVerdict::Fail | CheckVerdict::NotFound) => "\u{2717}",
        Some(CheckVerdict::Warn) => "~",
        _ => "-",
    };
    format!(
        "HTTPS {}  HSTS {}  CSP {}  CORS {}",
        symbol("https_redirect"),
        symbol("hsts"),
        symbol("csp"),
        symbol("cors"),
    )
}

/// `Auth: OK  Infra: Warn  Transport: Fail  Brand: N/A`: each group the worst of its checks.
fn email_headline(checks: &[CheckResult]) -> String {
    const GROUPS: &[(&str, &[&str])] = &[
        ("Auth", &["spf", "dkim", "dmarc"]),
        ("Infra", &["mx", "fcrdns", "dnsbl"]),
        ("Transport", &["mta_sts", "tls_rpt", "dane"]),
        ("Brand", &["bimi"]),
    ];
    GROUPS
        .iter()
        .map(|(display, members)| {
            let worst = checks
                .iter()
                .filter(|c| members.contains(&c.name.as_str()))
                .map(|c| match c.verdict {
                    CheckVerdict::Fail | CheckVerdict::NotFound => 3,
                    CheckVerdict::Warn => 2,
                    CheckVerdict::Pass => 1,
                    _ => 0,
                })
                .max();
            let symbol = match worst {
                Some(1) => "OK",
                Some(2) => "Warn",
                Some(3) => "Fail",
                _ => "N/A",
            };
            format!("{display}: {symbol}")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// `<orgs> + <geos>`: up to three organisations and two locations, or the address count.
fn ip_headline(addresses: &[IpInfo]) -> String {
    if addresses.is_empty() {
        return String::new();
    }
    let orgs: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.org.as_deref())
        .collect::<HashSet<_>>()
        .into_iter()
        .take(3)
        .collect();
    let geos: Vec<&str> = addresses
        .iter()
        .filter_map(|a| a.geo.as_deref())
        .collect::<HashSet<_>>()
        .into_iter()
        .take(2)
        .collect();
    match (orgs.is_empty(), geos.is_empty()) {
        (false, false) => format!("{} + {}", orgs.join(", "), geos.join(" + ")),
        (false, true) => orgs.join(", "),
        (true, false) => geos.join(" + "),
        (true, true) => format!("{} address(es)", addresses.len()),
    }
}

fn section_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Dns => "dns",
        Protocol::Tls => "tls",
        Protocol::Http => "http",
        Protocol::Email => "email",
        Protocol::Ip => "ip",
    }
}

pub struct ModuleSection {
    pub registry: Arc<Registry>,
    pub protocol: Protocol,
    pub timeout: Duration,
    pub public_url: String,
}

fn verdict(status: Status) -> CheckVerdict {
    match status {
        Status::Pass => CheckVerdict::Pass,
        Status::Warn => CheckVerdict::Warn,
        Status::Fail => CheckVerdict::Fail,
        Status::NotApplicable | Status::NotTested => CheckVerdict::Skip,
        Status::Unmeasured => CheckVerdict::Unmeasured,
    }
}

impl ModuleSection {
    /// Maps this section's outcome of an engine run onto lens's section result. The IP section
    /// of a run whose resolve stage answered no address is `NoDnsResults`.
    pub(crate) fn adapt(
        &self,
        domain: &str,
        outcome: SectionOutcome,
        report: &RunReport,
    ) -> Result<BackendResult, SectionError> {
        let section = section_name(self.protocol);
        if self.protocol == Protocol::Ip
            && report.resolve_error.is_none()
            && report.facts.a.is_empty()
            && report.facts.aaaa.is_empty()
            && !report.facts.no_address_definite
        {
            return Err(SectionError::NoDnsResults);
        }
        match outcome {
            SectionOutcome::Measured { checks, evidence } => {
                let run: HashSet<&BlockId> = report.blocks().into_iter().map(|b| &b.id).collect();
                let checks: Vec<CheckResult> = checks
                    .into_iter()
                    .map(|c| CheckResult {
                        name: c.id.name().to_string(),
                        verdict: verdict(c.status),
                        messages: c.findings,
                        evidence: c
                            .evidence
                            .into_iter()
                            .filter(|id| run.contains(id))
                            .collect(),
                    })
                    .collect();
                let extra = match self.protocol {
                    Protocol::Email => {
                        let base = if self.public_url.is_empty() {
                            "https://email.netray.info"
                        } else {
                            self.public_url.trim_end_matches('/')
                        };
                        BackendExtra::Email {
                            raw_headline: email_headline(&checks),
                            detail_url: format!("{base}/?domain={}", percent_encode(domain)),
                            grade: fields_of(&evidence, "email.summary")
                                .and_then(|f| text(f, "grade")),
                        }
                    }
                    Protocol::Ip => {
                        let addresses = ip_addresses(&evidence);
                        BackendExtra::Ip {
                            raw_headline: ip_headline(&addresses),
                            addresses,
                            detail_url: self.public_url.clone(),
                        }
                    }
                    Protocol::Dns => BackendExtra::Dns {
                        resolved_ips: resolved_ips(&evidence),
                        raw_headline: dns_headline(&checks, &evidence),
                        detail_url: format!(
                            "{}/?q={}+%2Bcheck",
                            self.public_url.trim_end_matches('/'),
                            percent_encode(domain),
                        ),
                    },
                    Protocol::Tls => BackendExtra::Tls {
                        raw_headline: tls_headline(&evidence),
                        detail_url: format!(
                            "{}/?h={}",
                            self.public_url.trim_end_matches('/'),
                            percent_encode(domain),
                        ),
                    },
                    Protocol::Http => {
                        let server = HttpServer::from_evidence(&evidence);
                        BackendExtra::Http {
                            raw_headline: http_headline(&checks, &evidence),
                            detail_url: format!(
                                "{}/?url=https%3A%2F%2F{}",
                                self.public_url.trim_end_matches('/'),
                                percent_encode(domain),
                            ),
                            status_code: server.status_code,
                            http_version: server.http_version,
                            response_duration_ms: server.inspection_ms,
                            server_ip: server.address,
                            server_org: server.org,
                            server_network_type: server.network_type,
                        }
                    }
                };
                Ok(BackendResult {
                    checks,
                    extra,
                    evidence,
                })
            }
            SectionOutcome::Incomplete { reason } => {
                tracing::warn!(service = section, url = %domain, error = %reason, "backend call failed");
                Err(SectionError::BackendError(reason))
            }
            SectionOutcome::TimedOut => {
                tracing::warn!(service = section, url = %domain, error = "timeout", "backend call failed");
                Err(SectionError::Timeout)
            }
            SectionOutcome::NotApplicable { reason } => Err(SectionError::NotApplicable { reason }),
        }
    }
}

impl Backend for ModuleSection {
    fn section(&self) -> &'static str {
        section_name(self.protocol)
    }

    fn run(
        &self,
        domain: &str,
        context: &BackendContext,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>>
    {
        let domain = domain.to_string();
        let dkim_selectors = context.dkim_selectors.clone();
        Box::pin(async move {
            if self.registry.module(self.protocol).is_none() {
                return Err(SectionError::BackendError(format!(
                    "no {:?} module registered",
                    self.protocol
                )));
            }
            let base = RunContext {
                deadline: Instant::now() + HARD_DEADLINE,
                domain: Domain::new(domain.as_str()),
                options: RunOptions { dkim_selectors },
            };
            let plan = [(self.protocol, self.timeout)];
            let (tx, mut rx) = mpsc::channel(1);
            let (report, event) = tokio::join!(
                netray_engine::run(&self.registry, base, &plan, HARD_DEADLINE, tx),
                rx.recv()
            );
            let outcome = event.map_or(SectionOutcome::TimedOut, |e| e.outcome);
            self.adapt(&domain, outcome, &report)
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn http_headline_https_follows_the_redirect_check() {
        let check = |name: &str, verdict: CheckVerdict| CheckResult {
            name: name.to_string(),
            verdict,
            messages: vec![],
            evidence: vec![],
        };
        let checks = [
            check("https_redirect", CheckVerdict::Pass),
            check("hsts", CheckVerdict::Fail),
            check("csp", CheckVerdict::Warn),
        ];
        // A port-80 probe that redirected to HTTPS and then errored carries no
        // `redirects_to_https`, yet the check passed: the headline follows the check.
        assert_eq!(
            http_headline(&checks, &[]),
            "HTTPS \u{2713}  HSTS \u{2717}  CSP ~  CORS -"
        );
    }

    use super::*;

    #[tokio::test]
    async fn ip_section_returns_no_dns_results_when_empty() {
        let module = netray_ip::testing::golden_module(include_str!(
            "../../../tests/fixtures/contracts/ifconfig-json.json"
        ));
        let section = ModuleSection {
            registry: Arc::new(Registry::new().with(module)),
            protocol: Protocol::Ip,
            timeout: Duration::from_secs(5),
            public_url: "https://ip.example.com".to_string(),
        };
        let context = BackendContext {
            dkim_selectors: None,
            forward_headers: Default::default(),
        };
        let result = section.run("example.com", &context).await;
        assert!(
            matches!(result, Err(SectionError::NoDnsResults)),
            "expected NoDnsResults, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn http_section_reads_response_duration_from_inspection_ms() {
        let server = Block {
            id: BlockId::parse("http.server").unwrap(),
            title: "Server".into(),
            shape: Shape::Fields(vec![
                ("status_code".into(), Value::Int(200)),
                ("inspection_ms".into(), Value::Duration(77)),
            ]),
        };
        let section = ModuleSection {
            registry: Arc::new(Registry::new()),
            protocol: Protocol::Http,
            timeout: Duration::from_secs(5),
            public_url: "https://http.example.com".to_string(),
        };
        let (tx, _rx) = mpsc::channel(1);
        let base = RunContext {
            deadline: Instant::now() + Duration::from_secs(5),
            domain: Domain::new("example.com"),
            options: RunOptions {
                dkim_selectors: None,
            },
        };
        let report =
            netray_engine::run(&section.registry, base, &[], Duration::from_secs(1), tx).await;
        let outcome = SectionOutcome::Measured {
            checks: vec![],
            evidence: vec![server],
        };
        let result = section
            .adapt("example.com", outcome, &report)
            .expect("measured http section adapts");
        match result.extra {
            BackendExtra::Http {
                response_duration_ms,
                ..
            } => assert_eq!(response_duration_ms, Some(77)),
            _ => panic!("expected BackendExtra::Http"),
        }
    }
}
