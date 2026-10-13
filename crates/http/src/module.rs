use std::convert::Infallible;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use governor::{Quota, RateLimiter};
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

use crate::config::{InspectConfig, ModuleConfig};
use crate::error::AppError;
use crate::inspect::assembler::{InspectResponse, ProbeError, ProbeErrorKind};
use crate::inspect::request::Outbound;
use crate::quality::types::{CheckStatus, QualityCheck};

static CHECKS: LazyLock<Vec<CheckId>> = LazyLock::new(|| {
    [
        "https_redirect",
        "hsts",
        "cors",
        "cookie_secure",
        "csp",
        "x_frame_options",
        "x_content_type_options",
        "referrer_policy",
        "permissions_policy",
        "deprecated_headers",
        "info_leakage",
        "caching",
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

    fn needs_address_records(&self) -> bool {
        true
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

/// Maps the inspection onto the twelve HTTP checks and the V1 headline and extras.
pub fn translate(resp: &InspectResponse) -> SectionOutcome {
    let quality = &resp.quality.checks;

    // https_redirect: synthesised from the http_upgrade field; a redirect_limit message rides on it.
    let https_redirect_status = match &resp.http_upgrade {
        None => CheckStatus::Skip,
        Some(u) if u.redirects_to_https => CheckStatus::Pass,
        Some(_) => CheckStatus::Fail,
    };
    let upgrade_error = resp.http_upgrade.as_ref().and_then(|u| u.error.as_ref());
    let mut https_redirect_findings = match &resp.http_upgrade {
        None => vec![NOT_REPORTED.to_string()],
        Some(_) if https_redirect_status == CheckStatus::Fail => {
            vec!["HTTP port 80 does not redirect to HTTPS".to_string()]
        }
        Some(_) => vec![],
    };
    https_redirect_findings.extend(upgrade_error.map(|e| e.message.clone()));
    if let Some(message) = find_check(quality, "redirect_limit").and_then(|c| c.message.clone()) {
        https_redirect_findings.push(message);
    }
    let https_redirect = match upgrade_error {
        Some(e) if unanswered(e) && https_redirect_status != CheckStatus::Pass => (
            Status::Unmeasured,
            vec![format!("HTTP port 80 probe got no answer: {}", e.message)],
        ),
        _ => (
            https_redirect_status.clone().into(),
            https_redirect_findings,
        ),
    };

    let response_override = resp.error.as_ref().map(|e| {
        if unanswered(e) {
            (Status::Unmeasured, format!("HTTPS probe: {}", e.message))
        } else {
            (Status::Fail, format!("no HTTPS response: {}", e.message))
        }
    });
    let cors_override = resp.cors.error.as_ref().map(|e| {
        if unanswered(e) {
            (Status::Unmeasured, format!("CORS probe: {}", e.message))
        } else {
            (Status::NotApplicable, "no CORS probe response".to_string())
        }
    });

    let checks = http_checks()
        .iter()
        .map(|id| {
            let name = id.to_string();
            let name = name.strip_prefix("http.").unwrap_or(&name);
            if name == "https_redirect" {
                return CheckResult {
                    id: id.clone(),
                    status: https_redirect.0,
                    findings: https_redirect.1.clone(),
                    evidence: vec![],
                };
            }
            let probe_override = if name == "cors" {
                &cors_override
            } else {
                &response_override
            };
            if let Some((status, finding)) = probe_override {
                return CheckResult {
                    id: id.clone(),
                    status: *status,
                    findings: vec![finding.clone()],
                    evidence: vec![],
                };
            }
            let (status, findings) = match find_check(quality, name) {
                None => (CheckStatus::Skip, vec![NOT_REPORTED.to_string()]),
                Some(c) => match c.status {
                    CheckStatus::Pass => (CheckStatus::Pass, vec![]),
                    CheckStatus::Skip => (
                        CheckStatus::Skip,
                        vec![
                            c.message
                                .clone()
                                .unwrap_or_else(|| NOT_APPLICABLE.to_string()),
                        ],
                    ),
                    _ => (c.status.clone(), c.message.clone().into_iter().collect()),
                },
            };
            CheckResult {
                id: id.clone(),
                status: status.into(),
                findings,
                evidence: vec![],
            }
        })
        .collect();

    let hsts_status = lookup_check(quality, "hsts");
    let cors_status = lookup_check(quality, "cors");
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

const NOT_REPORTED: &str = "not reported";
/// Finding of a check spectra measured as not applicable without giving a reason.
const NOT_APPLICABLE: &str = "not applicable";

/// A probe that got no answer, as opposed to one that was answered with a refusal.
fn unanswered(e: &ProbeError) -> bool {
    matches!(
        e.kind,
        ProbeErrorKind::Timeout | ProbeErrorKind::Blocked | ProbeErrorKind::Unreachable
    )
}

fn find_check<'a>(checks: &'a [QualityCheck], name: &str) -> Option<&'a QualityCheck> {
    checks.iter().find(|c| c.name == name)
}

fn lookup_check(checks: &[QualityCheck], name: &str) -> CheckStatus {
    find_check(checks, name)
        .map(|c| c.status.clone())
        .unwrap_or(CheckStatus::Skip)
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
            error: None,
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
    fn twelve_checks_always_present() {
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
                "http.cors",
                "http.cookie_secure",
                "http.csp",
                "http.x_frame_options",
                "http.x_content_type_options",
                "http.referrer_policy",
                "http.permissions_policy",
                "http.deprecated_headers",
                "http.info_leakage",
                "http.caching",
            ]
        );
    }

    #[test]
    fn skip_without_message_is_not_applicable_not_unreported() {
        let resp = response(Some(true), &[("cookie_secure", CheckStatus::Skip)]);
        let (checks, _) = translated(&resp);
        let c = checks
            .iter()
            .find(|c| c.id.to_string() == "http.cookie_secure")
            .expect("cookie_secure");
        assert_eq!(c.status, Status::NotApplicable);
        assert_eq!(c.findings, vec!["not applicable".to_string()]);
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
    fn security_header_checks_carry_their_own_status() {
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
        assert_eq!(status_of(&resp, "http.csp"), Status::Pass);
        assert_eq!(status_of(&resp, "http.x_frame_options"), Status::Warn);
        assert_eq!(status_of(&resp, "http.referrer_policy"), Status::Fail);
        assert_eq!(
            status_of(&resp, "http.permissions_policy"),
            Status::NotApplicable
        );
    }

    #[test]
    fn hygiene_checks_absent_are_not_applicable() {
        let resp = response(
            None,
            &[
                ("deprecated_headers", CheckStatus::Warn),
                ("info_leakage", CheckStatus::Skip),
            ],
        );
        assert_eq!(status_of(&resp, "http.deprecated_headers"), Status::Warn);
        assert_eq!(status_of(&resp, "http.caching"), Status::NotApplicable);
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
