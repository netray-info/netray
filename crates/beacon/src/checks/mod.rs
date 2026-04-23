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
    http_client: reqwest::Client,
    http_client_follow: reqwest::Client,
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
            http_client,
            http_client_follow,
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

fn skip_result(category: Category) -> CheckResult {
    CheckResult::new(
        category,
        vec![crate::quality::SubCheck {
            name: "skipped".to_string(),
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
    http_client: reqwest::Client,
    http_client_follow: reqwest::Client,
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
                let (result, flat, dash_all) = spf::check_spf(&domain, dns.as_ref()).await;
                let elapsed = start.elapsed().as_secs_f64();
                metrics::histogram!("beacon_check_duration_seconds", "category" => "spf")
                    .record(elapsed);
                Phase0Result::Spf {
                    result,
                    flat,
                    has_dash_all: dash_all,
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
        let http_follow = http_client_follow.clone();
        let handle = phase0.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, present) = bimi::check_bimi(&domain, dns.as_ref(), &http_follow).await;
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
                    } => {
                        spf_result = Some(result);
                        spf_flat = flat;
                        spf_has_dash_all = has_dash_all;
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
                let category = phase0_categories
                    .get(&e.id())
                    .copied()
                    .unwrap_or("unknown");
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
                let (result, found) =
                    dkim::check_dkim(&domain, &mx_hosts_clone, &selectors, max_sels, dns.as_ref()).await;
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
        let http = http_client.clone();
        let handle = phase1.spawn(
            async move {
                let start = std::time::Instant::now();
                let (result, info) = mta_sts::check_mta_sts(&domain, dns.as_ref(), &http).await;
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
                let result = dnsbl::check_dnsbl(&mx_ips_clone, &domain, &dnsbl_config, dns.as_ref()).await;
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
                let category = phase1_categories
                    .get(&e.id())
                    .copied()
                    .unwrap_or("unknown");
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

    // Phase 2: sequential cross-validation and grade
    let mx_r = mx_result.unwrap_or_else(|| skip_result(Category::Mx));
    let spf_r = spf_result.unwrap_or_else(|| skip_result(Category::Spf));
    let dkim_r = dkim_result.unwrap_or_else(|| skip_result(Category::Dkim));
    let dmarc_r = dmarc_result.unwrap_or_else(|| skip_result(Category::Dmarc));
    let mta_sts_r = mta_sts_result.unwrap_or_else(|| skip_result(Category::MtaSts));
    let tls_rpt_r = tls_rpt_result.unwrap_or_else(|| skip_result(Category::TlsRpt));
    let dane_r = dane_result.unwrap_or_else(|| skip_result(Category::Dane));
    let dnssec_r = dnssec_result.unwrap_or_else(|| skip_result(Category::Dnssec));
    let bimi_r = bimi_result.unwrap_or_else(|| skip_result(Category::Bimi));
    let fcrdns_r = fcrdns_result.unwrap_or_else(|| skip_result(Category::Fcrdns));
    let dnsbl_r = dnsbl_result.unwrap_or_else(|| skip_result(Category::Dnsbl));

    let fcrdns_all_pass = fcrdns_r
        .sub_checks
        .iter()
        .all(|s| s.verdict == Verdict::Pass);

    let all_results = AllResults {
        mx: mx_r,
        mx_hosts,
        mx_ips,
        null_mx,
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

        let http = reqwest::Client::new();
        let handle = tokio::spawn(run_all_checks(
            "example.com".to_string(),
            Vec::new(),
            config,
            dns,
            dnsbl,
            http.clone(),
            http,
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
        assert!(matches!(grade, Grade::Skipped), "expected Grade::Skipped, got {:?}", grade);
        assert_eq!(verdicts.len(), 12, "expected 12 verdicts, got {}", verdicts.len());
        for (k, v) in &verdicts {
            assert!(
                matches!(v, Verdict::Skip),
                "expected Skip for {}, got {:?}",
                k,
                v
            );
        }
    }
}
