//! Check orchestration and category model.
//!
//! Beacon evaluates twelve email-security categories in three phases. The
//! eleven submodules listed below each implement one category; the
//! [`cross_validation`] module adds the twelfth category as a consistency
//! pass over the other eleven:
//!
//! - Phase 0 (domain-only, run in parallel): [`mx`], [`spf`], [`dmarc`],
//!   [`tls_rpt`], [`dnssec`], [`bimi`].
//! - Phase 1 (MX-dependent, run in parallel once phase 0 has drained):
//!   [`dkim`], [`mta_sts`], [`dane`], [`fcrdns`], [`dnsbl`].
//! - Phase 2 (sequential): [`cross_validation`] correlates the previous
//!   eleven results, then [`run_all_checks`] computes the final
//!   [`crate::quality::Grade`] and emits the `Summary` SSE event.
//!
//! Each phase uses a `tokio::task::JoinSet` so an individual check panic is
//! isolated and surfaced as `Verdict::Skip` without aborting the whole
//! inspection. The entire pipeline is wrapped in a 30-second
//! `tokio::time::timeout`; on expiry all twelve categories fall back to
//! `Verdict::Skip` and `Grade::Skipped` is reported. Results stream to the
//! caller over `mpsc::Sender<SseEvent>` so the frontend can render each
//! category as it lands.
//!
//! The [`util`] submodule holds small shared helpers (e.g. TXT tag
//! parsing).

pub mod bimi;
pub mod cross_validation;
pub mod dane;
pub mod dkim;
pub mod dmarc;
pub mod dnsbl;
pub mod dnssec;
pub mod fcrdns;
pub mod mta_sts;
pub mod mx;
pub mod spf;
pub mod tls_rpt;
pub mod util;

#[cfg(test)]
mod bimi_results_table;
#[cfg(test)]
mod dkim_results_table;
#[cfg(test)]
mod mta_sts_results_table;
#[cfg(test)]
mod outbound_fetch_scenarios;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tracing::Instrument;

use crate::config::Config;
use crate::dns::DnsLookup;
use crate::quality::{AllResults, Category, CheckResult, Grade, SseEvent, Verdict, compute_grade};
use crate::state::OutboundFetch;
use netray_common::enrichment::EnrichmentClient;

/// Run all email security checks for a domain, streaming results via mpsc channel.
/// Wraps `run_inspection_inner` with a 30-second timeout.
#[allow(clippy::too_many_arguments)]
pub async fn run_all_checks<R: DnsLookup + 'static>(
    domain: String,
    selectors: Vec<String>,
    config: Arc<Config>,
    dns: Arc<R>,
    dnsbl_dns: Arc<R>,
    fetch: OutboundFetch,
    enrichment_client: Option<Arc<EnrichmentClient>>,
    tx: mpsc::Sender<SseEvent>,
) {
    let timeout_result = tokio::time::timeout(
        Duration::from_secs(30),
        run_inspection_inner(
            domain.clone(),
            selectors,
            config,
            dns,
            dnsbl_dns,
            fetch,
            enrichment_client,
            tx.clone(),
        ),
    )
    .await;

    if timeout_result.is_err() {
        tracing::warn!(duration_ms = 30_000, "inspection timed out after 30s");
        // Emit a partial summary with Skip verdicts for any missing checks
        let verdicts: HashMap<String, Verdict> = [
            "mx",
            "spf",
            "dkim",
            "dmarc",
            "mta_sts",
            "tls_rpt",
            "dane",
            "dnssec",
            "bimi",
            "fcrdns",
            "dnsbl",
            "cross_validation",
        ]
        .iter()
        .map(|k| (k.to_string(), Verdict::Skip))
        .collect();

        let _ = tx
            .send(SseEvent::Summary {
                grade: Grade::Skipped,
                verdicts,
                duration_ms: 30_000,
            })
            .await;
    }
}

/// Tagged result for phase-0 parallel checks.
enum Phase0Result {
    Mx {
        result: CheckResult,
        mx_hosts: Vec<String>,
        mx_ips: Vec<IpAddr>,
        null_mx: bool,
    },
    Spf {
        result: CheckResult,
        flat: Option<crate::quality::SpfFlat>,
        has_dash_all: bool,
        only_dash_all: bool,
    },
    Dmarc {
        result: CheckResult,
        policy: Option<String>,
        sp: Option<String>,
        rua_ok: bool,
    },
    TlsRpt {
        result: CheckResult,
        present: bool,
    },
    Dnssec {
        result: CheckResult,
        dnskey_present: bool,
    },
    Bimi {
        result: CheckResult,
        present: bool,
    },
}

impl Phase0Result {
    fn result(&self) -> &CheckResult {
        match self {
            Phase0Result::Mx { result, .. }
            | Phase0Result::Spf { result, .. }
            | Phase0Result::Dmarc { result, .. }
            | Phase0Result::TlsRpt { result, .. }
            | Phase0Result::Dnssec { result, .. }
            | Phase0Result::Bimi { result, .. } => result,
        }
    }
}

/// Tagged result for phase-1 parallel checks.
enum Phase1Result {
    Dkim {
        result: CheckResult,
        found: bool,
    },
    MtaSts {
        result: CheckResult,
        present: bool,
        info: Option<crate::quality::MtaStsInfo>,
    },
    Dane {
        result: CheckResult,
        has_tlsa: bool,
    },
    Other(CheckResult),
}

impl Phase1Result {
    fn result(&self) -> &CheckResult {
        match self {
            Phase1Result::Dkim { result, .. }
            | Phase1Result::MtaSts { result, .. }
            | Phase1Result::Dane { result, .. } => result,
            Phase1Result::Other(result) => result,
        }
    }
}

/// Sub-check name of a category that did not run; lens relies on it.
pub const SKIPPED: &str = "skipped";

fn skip_result(category: Category) -> CheckResult {
    CheckResult::new(
        category,
        vec![crate::quality::SubCheck {
            name: SKIPPED.to_string(),
            verdict: Verdict::Skip,
            detail: "check did not complete in time".to_string(),
        }],
        "skipped".to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
async fn run_inspection_inner<R: DnsLookup + 'static>(
    domain: String,
    selectors: Vec<String>,
    config: Arc<Config>,
    dns: Arc<R>,
    dnsbl_dns: Arc<R>,
    fetch: OutboundFetch,
    enrichment_client: Option<Arc<EnrichmentClient>>,
    tx: mpsc::Sender<SseEvent>,
) {
    let inspection_start = std::time::Instant::now();

    // Accumulated phase-0 data
    let mut mx_result: Option<CheckResult> = None;
    let mut mx_hosts: Vec<String> = Vec::new();
    let mut mx_ips: Vec<IpAddr> = Vec::new();
    let mut null_mx = false;
    let mut spf_result: Option<CheckResult> = None;
    let mut spf_flat: Option<crate::quality::SpfFlat> = None;
    let mut spf_has_dash_all = false;
    let mut spf_only_dash_all = false;
    let mut dmarc_result: Option<CheckResult> = None;
    let mut dmarc_policy: Option<String> = None;
    let mut dmarc_sp: Option<String> = None;
    let mut dmarc_rua_external_auth_ok = true;
    let mut tls_rpt_result: Option<CheckResult> = None;
    let mut tls_rpt_present = false;
    let mut dnssec_result: Option<CheckResult> = None;
    let mut dnssec_dnskey_present = false;
    let mut bimi_result: Option<CheckResult> = None;
    let mut bimi_present = false;

    // Phase 0: domain-only checks in parallel
    let mut phase0: JoinSet<Phase0Result> = JoinSet::new();
    let mut phase0_categories: HashMap<tokio::task::Id, &'static str> = HashMap::new();

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let enrichment_client = enrichment_client.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, ips, hosts, is_null_mx) =
                    mx::check_mx(&domain, dns.as_ref(), enrichment_client.as_deref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "mx")
                    .record(elapsed);
                Phase0Result::Mx {
                    result,
                    mx_hosts: hosts,
                    mx_ips: ips,
                    null_mx: is_null_mx,
                }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "mx");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, flat, dash_all, only_dash_all) =
                    spf::check_spf_detailed(&domain, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "spf")
                    .record(elapsed);
                Phase0Result::Spf {
                    result,
                    flat,
                    has_dash_all: dash_all,
                    only_dash_all,
                }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "spf");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, policy, sp, rua_ok) = dmarc::check_dmarc(&domain, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "dmarc")
                    .record(elapsed);
                Phase0Result::Dmarc {
                    result,
                    policy,
                    sp,
                    rua_ok,
                }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "dmarc");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, present) = tls_rpt::check_tls_rpt(&domain, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "tls_rpt")
                    .record(elapsed);
                Phase0Result::TlsRpt { result, present }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "tls_rpt");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, ad) = dnssec::check_dnssec(&domain, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "dnssec")
                    .record(elapsed);
                Phase0Result::Dnssec {
                    result,
                    dnskey_present: ad,
                }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "dnssec");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let fetch = fetch.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, present) = bimi::check_bimi(&domain, dns.as_ref(), &fetch).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "bimi")
                    .record(elapsed);
                Phase0Result::Bimi { result, present }
            }
            .instrument(tracing::Span::current()),
        );
        phase0_categories.insert(handle.id(), "bimi");
    }

    // Drain phase 0, sending each result immediately
    while let Some(join_result) = phase0.join_next_with_id().await {
        match join_result {
            Ok((_id, output)) => {
                if tx
                    .send(SseEvent::Category(output.result().clone()))
                    .await
                    .is_err()
                {
                    phase0.abort_all();
                    return;
                }
                match output {
                    Phase0Result::Mx {
                        result,
                        mx_hosts: hosts,
                        mx_ips: ips,
                        null_mx: is_null_mx,
                    } => {
                        mx_result = Some(result);
                        mx_hosts = hosts;
                        mx_ips = ips;
                        null_mx = is_null_mx;
                    }
                    Phase0Result::Spf {
                        result,
                        flat,
                        has_dash_all,
                        only_dash_all,
                    } => {
                        spf_result = Some(result);
                        spf_flat = flat;
                        spf_has_dash_all = has_dash_all;
                        spf_only_dash_all = only_dash_all;
                    }
                    Phase0Result::Dmarc {
                        result,
                        policy,
                        sp,
                        rua_ok,
                    } => {
                        dmarc_result = Some(result);
                        dmarc_policy = policy;
                        dmarc_sp = sp;
                        dmarc_rua_external_auth_ok = rua_ok;
                    }
                    Phase0Result::TlsRpt { result, present } => {
                        tls_rpt_result = Some(result);
                        tls_rpt_present = present;
                    }
                    Phase0Result::Dnssec {
                        result,
                        dnskey_present,
                    } => {
                        dnssec_result = Some(result);
                        dnssec_dnskey_present = dnskey_present;
                    }
                    Phase0Result::Bimi { result, present } => {
                        bimi_result = Some(result);
                        bimi_present = present;
                    }
                }
            }
            Err(e) => {
                let category = phase0_categories.get(&e.id()).copied().unwrap_or("unknown");
                if e.is_panic() {
                    metrics::counter!(
                        "beacon_check_task_panics_total",
                        "category" => category,
                    )
                    .increment(1);
                }
                tracing::error!(error = %e, category, "phase-0 check task panicked");
            }
        }
    }

    let sends_no_mail = null_mx || spf_only_dash_all;

    // Accumulated phase-1 data
    let mut dkim_result: Option<CheckResult> = None;
    let mut dkim_found = false;
    let mut mta_sts_result: Option<CheckResult> = None;
    let mut mta_sts_present = false;
    let mut mta_sts_info: Option<crate::quality::MtaStsInfo> = None;
    let mut dane_result: Option<CheckResult> = None;
    let mut dane_has_tlsa = false;
    let mut fcrdns_result: Option<CheckResult> = None;
    let mut dnsbl_result: Option<CheckResult> = None;

    // Phase 1: MX-dependent checks in parallel
    let mut phase1: JoinSet<Phase1Result> = JoinSet::new();
    let mut phase1_categories: HashMap<tokio::task::Id, &'static str> = HashMap::new();

    {
        let domain = domain.clone();
        let mx_hosts_clone = mx_hosts.clone();
        let selectors = selectors.clone();
        let dns = dns.clone();
        let max_sels = config.dkim.max_user_selectors;
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, found) = dkim::check_dkim(
                    &domain,
                    &mx_hosts_clone,
                    &selectors,
                    max_sels,
                    dns.as_ref(),
                    sends_no_mail,
                )
                .await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "dkim")
                    .record(elapsed);
                Phase1Result::Dkim { result, found }
            }
            .instrument(tracing::Span::current()),
        );
        phase1_categories.insert(handle.id(), "dkim");
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let fetch = fetch.clone();
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, info) = mta_sts::check_mta_sts(&domain, dns.as_ref(), &fetch).await;
                let present = info.is_some();
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "mta_sts")
                    .record(elapsed);
                Phase1Result::MtaSts {
                    result,
                    present,
                    info,
                }
            }
            .instrument(tracing::Span::current()),
        );
        phase1_categories.insert(handle.id(), "mta_sts");
    }

    {
        let mx_hosts_clone = mx_hosts.clone();
        let dns = dns.clone();
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, has_tlsa) = dane::check_dane(&mx_hosts_clone, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "dane")
                    .record(elapsed);
                Phase1Result::Dane { result, has_tlsa }
            }
            .instrument(tracing::Span::current()),
        );
        phase1_categories.insert(handle.id(), "dane");
    }

    {
        let mx_ips_clone = mx_ips.clone();
        let dns = dns.clone();
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let result = fcrdns::check_fcrdns(&mx_ips_clone, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "fcrdns")
                    .record(elapsed);
                Phase1Result::Other(result)
            }
            .instrument(tracing::Span::current()),
        );
        phase1_categories.insert(handle.id(), "fcrdns");
    }

    {
        let mx_ips_clone = mx_ips.clone();
        let domain = domain.clone();
        let dnsbl_config = config.dnsbl.clone();
        let dns = dnsbl_dns.clone();
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let result =
                    dnsbl::check_dnsbl(&mx_ips_clone, &domain, &dnsbl_config, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "dnsbl")
                    .record(elapsed);
                Phase1Result::Other(result)
            }
            .instrument(tracing::Span::current()),
        );
        phase1_categories.insert(handle.id(), "dnsbl");
    }

    // Drain phase 1, sending each result immediately
    while let Some(join_result) = phase1.join_next_with_id().await {
        match join_result {
            Ok((_id, output)) => {
                if tx
                    .send(SseEvent::Category(output.result().clone()))
                    .await
                    .is_err()
                {
                    phase1.abort_all();
                    return;
                }
                match output {
                    Phase1Result::Dkim { result, found } => {
                        dkim_result = Some(result);
                        dkim_found = found;
                    }
                    Phase1Result::MtaSts {
                        result,
                        present,
                        info,
                    } => {
                        mta_sts_result = Some(result);
                        mta_sts_present = present;
                        mta_sts_info = info;
                    }
                    Phase1Result::Dane { result, has_tlsa } => {
                        dane_result = Some(result);
                        dane_has_tlsa = has_tlsa;
                    }
                    Phase1Result::Other(result) => match result.category {
                        Category::Fcrdns => {
                            fcrdns_result = Some(result);
                        }
                        Category::Dnsbl => {
                            dnsbl_result = Some(result);
                        }
                        _ => {
                            tracing::error!(
                                category = ?result.category,
                                "unexpected phase-1 Other category"
                            );
                        }
                    },
                }
            }
            Err(e) => {
                let category = phase1_categories.get(&e.id()).copied().unwrap_or("unknown");
                if e.is_panic() {
                    metrics::counter!(
                        "beacon_check_task_panics_total",
                        "category" => category,
                    )
                    .increment(1);
                }
                tracing::error!(error = %e, category, "phase-1 check task panicked");
            }
        }
    }

    // Phase 2: sequential cross-validation and grade. A category whose task did
    // not finish falls back to `skip_result`, and its event is sent too.
    let mut skipped: Vec<CheckResult> = Vec::new();
    let mut or_skip = |result: Option<CheckResult>, category: Category| {
        result.unwrap_or_else(|| {
            let r = skip_result(category);
            skipped.push(r.clone());
            r
        })
    };
    let mx_r = or_skip(mx_result, Category::Mx);
    let spf_r = or_skip(spf_result, Category::Spf);
    let dkim_r = or_skip(dkim_result, Category::Dkim);
    let dmarc_r = or_skip(dmarc_result, Category::Dmarc);
    let mta_sts_r = or_skip(mta_sts_result, Category::MtaSts);
    let tls_rpt_r = or_skip(tls_rpt_result, Category::TlsRpt);
    let dane_r = or_skip(dane_result, Category::Dane);
    let dnssec_r = or_skip(dnssec_result, Category::Dnssec);
    let bimi_r = or_skip(bimi_result, Category::Bimi);
    let fcrdns_r = or_skip(fcrdns_result, Category::Fcrdns);
    let dnsbl_r = or_skip(dnsbl_result, Category::Dnsbl);

    for result in skipped {
        if tx.send(SseEvent::Category(result)).await.is_err() {
            return;
        }
    }

    let fcrdns_all_pass = fcrdns_r
        .sub_checks
        .iter()
        .all(|s| s.verdict == Verdict::Pass);

    let all_results = AllResults {
        mx: mx_r,
        mx_hosts,
        mx_ips,
        null_mx,
        sends_no_mail,
        spf: spf_r,
        spf_flat,
        spf_has_dash_all,
        dkim: dkim_r,
        dkim_found,
        dmarc: dmarc_r,
        dmarc_policy,
        dmarc_sp,
        dmarc_rua_external_auth_ok,
        mta_sts: mta_sts_r,
        mta_sts_present,
        mta_sts_info,
        tls_rpt: tls_rpt_r,
        tls_rpt_present,
        dane: dane_r,
        dane_has_tlsa,
        dnssec: dnssec_r,
        dnssec_dnskey_present,
        bimi: bimi_r,
        bimi_present,
        fcrdns: fcrdns_r,
        fcrdns_all_pass,
        dnsbl: dnsbl_r,
    };

    let cross_start = std::time::Instant::now();
    let cross_result = cross_validation::cross_validate(&all_results);
    let cross_elapsed = cross_start.elapsed().as_secs_f64();
    metrics::histogram!("beacon_check_duration_seconds", "category" => "cross_validation")
        .record(cross_elapsed);

    if tx
        .send(SseEvent::Category(cross_result.clone()))
        .await
        .is_err()
    {
        return;
    }

    // Collect all verdicts for grade computation
    let all_check_results = [
        &all_results.mx,
        &all_results.spf,
        &all_results.dkim,
        &all_results.dmarc,
        &all_results.mta_sts,
        &all_results.tls_rpt,
        &all_results.dane,
        &all_results.dnssec,
        &all_results.bimi,
        &all_results.fcrdns,
        &all_results.dnsbl,
        &cross_result,
    ];

    let verdicts_list: Vec<Verdict> = all_check_results.iter().map(|r| r.verdict).collect();
    let grade = compute_grade(&verdicts_list);
    metrics::counter!("beacon_grade_total", "grade" => grade.as_str()).increment(1);

    let verdicts_map: HashMap<String, Verdict> = all_check_results
        .iter()
        .map(|r| {
            let key = serde_json::to_value(&r.category)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            (key, r.verdict)
        })
        .collect();

    // B4: if the client has disconnected, the send will error; no further work
    // remains after the Summary event, so we simply let the function exit.
    let _ = tx
        .send(SseEvent::Summary {
            grade,
            verdicts: verdicts_map,
            duration_ms: inspection_start.elapsed().as_millis() as u64,
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns::test_support::TestDnsResolver;
    use std::time::Duration;

    fn test_config() -> crate::config::Config {
        // Build a default Config via serde from an empty TOML document. Every
        // field has a `#[serde(default = ...)]`, so this produces the same
        // struct as loading an empty file.
        let builder = ::config::Config::builder()
            .add_source(::config::File::from_str("", ::config::FileFormat::Toml));
        builder.build().unwrap().try_deserialize().unwrap()
    }

    /// E2: when every DNS lookup stalls past the 30s timeout, the Summary
    /// event must fire with Grade::Skipped and every category verdict Skip.
    #[tokio::test(start_paused = true)]
    async fn run_all_checks_timeout_yields_grade_skipped() {
        let dns = Arc::new(TestDnsResolver::new().with_delay(Duration::from_secs(40)));
        let dnsbl = Arc::new(TestDnsResolver::new().with_delay(Duration::from_secs(40)));
        let config = Arc::new(test_config());

        let (tx, mut rx) = mpsc::channel::<SseEvent>(64);

        let fetch = crate::state::OutboundFetch::new(
            5_000,
            Arc::new(crate::dns::FetchResolver(dns.clone())),
        );
        let handle = tokio::spawn(run_all_checks(
            "example.com".to_string(),
            Vec::new(),
            config,
            dns,
            dnsbl,
            fetch,
            None,
            tx,
        ));

        // Drain events until we see a Summary.
        let mut summary_grade: Option<Grade> = None;
        let mut summary_verdicts: Option<HashMap<String, Verdict>> = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                SseEvent::Summary {
                    grade, verdicts, ..
                } => {
                    summary_grade = Some(grade);
                    summary_verdicts = Some(verdicts);
                    break;
                }
                SseEvent::Category(_) => {}
            }
        }
        handle.await.unwrap();

        let grade = summary_grade.expect("Summary event must be emitted within 30s");
        let verdicts = summary_verdicts.unwrap();
        assert!(
            matches!(grade, Grade::Skipped),
            "expected Grade::Skipped, got {:?}",
            grade
        );
        assert_eq!(
            verdicts.len(),
            12,
            "expected 12 verdicts, got {}",
            verdicts.len()
        );
        for (k, v) in &verdicts {
            assert!(
                matches!(v, Verdict::Skip),
                "expected Skip for {}, got {:?}",
                k,
                v
            );
        }
    }

    // ---- email-scoring Phase 1 ------------------------------------------

    use crate::dns::TlsaRecord;
    use std::future::Future;
    use std::net::Ipv4Addr;

    /// Wraps a `TestDnsResolver` and panics on any lookup under `_domainkey`,
    /// so the DKIM task panics while every other task finishes normally.
    struct PanicOnDkim(TestDnsResolver);

    impl DnsLookup for PanicOnDkim {
        fn lookup_txt(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
            let name = name.to_string();
            async move {
                if name.contains("._domainkey.") {
                    panic!("injected DKIM task panic");
                }
                self.0.lookup_txt(&name).await
            }
        }
        fn lookup_mx(&self, name: &str) -> impl Future<Output = Vec<(u16, String)>> + Send {
            self.0.lookup_mx(name)
        }
        fn lookup_ips(&self, name: &str) -> impl Future<Output = Vec<IpAddr>> + Send {
            self.0.lookup_ips(name)
        }
        fn lookup_cname(&self, name: &str) -> impl Future<Output = Vec<String>> + Send {
            self.0.lookup_cname(name)
        }
        fn lookup_ptr(&self, ip: IpAddr) -> impl Future<Output = Vec<String>> + Send {
            self.0.lookup_ptr(ip)
        }
        fn lookup_a(&self, name: &str) -> impl Future<Output = Vec<Ipv4Addr>> + Send {
            self.0.lookup_a(name)
        }
        fn lookup_tlsa(&self, name: &str) -> impl Future<Output = Vec<TlsaRecord>> + Send {
            self.0.lookup_tlsa(name)
        }
        fn lookup_exists(&self, name: &str) -> impl Future<Output = bool> + Send {
            self.0.lookup_exists(name)
        }
        fn check_dnssec_signed(&self, name: &str) -> impl Future<Output = bool> + Send {
            self.0.check_dnssec_signed(name)
        }
    }

    /// Run the whole pipeline for `example.com` and collect every event sent.
    async fn run_events<R: DnsLookup + 'static>(dns: R) -> Vec<SseEvent> {
        let dns = Arc::new(dns);
        let fetch = crate::state::OutboundFetch::new(
            5_000,
            Arc::new(crate::dns::FetchResolver(dns.clone())),
        );
        let (tx, mut rx) = mpsc::channel::<SseEvent>(64);
        let handle = tokio::spawn(run_all_checks(
            "example.com".to_string(),
            Vec::new(),
            Arc::new(test_config()),
            dns.clone(),
            dns,
            fetch,
            None,
            tx,
        ));
        let mut events = Vec::new();
        while let Some(ev) = rx.recv().await {
            events.push(ev);
        }
        handle.await.unwrap();
        events
    }

    fn category_event(events: &[SseEvent], category: Category) -> Option<&CheckResult> {
        events.iter().find_map(|e| match e {
            SseEvent::Category(c) if c.category == category => Some(c),
            _ => None,
        })
    }

    fn summary_grade(events: &[SseEvent]) -> Grade {
        events
            .iter()
            .find_map(|e| match e {
                SseEvent::Summary { grade, .. } => Some(*grade),
                _ => None,
            })
            .expect("a Summary event")
    }

    /// C9/C2: a DKIM task that panics still yields a `dkim` category event whose
    /// only sub-check is `skipped`, sent before the Summary; every category is sent.
    #[tokio::test(start_paused = true)]
    async fn panicking_dkim_task_sends_skipped_category_before_summary() {
        let dns = TestDnsResolver::new()
            .with_mx("example.com", vec![(10, "mail.example.com")])
            .with_txt("example.com", vec!["v=spf1 -all"]);
        let events = run_events(PanicOnDkim(dns)).await;

        let summary_pos = events
            .iter()
            .position(|e| matches!(e, SseEvent::Summary { .. }))
            .expect("a Summary event");
        let dkim_pos = events
            .iter()
            .position(|e| matches!(e, SseEvent::Category(c) if c.category == Category::Dkim))
            .expect("a dkim Category event must be sent even when its task panics");
        assert!(
            dkim_pos < summary_pos,
            "dkim event must precede the Summary"
        );

        let dkim = category_event(&events, Category::Dkim).unwrap();
        assert_eq!(dkim.sub_checks.len(), 1);
        assert_eq!(dkim.sub_checks[0].name, SKIPPED);
        assert_eq!(dkim.sub_checks[0].verdict, Verdict::Skip);

        for cat in Category::ALL.iter() {
            let pos = events
                .iter()
                .position(|e| matches!(e, SseEvent::Category(c) if c.category == *cat))
                .unwrap_or_else(|| panic!("no Category event for {cat:?}"));
            assert!(pos < summary_pos, "{cat:?} must precede the Summary");
        }
    }

    /// C10/C11/C12: `sends_no_mail` is an Info cross-validation sub-check that
    /// does not count as an issue.
    #[tokio::test(start_paused = true)]
    async fn sends_no_mail_rows() {
        // (label, MX records, SPF, expect sends_no_mail)
        let rows: [(&str, Vec<(u16, &str)>, &str, bool); 3] = [
            ("null MX", vec![(0, ".")], "v=spf1 -all", true),
            (
                "-all only, normal MX",
                vec![(10, "mail.example.com")],
                "v=spf1 -all",
                true,
            ),
            (
                "include before -all",
                vec![(10, "mail.example.com")],
                "v=spf1 include:_spf.example.com -all",
                false,
            ),
        ];
        for (label, mx, spf, expect) in rows {
            let dns = TestDnsResolver::new()
                .with_mx("example.com", mx)
                .with_txt("example.com", vec![spf]);
            let events = run_events(dns).await;
            if label == "null MX" {
                let mx = category_event(&events, Category::Mx).expect("mx event");
                assert!(mx.sub_checks.iter().any(|s| s.name == mx::NULL_MX));
            }
            let cross = category_event(&events, Category::CrossValidation)
                .unwrap_or_else(|| panic!("{label}: no cross_validation event"));
            let sc = cross
                .sub_checks
                .iter()
                .find(|s| s.name == cross_validation::SENDS_NO_MAIL);
            assert_eq!(
                sc.is_some(),
                expect,
                "{label}: sub-checks {:?}",
                cross.sub_checks
            );
            if let Some(sc) = sc {
                assert_eq!(sc.verdict, Verdict::Info, "{label}");
                // It alone is not an issue: detail and verdict are what they are without it.
                assert_eq!(cross.sub_checks.len(), 1, "{label}: {:?}", cross.sub_checks);
                assert_eq!(
                    cross.detail, "all cross-validation checks passed",
                    "{label}"
                );
                assert_eq!(cross.verdict, Verdict::Pass, "{label}");
            }
        }
    }

    /// C18: a revoked DKIM key on a parked domain does not lower the grade.
    #[tokio::test(start_paused = true)]
    async fn parked_domain_revoked_key_keeps_grade() {
        let parked = |with_revoked_key: bool| {
            let mut dns = TestDnsResolver::new()
                .with_mx("example.com", vec![(0, ".")])
                .with_txt("example.com", vec!["v=spf1 -all"])
                .with_txt(
                    "_dmarc.example.com",
                    vec!["v=DMARC1; p=reject; rua=mailto:d@example.com"],
                );
            if with_revoked_key {
                dns = dns.with_txt("default._domainkey.example.com", vec!["v=DKIM1; k=rsa; p="]);
            }
            dns
        };
        let without = summary_grade(&run_events(parked(false)).await);
        let with_events = run_events(parked(true)).await;
        let with = summary_grade(&with_events);
        let dkim = category_event(&with_events, Category::Dkim).expect("a dkim category event");
        assert_eq!(
            dkim.verdict,
            Verdict::Info,
            "sends_no_mail must reach DKIM: revoked-only on a parked domain is Info"
        );
        assert_eq!(
            with, without,
            "a revoked key must not move the parked domain's grade"
        );
    }

    fn canonical(v: serde_json::Value) -> serde_json::Value {
        use serde_json::Value;
        match v {
            Value::Object(m) => {
                let mut keys: Vec<String> = m.keys().cloned().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
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

    /// C20: the real `run_all_checks` timeout path, encoded as on the wire
    /// (`From<SseEvent> for Event` through axum's `Sse`), against
    /// `tests/fixtures/contracts/beacon-timeout.sse`.
    /// Regenerate: `UPDATE_GOLDEN=1 cargo test -p beacon --lib timeout_golden`.
    #[tokio::test(start_paused = true)]
    async fn timeout_golden() {
        use axum::response::IntoResponse;
        use axum::response::sse::{Event, Sse};
        use http_body_util::BodyExt;

        let dns = TestDnsResolver::new().with_delay(Duration::from_secs(40));
        let events = run_events(dns).await;
        assert!(
            matches!(summary_grade(&events), Grade::Skipped),
            "timeout summary grade must be skipped"
        );

        let stream = futures::stream::iter(events.into_iter().map(|e| {
            let ev: Event = e.into();
            Ok::<_, std::convert::Infallible>(ev)
        }));
        let bytes = Sse::new(stream)
            .into_response()
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let wire: String = std::str::from_utf8(&bytes)
            .expect("utf-8")
            .split_inclusive('\n')
            .map(|line| match line.strip_prefix("data: ") {
                Some(rest) => {
                    let v: serde_json::Value =
                        serde_json::from_str(rest.trim_end()).expect("data is JSON");
                    format!("data: {}\n", serde_json::to_string(&canonical(v)).unwrap())
                }
                None => line.to_string(),
            })
            .collect();

        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/contracts/beacon-timeout.sse");
        if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &wire).unwrap();
            return;
        }
        let golden = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "golden {} is missing; run `UPDATE_GOLDEN=1 cargo test -p beacon --lib timeout_golden`",
                path.display()
            )
        });
        assert_eq!(
            golden,
            wire,
            "timeout wire output differs from {}",
            path.display()
        );
    }
}
