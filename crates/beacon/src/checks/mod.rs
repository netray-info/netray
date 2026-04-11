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

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::quality::{AllResults, Category, CheckResult, Grade, SseEvent, Verdict, compute_grade};
use netray_common::enrichment::EnrichmentClient;

/// Run all email security checks for a domain, streaming results via mpsc channel.
/// Wraps `run_inspection_inner` with a 30-second timeout.
#[allow(clippy::too_many_arguments)]
pub async fn run_all_checks(
    domain: String,
    selectors: Vec<String>,
    config: Arc<Config>,
    dns: Arc<DnsResolver>,
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
            http_client,
            http_client_follow,
            enrichment_client,
            tx.clone(),
        ),
    )
    .await;

    if timeout_result.is_err() {
        tracing::warn!(domain = %domain, "inspection timed out after 30s");
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
                grade: Grade::F,
                verdicts,
                duration_ms: 30_000,
            })
            .await;
    }
}

/// Phase tags for phase-0 JoinSet results.
enum Phase0Tag {
    Mx,
    Spf,
    Dmarc,
    TlsRpt,
    Dnssec,
    Bimi,
}

struct Phase0Output {
    result: CheckResult,
    mx_hosts: Vec<String>,
    mx_ips: Vec<IpAddr>,
    null_mx: bool,
    spf_flat: Option<crate::quality::SpfFlat>,
    spf_has_dash_all: bool,
    dmarc_policy: Option<String>,
    dmarc_sp: Option<String>,
    dmarc_rua_external_auth_ok: bool,
    tls_rpt_present: bool,
    dnssec_ad: bool,
    bimi_present: bool,
}

impl Phase0Output {
    fn from_mx(
        result: CheckResult,
        mx_hosts: Vec<String>,
        mx_ips: Vec<IpAddr>,
        null_mx: bool,
    ) -> Self {
        Self {
            result,
            mx_hosts,
            mx_ips,
            null_mx,
            spf_flat: None,
            spf_has_dash_all: false,
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            tls_rpt_present: false,
            dnssec_ad: false,
            bimi_present: false,
        }
    }

    fn from_spf(
        result: CheckResult,
        flat: Option<crate::quality::SpfFlat>,
        dash_all: bool,
    ) -> Self {
        Self {
            result,
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf_flat: flat,
            spf_has_dash_all: dash_all,
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            tls_rpt_present: false,
            dnssec_ad: false,
            bimi_present: false,
        }
    }

    fn from_dmarc(
        result: CheckResult,
        policy: Option<String>,
        sp: Option<String>,
        rua_ok: bool,
    ) -> Self {
        Self {
            result,
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf_flat: None,
            spf_has_dash_all: false,
            dmarc_policy: policy,
            dmarc_sp: sp,
            dmarc_rua_external_auth_ok: rua_ok,
            tls_rpt_present: false,
            dnssec_ad: false,
            bimi_present: false,
        }
    }

    fn from_tls_rpt(result: CheckResult, present: bool) -> Self {
        Self {
            result,
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf_flat: None,
            spf_has_dash_all: false,
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            tls_rpt_present: present,
            dnssec_ad: false,
            bimi_present: false,
        }
    }

    fn from_dnssec(result: CheckResult, ad: bool) -> Self {
        Self {
            result,
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf_flat: None,
            spf_has_dash_all: false,
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            tls_rpt_present: false,
            dnssec_ad: ad,
            bimi_present: false,
        }
    }

    fn from_bimi(result: CheckResult, present: bool) -> Self {
        Self {
            result,
            mx_hosts: Vec::new(),
            mx_ips: Vec::new(),
            null_mx: false,
            spf_flat: None,
            spf_has_dash_all: false,
            dmarc_policy: None,
            dmarc_sp: None,
            dmarc_rua_external_auth_ok: true,
            tls_rpt_present: false,
            dnssec_ad: false,
            bimi_present: present,
        }
    }
}

/// Phase tags for phase-1 JoinSet results.
enum Phase1Tag {
    Dkim,
    MtaSts,
    Dane,
    Fcrdns,
    Dnsbl,
}

struct Phase1Output {
    result: CheckResult,
    dkim_found: bool,
    mta_sts_info: Option<crate::quality::MtaStsInfo>,
    mta_sts_present: bool,
    dane_has_tlsa: bool,
}

impl Phase1Output {
    fn from_dkim(result: CheckResult, found: bool) -> Self {
        Self {
            result,
            dkim_found: found,
            mta_sts_info: None,
            mta_sts_present: false,
            dane_has_tlsa: false,
        }
    }

    fn from_mta_sts(
        result: CheckResult,
        present: bool,
        info: Option<crate::quality::MtaStsInfo>,
    ) -> Self {
        Self {
            result,
            dkim_found: false,
            mta_sts_info: info,
            mta_sts_present: present,
            dane_has_tlsa: false,
        }
    }

    fn from_dane(result: CheckResult, has_tlsa: bool) -> Self {
        Self {
            result,
            dkim_found: false,
            mta_sts_info: None,
            mta_sts_present: false,
            dane_has_tlsa: has_tlsa,
        }
    }

    fn from_other(result: CheckResult) -> Self {
        Self {
            result,
            dkim_found: false,
            mta_sts_info: None,
            mta_sts_present: false,
            dane_has_tlsa: false,
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
async fn run_inspection_inner(
    domain: String,
    selectors: Vec<String>,
    config: Arc<Config>,
    dns: Arc<DnsResolver>,
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
    let mut dnssec_ad = false;
    let mut bimi_result: Option<CheckResult> = None;
    let mut bimi_present = false;

    // Phase 0: domain-only checks in parallel
    let mut phase0: JoinSet<(Phase0Tag, Phase0Output)> = JoinSet::new();

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let enrichment_client = enrichment_client.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, ips, hosts, is_null_mx) =
                mx::check_mx(&domain, &dns, enrichment_client.as_deref()).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "mx")
                .record(elapsed);
            (
                Phase0Tag::Mx,
                Phase0Output::from_mx(result, hosts, ips, is_null_mx),
            )
        });
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, flat, dash_all) = spf::check_spf(&domain, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "spf")
                .record(elapsed);
            (
                Phase0Tag::Spf,
                Phase0Output::from_spf(result, flat, dash_all),
            )
        });
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, policy, sp, rua_ok) = dmarc::check_dmarc(&domain, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "dmarc")
                .record(elapsed);
            (
                Phase0Tag::Dmarc,
                Phase0Output::from_dmarc(result, policy, sp, rua_ok),
            )
        });
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, present) = tls_rpt::check_tls_rpt(&domain, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "tls_rpt")
                .record(elapsed);
            (
                Phase0Tag::TlsRpt,
                Phase0Output::from_tls_rpt(result, present),
            )
        });
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, ad) = dnssec::check_dnssec(&domain, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "dnssec")
                .record(elapsed);
            (Phase0Tag::Dnssec, Phase0Output::from_dnssec(result, ad))
        });
    }

    {
        let domain = domain.clone();
        let dns = dns.clone();
        let http_follow = http_client_follow.clone();
        phase0.spawn(async move {
            let start = std::time::Instant::now();
            let (result, present) = bimi::check_bimi(&domain, &dns, &http_follow).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "bimi")
                .record(elapsed);
            (Phase0Tag::Bimi, Phase0Output::from_bimi(result, present))
        });
    }

    // Drain phase 0, sending each result immediately
    while let Some(join_result) = phase0.join_next().await {
        match join_result {
            Ok((tag, output)) => {
                let _ = tx.send(SseEvent::Category(output.result.clone())).await;
                match tag {
                    Phase0Tag::Mx => {
                        mx_result = Some(output.result);
                        mx_hosts = output.mx_hosts;
                        mx_ips = output.mx_ips;
                        null_mx = output.null_mx;
                    }
                    Phase0Tag::Spf => {
                        spf_result = Some(output.result);
                        spf_flat = output.spf_flat;
                        spf_has_dash_all = output.spf_has_dash_all;
                    }
                    Phase0Tag::Dmarc => {
                        dmarc_result = Some(output.result);
                        dmarc_policy = output.dmarc_policy;
                        dmarc_sp = output.dmarc_sp;
                        dmarc_rua_external_auth_ok = output.dmarc_rua_external_auth_ok;
                    }
                    Phase0Tag::TlsRpt => {
                        tls_rpt_result = Some(output.result);
                        tls_rpt_present = output.tls_rpt_present;
                    }
                    Phase0Tag::Dnssec => {
                        dnssec_result = Some(output.result);
                        dnssec_ad = output.dnssec_ad;
                    }
                    Phase0Tag::Bimi => {
                        bimi_result = Some(output.result);
                        bimi_present = output.bimi_present;
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "phase-0 check task panicked");
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
    let mut phase1: JoinSet<(Phase1Tag, Phase1Output)> = JoinSet::new();

    {
        let domain = domain.clone();
        let mx_hosts_clone = mx_hosts.clone();
        let selectors = selectors.clone();
        let dns = dns.clone();
        let max_sels = config.dkim.max_user_selectors;
        phase1.spawn(async move {
            let start = std::time::Instant::now();
            let (result, found) =
                dkim::check_dkim(&domain, &mx_hosts_clone, &selectors, max_sels, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "dkim")
                .record(elapsed);
            (Phase1Tag::Dkim, Phase1Output::from_dkim(result, found))
        });
    }

    {
        let domain = domain.clone();
        let mx_hosts_clone = mx_hosts.clone();
        let dns = dns.clone();
        let http = http_client.clone();
        phase1.spawn(async move {
            let start = std::time::Instant::now();
            let (result, info) =
                mta_sts::check_mta_sts(&domain, &mx_hosts_clone, &dns, &http).await;
            let present = info.is_some();
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "mta_sts")
                .record(elapsed);
            (
                Phase1Tag::MtaSts,
                Phase1Output::from_mta_sts(result, present, info),
            )
        });
    }

    {
        let mx_hosts_clone = mx_hosts.clone();
        let dns = dns.clone();
        phase1.spawn(async move {
            let start = std::time::Instant::now();
            let (result, has_tlsa) = dane::check_dane(&mx_hosts_clone, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "dane")
                .record(elapsed);
            (Phase1Tag::Dane, Phase1Output::from_dane(result, has_tlsa))
        });
    }

    {
        let mx_ips_clone = mx_ips.clone();
        let dns = dns.clone();
        phase1.spawn(async move {
            let start = std::time::Instant::now();
            let result = fcrdns::check_fcrdns(&mx_ips_clone, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "fcrdns")
                .record(elapsed);
            (Phase1Tag::Fcrdns, Phase1Output::from_other(result))
        });
    }

    {
        let mx_ips_clone = mx_ips.clone();
        let domain = domain.clone();
        let dnsbl_config = config.dnsbl.clone();
        let dns = dns.clone();
        phase1.spawn(async move {
            let start = std::time::Instant::now();
            let result = dnsbl::check_dnsbl(&mx_ips_clone, &domain, &dnsbl_config, &dns).await;
            let elapsed = start.elapsed().as_secs_f64();
            metrics::histogram!("beacon_check_duration_seconds", "category" => "dnsbl")
                .record(elapsed);
            (Phase1Tag::Dnsbl, Phase1Output::from_other(result))
        });
    }

    // Drain phase 1, sending each result immediately
    while let Some(join_result) = phase1.join_next().await {
        match join_result {
            Ok((tag, output)) => {
                let _ = tx.send(SseEvent::Category(output.result.clone())).await;
                match tag {
                    Phase1Tag::Dkim => {
                        dkim_result = Some(output.result);
                        dkim_found = output.dkim_found;
                    }
                    Phase1Tag::MtaSts => {
                        mta_sts_result = Some(output.result);
                        mta_sts_present = output.mta_sts_present;
                        mta_sts_info = output.mta_sts_info;
                    }
                    Phase1Tag::Dane => {
                        dane_result = Some(output.result);
                        dane_has_tlsa = output.dane_has_tlsa;
                    }
                    Phase1Tag::Fcrdns => {
                        fcrdns_result = Some(output.result);
                    }
                    Phase1Tag::Dnsbl => {
                        dnsbl_result = Some(output.result);
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "phase-1 check task panicked");
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
        dnssec_ad,
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

    let _ = tx.send(SseEvent::Category(cross_result.clone())).await;

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

    let _ = tx
        .send(SseEvent::Summary {
            grade,
            verdicts: verdicts_map,
            duration_ms: inspection_start.elapsed().as_millis() as u64,
        })
        .await;
}
