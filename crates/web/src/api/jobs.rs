//! The inspector jobs of the `/inspect/*` pages over API v1's parts and limiters: one limiter
//! instance with `/api/v1`, one validation, one inspection per tool.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use netray_dns::parser::ParsedQuery;
use netray_dns::{
    DnsAuthCompareResult, DnsCompareResult, DnsDnssecResult, DnsErrorEvent, DnsJob, DnsJobEvent,
    DnsQueryResult, DnsRefusal, DnsTraceResult, DnssecRequest, TraceRequest, blocks,
};
use netray_engine::{Domain, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::Block;
use netray_model::{CheckResult, Protocol};
use netray_tls::input::Target;
use netray_tls::security::rate_limit::{Quotas, Scopes};

use super::dns::DNS_OFF;
use super::error::V1Error;
use super::inspectors::{HTTP_OFF, TLS_OFF};
use super::ip::charge;
use super::state::ApiState;
use crate::pages::{InspectorJobs, Job, JobPart, JobRequest, Refusal, Tool, registrable_domain};

const NO_STARTTLS: &str = "STARTTLS for IMAP and FTP is not supported";
const IP_OFF: &str = "IP lookups are unavailable";
const DNS_BAND_OFF: &str = "The DNS band is unavailable.";
const DNS_BAND_NONE: &str = "No registrable domain, so no DNS band.";

type Measured = Result<(Vec<Block>, Vec<CheckResult>), String>;

/// The inspector jobs over `ApiState`'s parts and `[api.*]` limiters.
pub struct InspectJobs {
    state: ApiState,
}

impl InspectJobs {
    pub fn new(state: ApiState) -> Self {
        Self { state }
    }
}

fn starttls_port(ports: &[u16]) -> bool {
    ports.iter().any(|port| matches!(port, 143 | 21))
}

fn measured(outcome: SectionOutcome) -> Measured {
    match outcome {
        SectionOutcome::Measured { checks, evidence } => Ok((evidence, checks)),
        SectionOutcome::NotApplicable { reason } | SectionOutcome::Incomplete { reason } => {
            Err(reason)
        }
        SectionOutcome::TimedOut => Err("timed out".into()),
    }
}

/// One inspection future, split into the part (evidence) and the band (checks).
fn job(label: &str, protocol: Protocol, run: BoxFuture<'static, Measured>) -> Job {
    let shared = run.shared();
    let evidence = shared.clone();
    let part = JobPart {
        label: label.into(),
        output: Box::pin(async move { evidence.await.map(|(blocks, _)| blocks) }),
    };
    Job::new(vec![part]).with_band(
        protocol,
        Box::pin(async move { shared.await.map(|(_, checks)| checks) }),
    )
}

/// The TLS inspection split per port: one part per port in `ports` (labelled `handshake` for
/// one port, `port <n>` for several) and the band of port 443, else the lowest port.
fn port_jobs(
    ports: &[u16],
    run: BoxFuture<'static, Result<Vec<(u16, SectionOutcome)>, String>>,
) -> Job {
    let shared = run.shared();
    let outcome_of = |shared: futures::future::Shared<_>, port: u16| async move {
        let outcomes: Vec<(u16, SectionOutcome)> = shared.await?;
        let outcome = outcomes
            .into_iter()
            .find(|(p, _)| *p == port)
            .map(|(_, outcome)| outcome)
            .ok_or_else(|| format!("port {port} was not inspected"))?;
        measured(outcome)
    };
    let parts = ports
        .iter()
        .map(|&port| {
            let label = if ports.len() == 1 {
                "handshake".to_string()
            } else {
                format!("port {port}")
            };
            let output = outcome_of(shared.clone(), port);
            JobPart {
                label,
                output: Box::pin(async move { output.await.map(|(blocks, _)| blocks) }),
            }
        })
        .collect();
    let band = outcome_of(shared, netray_tls::module::band_port(ports.iter().copied()));
    Job::new(parts).with_band(
        Protocol::Tls,
        Box::pin(async move { band.await.map(|(_, checks)| checks) }),
    )
}

/// A spawned module run that stops when the band is dropped unread.
struct AbortOnDrop(tokio::task::JoinHandle<SectionOutcome>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type Started<E> = tokio::sync::oneshot::Sender<DnsJob<E>>;

/// How a collected DNS job ended: whether it holds no answer, its errors, whether it finished.
trait Ended {
    fn ended(&self) -> (bool, &[DnsErrorEvent], bool);
    /// The warnings of its summary (an unrecognized server, a truncated server list, a refused
    /// address); trace and DNSSEC summaries carry none.
    fn warnings(&self) -> &[String] {
        &[]
    }
}

macro_rules! ended {
    ($result:ty, $answers:ident) => {
        impl Ended for $result {
            fn ended(&self) -> (bool, &[DnsErrorEvent], bool) {
                (self.$answers.is_empty(), &self.errors, self.done.is_some())
            }
        }
    };
    ($result:ty, $answers:ident, warnings) => {
        impl Ended for $result {
            fn ended(&self) -> (bool, &[DnsErrorEvent], bool) {
                (self.$answers.is_empty(), &self.errors, self.done.is_some())
            }
            fn warnings(&self) -> &[String] {
                self.done.as_ref().map_or(&[], |done| &done.warnings)
            }
        }
    };
}

ended!(DnsQueryResult, batches, warnings);
ended!(DnsCompareResult, batches, warnings);
ended!(DnsAuthCompareResult, batches, warnings);
ended!(DnsTraceResult, hops);
ended!(DnsDnssecResult, levels);

/// The `table` block `dns.errors` "Not measured": each distinct error message of a job that
/// also answered, with its code, so no failure is dropped beside the answers (Rule M).
fn errors_block(errors: &[DnsErrorEvent]) -> Option<Block> {
    use netray_model::evidence::{BlockId, Shape, Value};
    let mut seen = std::collections::BTreeSet::new();
    let rows: Vec<Vec<Value>> = errors
        .iter()
        .filter(|e| seen.insert(e.message.clone()))
        .map(|e| vec![Value::Text(e.message.clone()), Value::Text(e.code.clone())])
        .collect();
    (!rows.is_empty()).then(|| Block {
        id: BlockId::parse("dns.errors").expect("static block id"),
        title: "Not measured".to_owned(),
        shape: Shape::Table {
            key: "message".to_owned(),
            columns: vec!["message".to_owned(), "code".to_owned()],
            rows,
        },
    })
}

/// The `table` block `dns.warnings` "Warnings": each distinct warning of the job's summary.
fn warnings_block(warnings: &[String]) -> Option<Block> {
    use netray_model::evidence::{BlockId, Shape, Value};
    let rows: Vec<Vec<Value>> = warnings
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|w| vec![Value::Text(w.clone())])
        .collect();
    (!rows.is_empty()).then(|| Block {
        id: BlockId::parse("dns.warnings").expect("static block id"),
        title: "Warnings".to_owned(),
        shape: Shape::Table {
            key: "warning".to_owned(),
            columns: vec!["warning".to_owned()],
            rows,
        },
    })
}

const UNFINISHED: &str = "the job ended before it finished";

/// A part that awaits its started job, collects it and maps the result to blocks. A job with
/// no block, or no answer that failed or did not finish, is the part's error, never an empty
/// part; a job that answered and failed shows its blocks and its errors (Rule M).
fn dns_part<E: DnsJobEvent>(
    label: &str,
    started: tokio::sync::oneshot::Receiver<DnsJob<E>>,
    to_blocks: fn(&E::Collected) -> Vec<Block>,
) -> JobPart
where
    E::Collected: Ended,
{
    JobPart {
        label: label.into(),
        output: Box::pin(async move {
            let job = started.await.map_err(|_| "not started".to_string())?;
            let collected = job.collect().await;
            let mut blocks = to_blocks(&collected);
            let (empty, errors, done) = collected.ended();
            if blocks.is_empty() || (empty && (!errors.is_empty() || !done)) {
                return Err(match errors.first() {
                    Some(e) => e.message.clone(),
                    None if !done => UNFINISHED.to_string(),
                    None => "no answer".to_string(),
                });
            }
            let mut errors = errors.to_vec();
            if !done && errors.is_empty() {
                errors.push(DnsErrorEvent {
                    code: "UNFINISHED".to_owned(),
                    message: UNFINISHED.to_owned(),
                });
            }
            blocks.extend(errors_block(&errors));
            blocks.extend(warnings_block(collected.warnings()));
            Ok(blocks)
        }),
    }
}

fn refusal(error: netray_dns::error::ApiError) -> Refusal {
    match DnsRefusal::from(error) {
        DnsRefusal::RateLimited { retry_after_secs } => {
            Refusal::RateLimited(Duration::from_secs(retry_after_secs))
        }
        other => Refusal::Invalid(other.to_string()),
    }
}

fn retry_after(refusal: V1Error) -> Duration {
    match refusal {
        V1Error::RateLimited { retry_after_secs } => Duration::from_secs(retry_after_secs),
        _ => Duration::ZERO,
    }
}

impl InspectJobs {
    fn admit_tls(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        let Some(inspector) = self.state.tls_inspector() else {
            return Ok(());
        };
        let Ok(parsed) = inspector.parse(input) else {
            return Ok(());
        };
        if starttls_port(&parsed.ports) {
            return Ok(());
        }
        let target = match &parsed.target {
            Target::Hostname(host) => host.clone(),
            Target::Ip(ip) => ip.to_string(),
        };
        let limiters = self.state.tls_limiters();
        limiters.prune_if_due();
        match limiters.check_cost(client, &target, parsed.ports.len().max(1) as u32) {
            // Charged nothing and no wait admits it: the job start answers it as the notice.
            Err(netray_tls::error::AppError::CostExceedsLimit { .. }) => Ok(()),
            Err(netray_tls::error::AppError::RateLimited {
                retry_after_secs, ..
            }) => Err(Duration::from_secs(retry_after_secs)),
            Err(_) => Err(Duration::ZERO),
            Ok(()) => Ok(()),
        }
    }

    fn admit_http(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        if self.state.http_inspect().is_none() {
            return Ok(());
        }
        let Ok(url) = netray_http::input::parse_url(input) else {
            return Ok(());
        };
        let limiters = self.state.http_limiters();
        limiters.prune_if_due();
        limiters
            .check(client, url.host_str().unwrap_or_default())
            .map_err(|e| match e {
                netray_http::error::AppError::RateLimited {
                    retry_after_secs, ..
                } => Duration::from_secs(retry_after_secs),
                _ => Duration::ZERO,
            })
    }

    fn admit_ip(&self, client: IpAddr, input: &str) -> Result<(), Duration> {
        if self.state.ip_lookup().is_none() {
            return Ok(());
        }
        let limiters = self.state.ip_limiters();
        limiters.prune_if_due();
        limiters
            .charge_per_ip(client, 1)
            .map_err(Duration::from_secs)?;
        let input = input.trim();
        let target = if input.is_empty() {
            Some(client)
        } else {
            netray_ip::lookup::parse_address(input, self.state.config().ip.internal_mode).ok()
        };
        match target {
            Some(target) => {
                charge(&limiters.per_target, target, 1, "api_v1_ip_target").map_err(retry_after)
            }
            None => Ok(()),
        }
    }

    fn start_tls(&self, req: JobRequest) -> Result<Job, String> {
        let inspector = self
            .state
            .tls_inspector()
            .ok_or_else(|| TLS_OFF.to_string())?
            .clone();
        let parsed = inspector.parse(&req.input).map_err(|e| e.to_string())?;
        if starttls_port(&parsed.ports) {
            return Err(NO_STARTTLS.into());
        }
        self.state
            .tls_limiters()
            .cost_within_bursts(parsed.ports.len().max(1) as u32)
            .map_err(|e| e.to_string())?;
        let burst = self
            .state
            .config()
            .tls
            .per_ip_burst
            .max(parsed.ports.len() as u32)
            .max(1);
        let limiters = netray_tls::security::RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: burst,
                per_ip_burst: burst,
                per_target_per_minute: burst,
                per_target_burst: burst,
            },
            Scopes {
                metrics_prefix: "lens",
                per_ip: "inspect_tls",
                per_target: "inspect_tls_target",
            },
        );
        let client = req.client;
        let ports = parsed.ports.clone();
        let run: BoxFuture<'static, Result<Vec<(u16, SectionOutcome)>, String>> =
            Box::pin(async move {
                let resp = inspector
                    .inspect_parsed(parsed, client, &limiters, String::new())
                    .await
                    .map_err(|e| match e {
                        netray_tls::error::AppError::RateLimited {
                            retry_after_secs, ..
                        } => format!("too many requests; try again in {retry_after_secs} s"),
                        other => other.to_string(),
                    })?;
                Ok(netray_tls::module::translate_ports(&resp))
            });
        Ok(port_jobs(&ports, run))
    }

    fn start_http(&self, req: JobRequest) -> Result<Job, String> {
        let backend = self
            .state
            .http_inspect()
            .ok_or_else(|| HTTP_OFF.to_string())?
            .clone();
        let url = netray_http::input::parse_url(&req.input).map_err(|e| e.to_string())?;
        Ok(job(
            "request",
            Protocol::Http,
            Box::pin(async move {
                let target = backend.target(&url).await.map_err(|e| e.to_string())?;
                let resp = backend
                    .inspect(&url, target, std::time::Instant::now())
                    .await
                    .map_err(|e| e.to_string())?;
                measured(netray_http::translate(&resp))
            }),
        ))
    }

    fn start_ip(&self, req: JobRequest) -> Result<Job, String> {
        let ip = self
            .state
            .ip_lookup()
            .ok_or_else(|| IP_OFF.to_string())?
            .clone();
        let input = req.input.trim();
        let addr = if input.is_empty() {
            req.client
        } else {
            netray_ip::lookup::parse_address(input, self.state.config().ip.internal_mode)
                .map_err(|e| e.to_string())?
        };
        let caller = input.is_empty();
        Ok(job(
            "lookup",
            Protocol::Ip,
            Box::pin(async move {
                let result = if caller {
                    ip.lookup_caller(SocketAddr::new(addr, 0), None).await
                } else {
                    ip.lookup(addr).await
                };
                measured(netray_ip::translate(
                    &[(addr, result.map_err(|e| e.to_string()))],
                    1,
                ))
            }),
        ))
    }
}

/// The one job a `q` asks for, with the sender its part waits on.
enum DnsStart {
    Query(Started<netray_dns::DnsQueryEvent>),
    Trace(Started<netray_dns::DnsTraceEvent>),
    Dnssec(Started<netray_dns::DnsDnssecEvent>),
    Compare(Started<netray_dns::DnsCompareEvent>),
    AuthCompare(Started<netray_dns::DnsAuthCompareEvent>),
}

type DnsBand = (
    tokio::sync::oneshot::Sender<AbortOnDrop>,
    Arc<dyn Module>,
    String,
    std::time::Instant,
);

async fn start_dns_job(
    jobs: netray_dns::DnsJobs,
    limiter: Arc<netray_dns::security::RateLimitState>,
    client: IpAddr,
    parsed: ParsedQuery,
    start: DnsStart,
    band: Option<DnsBand>,
) -> Result<(), Refusal> {
    limiter.prune_if_due();
    let limits = &*limiter;
    match start {
        DnsStart::Query(tx) => {
            let job = jobs
                .query_parsed(parsed, client, limits, String::new())
                .await
                .map_err(refusal)?;
            let _ = tx.send(job);
        }
        DnsStart::Trace(tx) => {
            let record_type = parsed
                .record_types
                .first()
                .map_or_else(|| "A".to_string(), ToString::to_string);
            let request = TraceRequest {
                domain: parsed.domain,
                record_type,
                timeout_secs: None,
            };
            let job = jobs
                .trace(&request, client, limits, String::new())
                .await
                .map_err(refusal)?;
            let _ = tx.send(job);
        }
        DnsStart::Dnssec(tx) => {
            let request = DnssecRequest {
                domain: parsed.domain,
                timeout_secs: None,
            };
            let job = jobs
                .dnssec(&request, client, limits, String::new())
                .await
                .map_err(refusal)?;
            let _ = tx.send(job);
        }
        DnsStart::Compare(tx) => {
            let job = jobs
                .compare_parsed(parsed, client, limits, String::new())
                .await
                .map_err(refusal)?;
            let _ = tx.send(job);
        }
        DnsStart::AuthCompare(tx) => {
            let job = jobs
                .authcompare_parsed(parsed, client, limits, String::new())
                .await
                .map_err(refusal)?;
            let _ = tx.send(job);
        }
    }
    if let Some((tx, module, domain, deadline)) = band {
        let run = tokio::spawn(async move {
            let ctx = RunContext {
                deadline,
                domain: Domain::new(domain),
                options: Default::default(),
            };
            module.run(&ctx, &Facts::default()).await
        });
        let _ = tx.send(AbortOnDrop(run));
    }
    Ok(())
}

impl InspectJobs {
    fn start_dns(&self, req: JobRequest) -> Result<Job, String> {
        let jobs = self
            .state
            .dns_jobs()
            .ok_or_else(|| DNS_OFF.to_string())?
            .clone();
        let parsed = netray_dns::parser::parse(&req.input).map_err(|e| e.to_string())?;
        let flags: Vec<String> = req
            .input
            .split_whitespace()
            .filter_map(|t| t.strip_prefix('+'))
            .map(str::to_ascii_lowercase)
            .collect();
        let flagged = |name: &str| flags.iter().any(|f| f == name);

        let (part, start) = if flagged("trace") {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (
                dns_part("trace", rx, blocks::trace_blocks),
                DnsStart::Trace(tx),
            )
        } else if parsed.dnssec || flagged("dnssec") {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (
                dns_part("dnssec", rx, blocks::dnssec_blocks),
                DnsStart::Dnssec(tx),
            )
        } else if flagged("compare") {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (
                dns_part("compare", rx, blocks::compare_blocks),
                DnsStart::Compare(tx),
            )
        } else if flagged("auth") {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (
                dns_part("authcompare", rx, blocks::authcompare_blocks),
                DnsStart::AuthCompare(tx),
            )
        } else {
            let (tx, rx) = tokio::sync::oneshot::channel();
            (
                dns_part("query", rx, blocks::query_blocks),
                DnsStart::Query(tx),
            )
        };

        let domain = parsed.domain.trim_end_matches('.');
        let registrable = if domain.parse::<IpAddr>().is_ok() {
            None
        } else {
            registrable_domain(domain)
        };
        let module = self.state.dns_module().cloned();
        let (band_tx, band_rx) = tokio::sync::oneshot::channel::<AbortOnDrop>();
        let (band, band_send): (BoxFuture<'static, Result<Vec<CheckResult>, String>>, _) =
            match (registrable, module) {
                (None, _) => (Box::pin(async { Err(DNS_BAND_NONE.to_string()) }), None),
                (Some(_), None) => (Box::pin(async { Err(DNS_BAND_OFF.to_string()) }), None),
                (Some(domain), Some(module)) => (
                    Box::pin(async move {
                        let mut run = band_rx.await.map_err(|_| "not started".to_string())?;
                        let outcome = (&mut run.0)
                            .await
                            .map_err(|e| format!("the DNS band failed: {e}"))?;
                        // The band judges the registrable domain; the page shows the queried
                        // name's records, so the checks link none of them.
                        measured(outcome).map(|(_, checks)| {
                            checks
                                .into_iter()
                                .map(|c| CheckResult {
                                    evidence: Vec::new(),
                                    ..c
                                })
                                .collect()
                        })
                    }),
                    Some((band_tx, module, domain, req.deadline)),
                ),
            };

        let gate = start_dns_job(
            jobs,
            self.state.dns_limiter().clone(),
            req.client,
            parsed,
            start,
            band_send,
        );
        Ok(Job::new(vec![part])
            .with_band(Protocol::Dns, band)
            .with_gate(Box::pin(gate)))
    }
}

impl InspectorJobs for InspectJobs {
    fn admit(&self, tool: Tool, client: IpAddr, input: &str) -> Result<(), Duration> {
        match tool {
            Tool::Tls => self.admit_tls(client, input),
            Tool::Http => self.admit_http(client, input),
            Tool::Ip => self.admit_ip(client, input),
            Tool::Dns => Ok(()),
        }
    }

    fn start(&self, req: JobRequest) -> Result<Job, String> {
        match req.tool {
            Tool::Tls => self.start_tls(req),
            Tool::Http => self.start_http(req),
            Tool::Ip => self.start_ip(req),
            Tool::Dns => self.start_dns(req),
        }
    }
}
