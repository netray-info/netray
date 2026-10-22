use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::backend::TlsBackend;
use crate::config::Config;
use crate::dns::{AnsweredRecord, CaaLookup, TlsaLookup};
use crate::error::AppError;
use crate::input::{self, Target};
use crate::security::rate_limit::select_representative_ips;
use crate::security::target_policy;
use crate::state::AppState;
use crate::tls;
use crate::validate;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InspectResponse {
    pub request_id: String,
    pub hostname: String,
    #[serde(deserialize_with = "deserialize_input_mode")]
    #[schema(value_type = String)]
    pub input_mode: InputMode,
    pub summary: validate::Summary,
    pub ports: Vec<PortResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<crate::quality::QualityResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_ips: Vec<String>,
    pub duration_ms: u64,
}

/// `"hostname"` or `"ip"`. An alias, so serde does not borrow the field from its input.
pub type InputMode = &'static str;

fn deserialize_input_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<InputMode, D::Error> {
    match String::deserialize(d)?.as_str() {
        "hostname" => Ok("hostname"),
        "ip" => Ok("ip"),
        other => Err(serde::de::Error::unknown_variant(
            other,
            &["hostname", "ip"],
        )),
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PortResult {
    pub port: u16,
    pub ips: Vec<tls::IpInspectionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistency: Option<ConsistencyResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<validate::ValidationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tlsa: Option<TlsaInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<crate::quality::PortQualityResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<tls::InspectionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DnsContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caa: Option<CaaInfo>,
    pub resolved_ips: Vec<String>,
    /// The lookups that failed or got no definitive answer: "caa", "tlsa", "https".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookup_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CaaInfo {
    pub records: Vec<String>,
    pub issuer_allowed: Option<bool>,
    pub issuewild_present: bool,
    /// What the CAA verdict was judged from; `None` when the lookup failed or the response was
    /// read from JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub answered: Option<CaaSource>,
    /// The records the verdict was judged from, so each port's leaf can be judged against them;
    /// `None` when the response was read from JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub lookup: Option<CaaLookup>,
}

/// The CAA records TLS judged by: the facts' records, or its own climb's answered records with
/// owner and TTL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaaSource {
    Facts,
    Lookup(Vec<AnsweredRecord>),
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TlsaInfo {
    pub records: Vec<String>,
    pub dnssec_signed: bool,
    pub dane_valid: Option<bool>,
    /// The answered records with owner and TTL; `None` when the response was read from JSON.
    #[serde(skip)]
    #[schema(ignore)]
    pub answered: Option<Vec<AnsweredRecord>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConsistencyResult {
    pub certificates_match: bool,
    pub tls_versions_match: bool,
    pub cipher_suites_match: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mismatches: Vec<ConsistencyMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConsistencyMismatch {
    pub field: String,
    /// Per-IP values, keyed by IP address string.
    pub values: HashMap<String, String>,
}

/// The parsed input, the addresses to inspect (after the target policy and any rate-limit
/// cap) and the warnings so far: what an inspection starts from. `caa` and `ech`, when set,
/// replace the inspection's own CAA and HTTPS-record lookups.
pub struct InspectRequest {
    pub parsed: input::ParsedInput,
    pub ips: Vec<IpAddr>,
    /// How many of `ips` are probed (the plan's charge): the first, in order, whose handshake
    /// succeeded.
    pub probed: usize,
    pub warnings: Vec<String>,
    pub skipped_ips: Vec<String>,
    pub request_id: String,
    pub start: Instant,
    pub caa: Option<CaaLookup>,
    pub ech: Option<bool>,
}

/// Resolves the target to its addresses; an IP target is its own address.
pub async fn resolve_target(state: &AppState, target: &Target) -> Result<Vec<IpAddr>, AppError> {
    // Resolved IPs are re-checked against target_policy (DNS rebinding protection).
    match target {
        Target::Hostname(h) => state.backend.resolve(h).await,
        Target::Ip(ip) => Ok(vec![*ip]),
    }
}

/// Drops the addresses the target policy blocks, with a warning each; none left is
/// `BlockedTarget`.
pub fn filter_allowed(
    ips: Vec<IpAddr>,
    allow_blocked: bool,
    hostname: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<IpAddr>, AppError> {
    let allowed_ips: Vec<IpAddr> = ips
        .into_iter()
        .filter(
            |ip| match target_policy::check_allowed_with_policy(ip, allow_blocked) {
                Ok(()) => true,
                Err(reason) => {
                    warnings.push(format!("{ip}: blocked ({reason})"));
                    false
                }
            },
        )
        .collect();

    if allowed_ips.is_empty() {
        return Err(AppError::BlockedTarget(format!(
            "all resolved IPs for {hostname} are in blocked ranges"
        )));
    }
    Ok(allowed_ips)
}

/// The addresses an inspection covers within a rate-limit budget, how many of them (the first,
/// in selection order) are probed, and what that costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub ips: Vec<IpAddr>,
    pub skipped: Vec<String>,
    pub probed: usize,
    pub cost: u32,
}

/// The counts of inspected and probed addresses out of `addresses` within `budget`: on each of
/// `ports` ports an inspected address costs 1 and a probed one `probes` more. All of them when
/// the whole cost fits; else, when one probed address fits on every port, as many inspected
/// addresses as leave room for it and as many probed as the rest pays for; else at least one
/// inspected address and none probed. With `probes` 0 every inspected address is probed.
pub fn plan_counts(addresses: usize, ports: usize, budget: u32, probes: u32) -> (usize, usize) {
    let ports = ports.max(1) as u32;
    let total = addresses as u32;
    if ports * total * (1 + probes) <= budget {
        return (addresses, addresses);
    }
    if probes > 0 && budget >= ports * (1 + probes) {
        let inspected = ((budget - ports * probes) / ports).min(total);
        let probed = ((budget - ports * inspected) / (ports * probes)).min(inspected);
        return (inspected as usize, probed as usize);
    }
    let inspected = (budget / ports).max(1).min(total.max(1)) as usize;
    (inspected, if probes == 0 { inspected } else { 0 })
}

/// The cost of `inspected` addresses, `probed` of them probed, over `ports` ports.
pub fn plan_cost(ports: usize, inspected: usize, probed: usize, probes: u32) -> u32 {
    ports.max(1) as u32 * (inspected as u32 + probes * probed as u32)
}

/// The plan for `ips` over `ports` ports within `budget` (see `plan_counts`): representative
/// addresses with the cap-and-warn warning when not all fit, and a warning when not every
/// inspected address is probed.
pub fn plan_budget(
    ips: &[IpAddr],
    ports: usize,
    budget: u32,
    probes: u32,
    warnings: &mut Vec<String>,
) -> Plan {
    let (inspected, probed) = plan_counts(ips.len(), ports, budget, probes);
    let (selected, skipped) = select_representative_ips(ips, inspected);
    if !skipped.is_empty() {
        warnings.push(format!(
            "rate limit: inspecting {} of {} IPs to stay within budget",
            selected.len(),
            selected.len() + skipped.len()
        ));
    }
    let probed = probed.min(selected.len());
    if probed < selected.len() {
        warnings.push(format!(
            "rate limit: probing {probed} of {} addresses to stay within budget",
            selected.len()
        ));
    }
    Plan {
        cost: plan_cost(ports, selected.len(), probed, probes),
        ips: selected,
        skipped: skipped.iter().map(|ip| ip.to_string()).collect(),
        probed,
    }
}

/// The inspection of `req.ips`: enrichment, CAA/TLSA/ECH, per-port handshakes, consistency,
/// validation, quality and the assembled response.
pub async fn inspect(
    state: &AppState,
    config: &Config,
    req: InspectRequest,
) -> Result<InspectResponse, AppError> {
    let InspectRequest {
        parsed,
        ips: inspected_ips,
        probed,
        mut warnings,
        skipped_ips,
        request_id,
        start: request_start,
        caa: known_caa,
        ech: known_ech,
    } = req;
    let hostname_str = match &parsed.target {
        Target::Hostname(h) => h.clone(),
        Target::Ip(ip) => ip.to_string(),
    };
    let input_mode = match &parsed.target {
        Target::Hostname(_) => "hostname",
        Target::Ip(_) => "ip",
    };

    // IP input warning
    if input_mode == "ip" {
        warnings.push(
            "IP address input: no SNI sent, results may differ from hostname-based access"
                .to_string(),
        );
    }

    // Spawn enrichment (concurrent with TLS + DNS)
    let enrichment_handle = if let Some(ref client) = state.enrichment_client {
        let client = Arc::clone(client);
        let ips = inspected_ips.clone();
        let rid = request_id.clone();
        Some(tokio::spawn(async move {
            client.lookup_batch(&ips, Some(&rid)).await
        }))
    } else {
        None
    };

    let handshake_timeout = Duration::from_secs(config.limits.handshake_timeout_secs);
    let request_timeout = Duration::from_secs(config.limits.request_timeout_secs);

    let is_hostname = input_mode == "hostname";
    let do_dns = is_hostname && state.dns_resolver.is_some();

    // Spawn DNS lookups on a blocking thread (mhost uses rand::thread_rng which
    // is !Send in rand 0.9, so the DNS future can't be sent across threads via tokio::spawn;
    // TODO: remove spawn_blocking once mhost upgrades to a Send-compatible rand version)
    let dns_handle = if do_dns {
        let resolver = state.dns_resolver.clone().unwrap();
        let hostname = parsed.target.hostname().unwrap().to_string();
        let ports = parsed.ports.clone();
        let check_caa = config.validation.check_caa;
        let check_dane = config.validation.check_dane;
        let rt = tokio::runtime::Handle::current();

        Some(tokio::task::spawn_blocking(move || {
            rt.block_on(async move {
                let mut errors = Vec::new();
                let (caa, caa_answered) = match (check_caa, known_caa) {
                    (false, _) => (None, None),
                    (true, Some(caa)) => (Some(caa), Some(CaaSource::Facts)),
                    (true, None) => match resolver.lookup_caa(&hostname).await {
                        Some((caa, answered)) => (Some(caa), Some(CaaSource::Lookup(answered))),
                        None => {
                            errors.push("caa".to_string());
                            (Some(CaaLookup { records: vec![] }), None)
                        }
                    },
                };

                let mut tlsa = HashMap::new();
                if check_dane {
                    for &port in &ports {
                        let lookup = resolver.lookup_tlsa(&hostname, port).await;
                        let lookup = lookup.unwrap_or_else(|| {
                            if !errors.iter().any(|e| e == "tlsa") {
                                errors.push("tlsa".to_string());
                            }
                            TlsaLookup {
                                records: vec![],
                                dnssec_signed: false,
                                answered: vec![],
                            }
                        });
                        tlsa.insert(port, lookup);
                    }
                }

                let ech = match known_ech {
                    Some(ech) => ech,
                    None => resolver
                        .lookup_ech_advertised(&hostname)
                        .await
                        .unwrap_or_else(|| {
                            errors.push("https".to_string());
                            false
                        }),
                };

                (caa, caa_answered, tlsa, ech, errors)
            })
        }))
    } else {
        None
    };

    // Run TLS handshakes with request timeout — ports run concurrently
    let port_results = tokio::time::timeout(request_timeout, async {
        let mut join_set = tokio::task::JoinSet::new();
        for &port in &parsed.ports {
            let ips = inspected_ips.clone();
            let hostname = parsed.target.hostname().map(|h| h.to_string());
            let trust_store = state.trust_store.load_full();
            let check_ct = config.validation.check_ct;
            let semaphore = Arc::clone(&state.handshake_semaphore);
            let probe_semaphore = Arc::clone(&state.probe_semaphore);
            let backend = Arc::clone(&state.backend);
            join_set.spawn(async move {
                inspect_port(
                    &ips,
                    port,
                    hostname.as_deref(),
                    handshake_timeout,
                    PortEnv {
                        trust_store,
                        check_ct,
                        semaphore,
                        probe_semaphore,
                        backend,
                        probed,
                    },
                )
                .await
            });
        }
        let mut results = Vec::with_capacity(parsed.ports.len());
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }
        results.sort_by_key(|r| r.port);
        results
    })
    .await
    .map_err(|_| AppError::RequestTimeout)?;

    // Collect DNS results
    let (caa_lookup, caa_answered, tlsa_lookups, ech_advertised, lookup_errors) =
        if let Some(handle) = dns_handle {
            match handle.await {
                Ok(result) => result,
                Err(e) => {
                    tracing::warn!(error = %e, "DNS task failed or was cancelled");
                    let enabled = [
                        ("caa", config.validation.check_caa),
                        (
                            "tlsa",
                            config.validation.check_dane && !parsed.ports.is_empty(),
                        ),
                        ("https", true),
                    ];
                    let errors = enabled
                        .into_iter()
                        .filter(|(_, on)| *on)
                        .map(|(name, _)| name.to_string())
                        .collect();
                    (None, None, HashMap::new(), false, errors)
                }
            }
        } else {
            (None, None, HashMap::new(), false, Vec::new())
        };

    // Collect enrichment results
    let enrichments = if let Some(handle) = enrichment_handle {
        match handle.await {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(error = %e, "enrichment task failed or was cancelled");
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    // Extract leaf issuer for CAA checking
    let leaf = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|r| r.chain.is_some())
        .and_then(|r| r.chain.as_ref()?.first());
    let leaf_issuer: Option<String> = leaf.map(|leaf| leaf.issuer.clone());
    let leaf_sans: Vec<String> = leaf.map(|leaf| leaf.sans.clone()).unwrap_or_default();

    // Compute CAA check status
    let caa_status = match (&caa_lookup, &leaf_issuer) {
        (Some(caa), Some(issuer)) => {
            validate::caa_compliance::check_caa_compliance(caa, issuer, &leaf_sans)
        }
        (Some(caa), None) if caa.is_empty() => validate::CheckStatus::Pass,
        (Some(_), None) => validate::CheckStatus::Skip, // Have records but no cert to check against
        (None, _) => validate::CheckStatus::Skip,
    };

    // Compute per-port DANE status by matching TLSA records against the presented cert chain.
    // DNSSEC signature verification is not yet available, so dnssec_signed is always false for now.
    // We perform structural DANE matching (RFC 6698) regardless; callers can check dnssec_signed.
    let mut port_dane_valid: HashMap<u16, Option<bool>> = HashMap::new();
    for pr in &port_results {
        if let Some(tlsa) = tlsa_lookups.get(&pr.port)
            && !tlsa.is_empty()
        {
            // Take raw certs from the first successful IP result for this port.
            let raw_certs_opt = pr
                .ips
                .iter()
                .find(|r| r.error.is_none())
                .and_then(|r| r.raw_certs.as_deref());

            let dane_ok = if let Some(raw_certs) = raw_certs_opt {
                tlsa.records
                    .iter()
                    .any(|record| validate::dane::dane_match(record, raw_certs))
            } else {
                false
            };
            port_dane_valid.insert(pr.port, Some(dane_ok));
        }
    }

    // Global DANE status: Pass if all ports with TLSA records matched; Skip if none had records.
    let dane_status = {
        let results: Vec<bool> = port_dane_valid.values().filter_map(|v| *v).collect();
        if results.is_empty() {
            validate::CheckStatus::Skip
        } else if results.iter().all(|&ok| ok) {
            validate::CheckStatus::Pass
        } else {
            validate::CheckStatus::Fail
        }
    };

    // Build DNS context for response
    let dns_context = if do_dns {
        let caa_info = caa_lookup.as_ref().map(|caa| {
            let issuer_allowed = leaf_issuer.as_ref().map(|issuer| {
                validate::caa_compliance::check_caa_compliance(caa, issuer, &leaf_sans)
                    == validate::CheckStatus::Pass
            });
            CaaInfo {
                records: caa
                    .records
                    .iter()
                    .map(|r| format!("{} \"{}\"", r.tag, r.value))
                    .collect(),
                issuer_allowed,
                issuewild_present: caa.issuewild_present(),
                answered: caa_answered.clone(),
                lookup: Some(caa.clone()),
            }
        });

        Some(DnsContext {
            caa: caa_info,
            resolved_ips: inspected_ips.iter().map(|ip| ip.to_string()).collect(),
            lookup_errors,
        })
    } else {
        None
    };

    // Inject enrichment data and ECH into IP results
    let mut port_results = port_results;
    let ech_for_port = if is_hostname && do_dns {
        Some(ech_advertised)
    } else {
        None
    };
    for pr in &mut port_results {
        for ip_result in &mut pr.ips {
            if let Ok(ip) = ip_result.ip.parse::<IpAddr>()
                && !enrichments.is_empty()
            {
                ip_result.enrichment = enrichments.get(&ip).cloned();
            }
            // Set ECH and STARTTLS on TLS params
            if let Some(ref mut tls) = ip_result.tls {
                tls.ech_advertised = ech_for_port;
                // STARTTLS: set based on port
                if tls.starttls.is_none() {
                    tls.starttls = starttls_protocol_for_port(pr.port);
                }
            }
        }
    }

    // Build per-port TLSA info
    for pr in &mut port_results {
        if let Some(tlsa) = tlsa_lookups.get(&pr.port)
            && !tlsa.is_empty()
        {
            pr.tlsa = Some(TlsaInfo {
                records: tlsa.records.iter().map(|r| r.display.clone()).collect(),
                dnssec_signed: tlsa.dnssec_signed,
                dane_valid: port_dane_valid.get(&pr.port).copied().flatten(),
                answered: Some(tlsa.answered.clone()),
            });
        }
    }

    // Quality assessment (always when enabled in config)
    let do_quality = config.quality.enabled;

    let hostname_quality = if do_quality {
        let is_hn = is_hostname;
        let skip_http = config.quality.skip_http_checks || !is_hn;
        if skip_http {
            Some(crate::quality::assess_hostname(None, None))
        } else {
            let hsts_port = crate::quality::hsts_port(&parsed.ports);
            let first_ip = inspected_ips[0];
            let hostname_clone = hostname_str.clone();
            let http_timeout = Duration::from_secs(config.quality.http_check_timeout_secs);

            let hsts_connector = Arc::clone(&state.hsts_tls_connector);
            let (hsts_result, redirect_result) = tokio::join!(
                crate::quality::http::check_hsts(
                    first_ip,
                    &hostname_clone,
                    hsts_port,
                    http_timeout,
                    &hsts_connector,
                ),
                crate::quality::http::check_https_redirect(first_ip, &hostname_clone, http_timeout,),
            );
            Some(crate::quality::assess_hostname(
                Some(hsts_result),
                Some(redirect_result),
            ))
        }
    } else {
        None
    };

    // Compute summary from first successful port result
    let (validation_ref, ocsp_stapled, has_ocsp_url) = port_results
        .iter()
        .flat_map(|pr| pr.ips.iter())
        .find(|ip_result| ip_result.error.is_none())
        .map(|ip_result| {
            (
                &ip_result.validation,
                ip_result.ocsp_stapled(),
                ip_result.leaf_has_ocsp_url(),
            )
        })
        .unwrap_or((&None, false, false));

    // Compute CT status from first successful IP result
    let ct_status = if config.validation.check_ct {
        let ct_info = port_results
            .iter()
            .flat_map(|pr| pr.ips.iter())
            .find(|r| r.error.is_none())
            .and_then(|r| r.ct.as_ref());
        validate::ct::check_ct_status(ct_info)
    } else {
        validate::CheckStatus::Skip
    };

    // Check if any port has consistency mismatches
    let has_consistency_mismatch = port_results.iter().any(|pr| {
        pr.consistency
            .as_ref()
            .is_some_and(|c| !c.mismatches.is_empty())
    });

    let summary = validate::summarize(
        validation_ref.as_ref(),
        parsed.target.hostname(),
        ocsp_stapled,
        has_ocsp_url,
        has_consistency_mismatch,
        caa_status,
        dane_status,
        ct_status,
    );

    // Per-port quality assessment
    if do_quality {
        let ct_enabled = config.validation.check_ct;
        for pr in &mut port_results {
            let port_quality = crate::quality::assess_port(
                &pr.ips,
                pr.port,
                is_hostname,
                caa_status,
                dane_status,
                ocsp_stapled,
                pr.consistency.as_ref(),
                ct_enabled,
                if is_hostname {
                    parsed.target.hostname().unwrap_or("")
                } else {
                    ""
                },
            );
            pr.quality = Some(port_quality);
        }
    }

    let duration_ms = request_start.elapsed().as_millis() as u64;

    metrics::histogram!("netray_tls_inspect_duration_ms").record(duration_ms as f64);
    metrics::counter!("netray_tls_ips_inspected_total").increment(inspected_ips.len() as u64);
    if !skipped_ips.is_empty() {
        metrics::counter!("netray_tls_skipped_ips_total").increment(skipped_ips.len() as u64);
    }

    tracing::info!(
        hostname = %hostname_str,
        ip_count = inspected_ips.len(),
        duration_ms = duration_ms,
        request_id = %request_id,
        "inspect complete"
    );

    Ok(InspectResponse {
        request_id,
        hostname: hostname_str,
        input_mode,
        summary,
        ports: port_results,
        dns: dns_context,
        quality: hostname_quality,
        warnings,
        skipped_ips,
        duration_ms,
    })
}

/// Map well-known STARTTLS ports to a protocol name.
fn starttls_protocol_for_port(port: u16) -> Option<String> {
    match port {
        25 | 587 => Some("smtp".to_string()),
        143 => Some("imap".to_string()),
        21 => Some("ftp".to_string()),
        _ => None,
    }
}

/// The handshake outcome of each address of a port, in selection order: which addresses get the
/// charged probe slots.
struct SlotBoard {
    /// `None` while the handshake runs, then whether it succeeded.
    done: std::sync::Mutex<Vec<Option<bool>>>,
    changed: tokio::sync::Notify,
}

impl SlotBoard {
    fn new(addresses: usize) -> Self {
        Self {
            done: std::sync::Mutex::new(vec![None; addresses]),
            changed: tokio::sync::Notify::new(),
        }
    }

    fn finish(&self, idx: usize, succeeded: bool) {
        self.done.lock().expect("slot board lock")[idx] = Some(succeeded);
        self.changed.notify_waiters();
    }

    /// Whether the address at `idx`, whose handshake succeeded, is among the first `slots`
    /// successes: decided as soon as the earlier addresses that are still running can no longer
    /// change the answer.
    async fn claim(&self, idx: usize, slots: usize) -> bool {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let done = self.done.lock().expect("slot board lock");
                let earlier = &done[..idx];
                let succeeded = earlier.iter().filter(|d| **d == Some(true)).count();
                let running = earlier.iter().filter(|d| d.is_none()).count();
                if succeeded >= slots {
                    return false;
                }
                if succeeded + running < slots {
                    return true;
                }
            }
            changed.await;
        }
    }
}

/// How long past its own timeout a backend's probes may take before their answers are dropped:
/// `probe_address` returns only after its per-probe timers, started after the call, fire.
const PROBE_GRACE: Duration = Duration::from_millis(250);

/// What a port's handshakes share: trust anchors, the CT switch, the handshake permits and
/// the backend that performs them.
struct PortEnv {
    trust_store: Arc<rustls::RootCertStore>,
    check_ct: bool,
    semaphore: Arc<tokio::sync::Semaphore>,
    /// Permits of the addresses probing at once, apart from the handshake permits.
    probe_semaphore: Arc<tokio::sync::Semaphore>,
    backend: Arc<dyn TlsBackend>,
    /// How many addresses' probes the plan charged; the slots go to the first addresses whose
    /// handshake succeeded.
    probed: usize,
}

/// Inspect a single port across all IPs.
async fn inspect_port(
    ips: &[std::net::IpAddr],
    port: u16,
    hostname: Option<&str>,
    timeout: Duration,
    env: PortEnv,
) -> PortResult {
    let PortEnv {
        trust_store,
        check_ct,
        semaphore,
        probe_semaphore,
        backend,
        probed,
    } = env;
    // The probe slots the plan charged go to the first addresses, in selection order, whose
    // handshake succeeded. An address knows its slot once every earlier address has finished its
    // handshake; its probe deadline runs from its own handshake, so waiting on a slow earlier
    // address costs probe time, never the inspection's.
    let slots = Arc::new(SlotBoard::new(ips.len()));
    let mut join_set = tokio::task::JoinSet::new();
    for (idx, &ip) in ips.iter().enumerate() {
        let semaphore = Arc::clone(&semaphore);
        let probe_semaphore = Arc::clone(&probe_semaphore);
        let trust_store = Arc::clone(&trust_store);
        let backend = Arc::clone(&backend);
        let slots = Arc::clone(&slots);
        let hostname = hostname.map(|h| h.to_string());
        join_set.spawn(async move {
            // The handshake permit covers the handshake only.
            let mut result = {
                let _permit = semaphore.acquire().await.ok();
                backend
                    .handshake(ip, port, hostname.as_deref(), timeout)
                    .await
            };
            if result.error.is_some() {
                metrics::counter!("netray_tls_handshake_errors_total").increment(1);
            }
            let version = result
                .tls
                .as_ref()
                .filter(|_| result.error.is_none())
                .map(|t| t.version.clone());
            slots.finish(idx, version.is_some());
            let deadline = tokio::time::Instant::now() + timeout;

            // Run validation if we got a chain
            if let Some(chain) = &result.chain {
                let raw_certs = result.raw_certs.as_deref().unwrap_or(&[]);
                let validation = validate::chain_trust::validate_chain(
                    chain,
                    hostname.as_deref(),
                    trust_store.as_ref(),
                    raw_certs,
                );
                result.validation = Some(validation);

                // Extract SCTs from leaf certificate
                if check_ct && let Some(leaf_der) = raw_certs.first() {
                    result.ct = validate::ct::extract_ct_info(leaf_der.as_ref());
                }
            }

            // Live OCSP revocation check via AIA OCSP URL, beside the protocol probes
            let live_ocsp = async {
                let chain = result.chain.as_ref()?;
                let raw_certs = result.raw_certs.as_deref().unwrap_or(&[]);
                let ocsp_url = chain.first().and_then(|c| c.ocsp_url.clone())?;
                let leaf_der = raw_certs.first().map(|c| c.as_ref().to_vec())?;
                let issuer_der = raw_certs.get(1).map(|c| c.as_ref().to_vec())?;
                Some(tls::ocsp::check_live_ocsp(&ocsp_url, &leaf_der, &issuer_der).await)
            };
            // The probes take a probe permit; the wait and the probes together are bounded by
            // the handshake timeout from the end of the handshake: the slot and permit waits by
            // the deadline, the probes by the time left, which each probe enforces itself, so
            // answers that arrived are kept. Past the deadline every kind the address would have
            // probed is unanswered (Rule M). The grace only backstops a backend that overruns
            // its own timeout.
            let probes = async {
                let version = version.as_ref()?;
                if !slots.claim(idx, probed).await {
                    return None;
                }
                let timed_out = || {
                    (backend.probe_connections() > 0)
                        .then(|| tls::probe::ProtocolProbes::unanswered(version, "timed out"))
                };
                let Ok(_permit) =
                    tokio::time::timeout_at(deadline, probe_semaphore.acquire()).await
                else {
                    return timed_out();
                };
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                if left.is_zero() {
                    return timed_out();
                }
                let run = backend.probe(ip, port, hostname.as_deref(), version, left);
                tokio::time::timeout(left + PROBE_GRACE, run)
                    .await
                    .unwrap_or_else(|_| timed_out())
            };
            let (live_ocsp, probes) = tokio::join!(live_ocsp, probes);
            if let (Some(ocsp_result), Some(tls_params)) = (live_ocsp, result.tls.as_mut()) {
                tls_params.ocsp_live = Some(ocsp_result);
            }
            result.probes = probes;
            (idx, result)
        });
    }

    let mut indexed: Vec<(usize, tls::IpInspectionResult)> =
        join_set.join_all().await.into_iter().collect();
    indexed.sort_by_key(|(idx, _)| *idx);
    let ip_results: Vec<tls::IpInspectionResult> = indexed.into_iter().map(|(_, r)| r).collect();

    let consistency = compute_consistency(&ip_results);

    PortResult {
        port,
        ips: ip_results,
        consistency,
        validation: None,
        tlsa: None,
        quality: None,
        error: None,
    }
}

/// Compute consistency across IP results for a single port.
///
/// Only considers successful IPs (those with `tls` present).
/// Returns `None` if fewer than 2 IPs succeeded.
pub(crate) fn compute_consistency(
    ip_results: &[tls::IpInspectionResult],
) -> Option<ConsistencyResult> {
    let successful: Vec<_> = ip_results.iter().filter(|r| r.tls.is_some()).collect();

    if successful.len() < 2 {
        return None;
    }

    let mut mismatches = Vec::new();

    // Compare leaf cert fingerprints
    let fingerprints: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let fp = r.chain.as_ref()?.first()?.fingerprint_sha256.clone();
            Some((r.ip.clone(), fp))
        })
        .collect();
    let certs_match = fingerprints
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !certs_match {
        mismatches.push(ConsistencyMismatch {
            field: "leaf_certificate".to_string(),
            values: fingerprints,
        });
    }

    // Compare TLS versions
    let versions: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let v = r.tls.as_ref()?.version.clone();
            Some((r.ip.clone(), v))
        })
        .collect();
    let versions_match = versions
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !versions_match {
        mismatches.push(ConsistencyMismatch {
            field: "tls_version".to_string(),
            values: versions,
        });
    }

    // Compare cipher suites
    let ciphers: HashMap<String, String> = successful
        .iter()
        .filter_map(|r| {
            let c = r.tls.as_ref()?.cipher_suite.clone();
            Some((r.ip.clone(), c))
        })
        .collect();
    let ciphers_match = ciphers
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
        <= 1;
    if !ciphers_match {
        mismatches.push(ConsistencyMismatch {
            field: "cipher_suite".to_string(),
            values: ciphers,
        });
    }

    Some(ConsistencyResult {
        certificates_match: certs_match,
        tls_versions_match: versions_match,
        cipher_suites_match: ciphers_match,
        mismatches,
    })
}

/// Resolve a hostname to IP addresses using the mhost resolver from AppState.
///
/// Falls back to an error if the resolver is unavailable or returns no addresses.
/// IP filtering (DNS rebinding protection) is applied by the caller via
/// `target_policy::check_allowed_with_policy` immediately after this returns.
pub(crate) async fn resolve_hostname(
    hostname: &str,
    resolver: &Arc<crate::dns::DnsResolver>,
) -> Result<Vec<std::net::IpAddr>, AppError> {
    let resolver = Arc::clone(resolver);
    let hostname_owned = hostname.to_string();

    // Spawn on a blocking thread because mhost uses rand::thread_rng which is !Send in rand 0.9.
    // This matches the existing DNS task pattern used for CAA/TLSA in do_inspect.
    let rt = tokio::runtime::Handle::current();
    let ips =
        tokio::task::spawn_blocking(move || rt.block_on(resolver.lookup_ips(&hostname_owned)))
            .await
            .map_err(|e| {
                AppError::DnsResolutionFailed(format!("{hostname}: task join error: {e}"))
            })?;

    if ips.is_empty() {
        return Err(AppError::DnsResolutionFailed(format!(
            "no addresses found for {hostname}"
        )));
    }
    Ok(ips)
}

// ---------------------------------------------------------------------------
// IpInspectionResult helper
// ---------------------------------------------------------------------------

impl tls::IpInspectionResult {
    fn ocsp_stapled(&self) -> bool {
        self.tls.as_ref().is_some_and(|t| t.ocsp.stapled)
    }

    fn leaf_has_ocsp_url(&self) -> bool {
        self.chain
            .as_ref()
            .and_then(|c| c.first())
            .is_some_and(|leaf| leaf.ocsp_url.is_some())
    }
}
