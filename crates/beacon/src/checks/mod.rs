pub mod mx;
pub mod spf;
pub mod dkim;
pub mod dmarc;
pub mod mta_sts;
pub mod tls_rpt;
pub mod dane;
pub mod dnssec;
pub mod bimi;
pub mod fcrdns;
pub mod dnsbl;
pub mod cross_validation;

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::quality::{AllResults, Grade, SseEvent, Verdict, compute_grade};
use netray_common::enrichment::EnrichmentClient;

/// Run all email security checks for a domain.
///
/// Uses `spawn_blocking` + a current-thread runtime + `LocalSet` so that
/// mhost's !Send resolver futures (internal `Rc<ThreadRng>`) never cross
/// a Send boundary.
pub async fn run_all_checks(
    domain: String,
    selectors: Vec<String>,
    config: Arc<Config>,
    http_client: reqwest::Client,
    http_client_follow: reqwest::Client,
    enrichment_client: Option<Arc<EnrichmentClient>>,
) -> Vec<SseEvent> {
    let dns_config = config.dns.clone();

    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build current-thread runtime");

        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let resolver = match crate::dns::DnsResolver::new(&dns_config).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error = %e, "failed to create per-request DNS resolver");
                    return vec![SseEvent::Summary {
                        grade: Grade::F,
                        verdicts: HashMap::new(),
                    }];
                }
            };

            run_checks_inner(
                &domain,
                &selectors,
                &resolver,
                &config,
                &http_client,
                &http_client_follow,
                enrichment_client.as_deref(),
            )
            .await
        })
    })
    .await;

    result.unwrap_or_else(|e| {
        tracing::error!(error = %e, "check task panicked");
        vec![SseEvent::Summary {
            grade: Grade::F,
            verdicts: HashMap::new(),
        }]
    })
}

async fn run_checks_inner(
    domain: &str,
    selectors: &[String],
    resolver: &crate::dns::DnsResolver,
    config: &Config,
    http_client: &reqwest::Client,
    http_client_follow: &reqwest::Client,
    enrichment_client: Option<&EnrichmentClient>,
) -> Vec<SseEvent> {
    let dns_start = std::time::Instant::now();

    // Phase 1: MX check (needed by many subsequent checks)
    let (mx_result, mx_ips, mx_hosts, null_mx) =
        mx::check_mx(domain, resolver, enrichment_client).await;

    // Phase 2: Run all remaining checks concurrently
    let (
        (spf_result, spf_flat, spf_has_dash_all),
        dkim_result_data,
        dmarc_result_data,
        (mta_sts_result, mta_sts_info),
        (tls_rpt_result, tls_rpt_present),
        (dane_result, dane_has_tlsa),
        (dnssec_result, dnssec_ad),
        bimi_result_data,
        fcrdns_result,
        dnsbl_result,
    ) = tokio::join!(
        spf::check_spf(domain, resolver),
        dkim::check_dkim(
            domain,
            &mx_hosts,
            selectors,
            config.dkim.max_user_selectors,
            resolver,
        ),
        dmarc::check_dmarc(domain, resolver),
        mta_sts::check_mta_sts(domain, &mx_hosts, resolver, http_client,),
        tls_rpt::check_tls_rpt(domain, resolver),
        dane::check_dane(&mx_hosts, resolver),
        dnssec::check_dnssec(domain, resolver),
        bimi::check_bimi(domain, resolver, http_client_follow,),
        fcrdns::check_fcrdns(&mx_ips, resolver),
        dnsbl::check_dnsbl(&mx_ips, domain, &config.dnsbl, resolver,),
    );

    let (dkim_result, dkim_found) = dkim_result_data;
    let (dmarc_result, dmarc_policy, dmarc_sp, dmarc_rua_external_auth_ok) = dmarc_result_data;
    let (bimi_result, bimi_present) = bimi_result_data;
    let fcrdns_all_pass = fcrdns_result
        .sub_checks
        .iter()
        .all(|s| s.verdict == Verdict::Pass);
    let mta_sts_present = mta_sts_info.is_some();

    // Collect all category results
    let all_results = AllResults {
        mx: mx_result.clone(),
        mx_hosts,
        mx_ips,
        null_mx,
        spf: spf_result.clone(),
        spf_flat,
        spf_has_dash_all,
        dkim: dkim_result.clone(),
        dkim_found,
        dmarc: dmarc_result.clone(),
        dmarc_policy,
        dmarc_sp,
        dmarc_rua_external_auth_ok,
        mta_sts: mta_sts_result.clone(),
        mta_sts_present,
        mta_sts_info,
        tls_rpt: tls_rpt_result.clone(),
        tls_rpt_present,
        dane: dane_result.clone(),
        dane_has_tlsa,
        dnssec: dnssec_result.clone(),
        dnssec_ad,
        bimi: bimi_result.clone(),
        bimi_present,
        fcrdns: fcrdns_result.clone(),
        fcrdns_all_pass,
        dnsbl: dnsbl_result.clone(),
    };

    let dns_elapsed = dns_start.elapsed().as_secs_f64();
    metrics::histogram!("beacon_dns_query_duration_seconds", "record_type" => "all")
        .record(dns_elapsed);

    // Cross-validation
    let cross_result = cross_validation::cross_validate(&all_results);

    // Build category events in specified order
    let category_results = vec![
        mx_result,
        spf_result,
        dkim_result,
        dmarc_result,
        mta_sts_result,
        tls_rpt_result,
        dane_result,
        dnssec_result,
        bimi_result,
        fcrdns_result,
        dnsbl_result,
        cross_result.clone(),
    ];

    // Build SSE events in order
    let mut events: Vec<SseEvent> = category_results
        .iter()
        .map(|r| SseEvent::Category(r.clone()))
        .collect();

    // Summary
    let verdicts_list: Vec<Verdict> = category_results.iter().map(|r| r.verdict).collect();
    let grade = compute_grade(&verdicts_list);

    let verdicts_map: HashMap<String, Verdict> = category_results
        .iter()
        .map(|r| {
            let key = serde_json::to_value(&r.category)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            (key, r.verdict)
        })
        .collect();

    events.push(SseEvent::Summary {
        grade,
        verdicts: verdicts_map,
    });

    events
}
