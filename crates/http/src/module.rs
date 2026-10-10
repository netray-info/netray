use std::convert::Infallible;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use governor::{Quota, RateLimiter};
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol};
use serde_json::json;

use crate::config::{InspectConfig, ModuleConfig};
use crate::error::AppError;
use crate::inspect::assembler::InspectResponse;
use crate::inspect::request::Outbound;
use crate::quality::types::{CheckStatus, QualityCheck};

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    [
        "https_redirect",
        "hsts",
        "security_headers",
        "cors",
        "cookie_secure",
        "hygiene",
    ]
    .into_iter()
    .map(|name| CheckId::parse(&format!("http.{name}")).expect("static check id"))
    .collect()
});

pub(crate) fn http_checks() -> &'static [CheckId] {
    CHECKS.as_slice()
}

/// The HTTP inspection as an engine module. The target address comes from `Facts` (`a`, then
/// `aaaa`) through the target policy; while the engine run does not supply Facts, it resolves
/// the name itself.
pub struct HttpModule {
    inspect: InspectConfig,
    outbound: Outbound,
    enrichment: Option<Arc<EnrichmentClient>>,
    per_target: KeyedLimiter<String>,
}

impl HttpModule {
    pub fn new(config: ModuleConfig) -> Result<Self, Infallible> {
        let per_target = RateLimiter::keyed(
            Quota::per_minute(config.limits.per_target_per_minute)
                .allow_burst(config.limits.per_target_burst),
        );
        Ok(Self {
            per_target,
            enrichment: crate::state::enrichment_client(&config.enrichment),
            inspect: config.inspect,
            outbound: Outbound::system(),
        })
    }

    async fn measure(&self, ctx: &RunContext, facts: &Facts) -> Result<InspectResponse, AppError> {
        let start = Instant::now();
        let url = crate::input::parse_url(&format!("https://{}", ctx.domain.as_str()))?;

        let hostname = url.host_str().unwrap_or_default().to_lowercase();
        check_keyed_cost(
            &self.per_target,
            &hostname,
            NonZeroU32::MIN,
            "per_target",
            "spectra",
        )
        .map_err(|r| AppError::RateLimited {
            retry_after_secs: r.retry_after_secs,
            scope: r.scope,
        })?;

        let addrs: Vec<IpAddr> = facts
            .a
            .iter()
            .map(|ip| IpAddr::V4(*ip))
            .chain(facts.aaaa.iter().map(|ip| IpAddr::V6(*ip)))
            .collect();
        let resolved_addr = if addrs.is_empty() {
            crate::input::validate_target(&url).await?
        } else {
            crate::input::pick_target(&addrs, url.port_or_known_default().unwrap_or(443))?
        };

        crate::inspect::inspect_and_assemble(
            &url,
            resolved_addr,
            &self.inspect,
            &self.outbound,
            self.enrichment.as_deref(),
            None,
            start,
        )
        .await
    }
}

impl Module for HttpModule {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }

    fn checks(&self) -> &'static [CheckId] {
        http_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match self.measure(ctx, facts).await {
                Ok(resp) => translate(&resp),
                Err(e) => SectionOutcome::Incomplete {
                    reason: e.to_string(),
                },
            }
        })
    }
}

/// Maps the inspection onto the six HTTP checks and the V1 headline and extras.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    let quality = &resp.quality.checks;

    // https_redirect: synthesised from the http_upgrade field.
    let https_redirect_status = match &resp.http_upgrade {
        None => CheckStatus::Skip,
        Some(u) => {
            if u.redirects_to_https {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            }
        }
    };

    let hsts_status = lookup_check(quality, "hsts");
    let cors_status = lookup_check(quality, "cors");
    let cookie_secure_status = lookup_check(quality, "cookie_secure");

    const SECURITY_HEADERS: [&str; 5] = [
        "csp",
        "x_frame_options",
        "x_content_type_options",
        "referrer_policy",
        "permissions_policy",
    ];
    const HYGIENE: [&str; 4] = [
        "deprecated_headers",
        "info_leakage",
        "caching",
        "redirect_limit",
    ];
    let security_headers_status = aggregate_worst(quality, &SECURITY_HEADERS);
    let hygiene_status = aggregate_worst(quality, &HYGIENE);

    let results = [
        (
            https_redirect_status.clone(),
            match https_redirect_status {
                CheckStatus::Fail => vec!["HTTP port 80 does not redirect to HTTPS".to_string()],
                _ => vec![],
            },
        ),
        (
            hsts_status.clone(),
            check_messages(quality, "hsts", &hsts_status),
        ),
        (
            security_headers_status,
            aggregate_messages(quality, &SECURITY_HEADERS),
        ),
        (
            cors_status.clone(),
            check_messages(quality, "cors", &cors_status),
        ),
        (
            cookie_secure_status.clone(),
            check_messages(quality, "cookie_secure", &cookie_secure_status),
        ),
        (hygiene_status, aggregate_messages(quality, &HYGIENE)),
    ];
    let checks = http_checks()
        .iter()
        .zip(results)
        .map(|(id, (status, findings))| CheckResult {
            id: id.clone(),
            status: status.into(),
            findings,
            evidence: vec![],
        })
        .collect();

    // Headline uses the individual csp check, not the aggregated security_headers.
    let csp_status = lookup_check(quality, "csp");
    let headline = format!(
        "HTTPS {}  HSTS {}  CSP {}  CORS {}",
        verdict_symbol(&https_redirect_status),
        verdict_symbol(&hsts_status),
        verdict_symbol(&csp_status),
        verdict_symbol(&cors_status),
    );

    let presentation = json!({
        "headline": headline,
        "status_code": resp.status,
        "http_version": resp.http_version,
        "response_duration_ms": resp.duration_ms,
        "server_ip": resp.enrichment.ip,
        "server_org": resp.enrichment.org,
        "server_network_type": resp.enrichment.ip_type,
    });

    SectionOutcome::Measured {
        checks,
        presentation,
    }
}

fn aggregate_worst(checks: &[QualityCheck], names: &[&str]) -> CheckStatus {
    names
        .iter()
        .map(|name| lookup_check(checks, name))
        .max()
        .unwrap_or(CheckStatus::Skip)
}

fn lookup_check(checks: &[QualityCheck], name: &str) -> CheckStatus {
    checks
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.status.clone())
        .unwrap_or(CheckStatus::Skip)
}

/// Return messages for a check — only for non-pass, non-skip verdicts.
fn check_messages(checks: &[QualityCheck], name: &str, status: &CheckStatus) -> Vec<String> {
    match status {
        CheckStatus::Pass | CheckStatus::Skip => vec![],
        _ => checks
            .iter()
            .find(|c| c.name == name)
            .and_then(|c| c.message.clone())
            .map(|m| vec![m])
            .unwrap_or_default(),
    }
}

/// Collect messages from constituent checks for an aggregated check, prefixed with their label.
fn aggregate_messages(checks: &[QualityCheck], names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter_map(|name| {
            checks
                .iter()
                .find(|c| c.name == *name)
                .and_then(|c| match c.status {
                    CheckStatus::Pass | CheckStatus::Skip => None,
                    _ => c.message.as_ref().map(|m| format!("{}: {m}", c.label)),
                })
        })
        .collect()
}

fn verdict_symbol(s: &CheckStatus) -> &'static str {
    match s {
        CheckStatus::Pass => "✓",
        CheckStatus::Fail => "✗",
        CheckStatus::Warn => "~",
        CheckStatus::Skip => "-",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::assembler::HttpUpgrade;
    use netray_model::Status;

    const SPECTRA_JSON: &str =
        include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

    fn response(
        redirects_to_https: Option<bool>,
        checks: &[(&str, CheckStatus)],
    ) -> InspectResponse {
        let mut resp: InspectResponse = serde_json::from_str(SPECTRA_JSON).unwrap();
        resp.http_upgrade = redirects_to_https.map(|redirects_to_https| HttpUpgrade {
            redirects_to_https,
            status_code: None,
            same_host: true,
            message: String::new(),
            redirects: vec![],
        });
        resp.quality.checks = checks
            .iter()
            .map(|(name, status)| QualityCheck {
                name: name.to_string(),
                label: name.to_string(),
                status: status.clone(),
                message: None,
                explanation: None,
            })
            .collect();
        resp
    }

    fn translated(resp: &InspectResponse) -> (Vec<CheckResult>, serde_json::Value) {
        match translate(resp) {
            SectionOutcome::Measured {
                checks,
                presentation,
            } => (checks, presentation),
            other => panic!("expected Measured, got {other:?}"),
        }
    }

    fn status_of(resp: &InspectResponse, id: &str) -> Status {
        let (checks, _) = translated(resp);
        checks
            .iter()
            .find(|c| c.id.to_string() == id)
            .unwrap_or_else(|| panic!("no check {id}"))
            .status
    }

    #[test]
    fn six_checks_always_present() {
        let resp = response(
            Some(true),
            &[
                ("hsts", CheckStatus::Pass),
                ("csp", CheckStatus::Pass),
                ("x_frame_options", CheckStatus::Pass),
                ("x_content_type_options", CheckStatus::Pass),
                ("referrer_policy", CheckStatus::Pass),
                ("permissions_policy", CheckStatus::Pass),
                ("cors", CheckStatus::Pass),
                ("cookie_secure", CheckStatus::Pass),
                ("deprecated_headers", CheckStatus::Pass),
                ("info_leakage", CheckStatus::Pass),
                ("caching", CheckStatus::Pass),
            ],
        );
        let (checks, _) = translated(&resp);
        let ids: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
        assert_eq!(
            ids,
            [
                "http.https_redirect",
                "http.hsts",
                "http.security_headers",
                "http.cors",
                "http.cookie_secure",
                "http.hygiene",
            ]
        );
    }

    #[test]
    fn https_redirect_null_is_skip() {
        let resp = response(None, &[]);
        assert_eq!(
            status_of(&resp, "http.https_redirect"),
            Status::NotApplicable
        );
    }

    #[test]
    fn https_redirect_false_is_fail() {
        let resp = response(Some(false), &[]);
        assert_eq!(status_of(&resp, "http.https_redirect"), Status::Fail);
        let (checks, _) = translated(&resp);
        assert_eq!(
            checks[0].findings,
            ["HTTP port 80 does not redirect to HTTPS"]
        );
    }

    #[test]
    fn https_redirect_true_is_pass() {
        let resp = response(Some(true), &[]);
        assert_eq!(status_of(&resp, "http.https_redirect"), Status::Pass);
    }

    #[test]
    fn security_headers_worst_verdict_is_fail() {
        let resp = response(
            None,
            &[
                ("csp", CheckStatus::Pass),
                ("x_frame_options", CheckStatus::Warn),
                ("x_content_type_options", CheckStatus::Pass),
                ("referrer_policy", CheckStatus::Fail),
                ("permissions_policy", CheckStatus::Skip),
            ],
        );
        assert_eq!(status_of(&resp, "http.security_headers"), Status::Fail);
    }

    #[test]
    fn hygiene_aggregation_absent_checks_is_warn() {
        let resp = response(
            None,
            &[
                ("deprecated_headers", CheckStatus::Warn),
                ("info_leakage", CheckStatus::Skip),
            ],
        );
        assert_eq!(status_of(&resp, "http.hygiene"), Status::Warn);
    }

    #[test]
    fn headline_format() {
        let resp = response(
            Some(true),
            &[
                ("hsts", CheckStatus::Pass),
                ("csp", CheckStatus::Fail),
                ("cors", CheckStatus::Pass),
            ],
        );
        let (_, presentation) = translated(&resp);
        assert_eq!(presentation["headline"], "HTTPS ✓  HSTS ✓  CSP ✗  CORS ✓");
    }
}
