use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use governor::{Quota, RateLimiter};
use indexmap::IndexMap;
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{
    Block, BlockId, Fields, Node, Register, Row, Shape, ShapeKind, Value as EvValue, ValueType,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use url::Url;

use crate::config::{InspectConfig, ModuleConfig};
use crate::error::AppError;
use crate::inspect::assembler::{
    HttpUpgrade, InspectResponse, ProbeError, ProbeErrorKind, RedirectHop,
};
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
        "coop",
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

/// The HTTP register: the HTTPS and port-80 redirect chains, the port-80 probe, the HTTPS
/// response headers, the server that answered (or the error kind of the probe), the CORS probe,
/// the cookies and the parsed header blocks (CSP, HSTS, caching, CDN, cross-origin isolation).
pub const REGISTER: Register = Register {
    blocks: &[
        ("http.redirects", ShapeKind::Chain),
        ("http.port80", ShapeKind::Chain),
        ("http.port80_probe", ShapeKind::Fields),
        ("http.headers", ShapeKind::Table),
        ("http.server", ShapeKind::Fields),
        ("http.cors", ShapeKind::Fields),
        ("http.cookies", ShapeKind::Table),
        ("http.csp", ShapeKind::Tree),
        ("http.hsts", ShapeKind::Fields),
        ("http.caching", ShapeKind::Fields),
        ("http.cdn", ShapeKind::Fields),
        ("http.isolation", ShapeKind::Fields),
    ],
    keys: &[
        ("http.redirects", "url", ValueType::Text),
        ("http.redirects", "status", ValueType::Int),
        ("http.redirects", "location", ValueType::Text),
        ("http.redirects", "http_version", ValueType::Text),
        ("http.port80", "url", ValueType::Text),
        ("http.port80", "status", ValueType::Int),
        ("http.port80", "location", ValueType::Text),
        ("http.port80", "http_version", ValueType::Text),
        ("http.port80_probe", "url", ValueType::Text),
        ("http.port80_probe", "status_code", ValueType::Int),
        ("http.port80_probe", "location", ValueType::Text),
        ("http.port80_probe", "redirects_to_https", ValueType::Bool),
        ("http.port80_probe", "error_kind", ValueType::Text),
        ("http.headers", "name", ValueType::Text),
        ("http.headers", "value", ValueType::Text),
        ("http.server", "final_url", ValueType::Text),
        ("http.server", "status_code", ValueType::Int),
        ("http.server", "http_version", ValueType::Text),
        ("http.server", "inspection_ms", ValueType::Duration),
        ("http.server", "address", ValueType::Addr),
        ("http.server", "org", ValueType::Text),
        ("http.server", "network_type", ValueType::Text),
        ("http.server", "error_kind", ValueType::Text),
        ("http.cors", "allows_any_origin", ValueType::Bool),
        ("http.cors", "reflects_origin", ValueType::Bool),
        ("http.cors", "allows_credentials", ValueType::Bool),
        ("http.cors", "allow_origin", ValueType::Text),
        ("http.cors", "allow_credentials_header", ValueType::Text),
        ("http.cookies", "cookie", ValueType::Text),
        ("http.cookies", "name", ValueType::Text),
        ("http.cookies", "secure", ValueType::Bool),
        ("http.cookies", "http_only", ValueType::Bool),
        ("http.cookies", "same_site", ValueType::Text),
        ("http.csp", "report_only", ValueType::Bool),
        ("http.csp", "issue_count", ValueType::Int),
        ("http.csp", "issue", ValueType::Text),
        ("http.csp", "directive", ValueType::Text),
        ("http.csp", "source", ValueType::Text),
        ("http.hsts", "max_age", ValueType::Int),
        ("http.hsts", "include_subdomains", ValueType::Bool),
        ("http.hsts", "preload", ValueType::Bool),
        ("http.caching", "directive", ValueType::Text),
        ("http.caching", "max_age", ValueType::Int),
        ("http.caching", "etag", ValueType::Text),
        ("http.caching", "etag_weak", ValueType::Bool),
        ("http.caching", "last_modified", ValueType::Time),
        ("http.caching", "vary", ValueType::Text),
        ("http.caching", "age", ValueType::Int),
        ("http.cdn", "provider", ValueType::Text),
        ("http.cdn", "cache_status", ValueType::Text),
        ("http.cdn", "indicator", ValueType::Text),
        ("http.isolation", "coop", ValueType::Text),
        ("http.isolation", "coep", ValueType::Text),
        ("http.isolation", "corp", ValueType::Text),
        ("http.isolation", "unreadable", ValueType::Text),
        ("http.isolation", "missing", ValueType::Text),
    ],
};

const VOLATILE: &[EvidencePath] = &[
    EvidencePath("http.server/inspection_ms"),
    EvidencePath("http.headers/value@date"),
    EvidencePath("http.headers/value@age"),
    EvidencePath("http.headers/value@set-cookie"),
    EvidencePath("http.headers/value@server-timing"),
    EvidencePath("http.headers/value@etag"),
    EvidencePath("http.headers/value@last-modified"),
    EvidencePath("http.headers/value@x-request-id"),
    EvidencePath("http.headers/value@cf-ray"),
    EvidencePath("http.headers/value@x-amz-cf-id"),
    EvidencePath("http.headers/value@x-correlation-id"),
    EvidencePath("http.headers/value@x-trace-id"),
    EvidencePath("http.headers/value@traceparent"),
    EvidencePath("http.headers/value@x-amzn-requestid"),
    EvidencePath("http.headers/value@expires"),
    EvidencePath("http.headers/value@x-timer"),
    EvidencePath("http.headers/value@x-served-by"),
    EvidencePath("http.headers/value@x-cache"),
    EvidencePath("http.headers/value@x-cache-hits"),
    EvidencePath("http.headers/value@cf-cache-status"),
    EvidencePath("http.headers/value@report-to"),
    EvidencePath("http.headers/value@via"),
    EvidencePath("http.headers/value@x-fastly-request-id"),
    EvidencePath("http.headers/value@x-vercel-id"),
    EvidencePath("http.headers/value@x-vercel-cache"),
    EvidencePath("http.headers/value@x-github-request-id"),
    EvidencePath("http.headers/value@x-amz-cf-pop"),
    EvidencePath("http.server/address"),
    EvidencePath("http.server/org"),
    EvidencePath("http.server/network_type"),
    EvidencePath("http.caching/etag"),
    EvidencePath("http.caching/etag_weak"),
    EvidencePath("http.caching/last_modified"),
    EvidencePath("http.caching/age"),
    EvidencePath("http.cdn/cache_status"),
    EvidencePath("http.csp/source"),
];

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

impl crate::job::HttpInspect for HttpModule {
    fn target<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<SocketAddr, AppError>> {
        Box::pin(crate::input::validate_target(url))
    }

    fn inspect<'a>(
        &'a self,
        url: &'a Url,
        target: SocketAddr,
        start: Instant,
    ) -> BoxFuture<'a, Result<InspectResponse, AppError>> {
        Box::pin(crate::inspect::inspect_and_assemble(
            url,
            target,
            &self.inspect,
            &self.outbound,
            self.enrichment.as_deref(),
            None,
            start,
        ))
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
        VOLATILE
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

/// Maps the inspection onto the thirteen HTTP checks and their blocks.
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

    let evidence = blocks(resp);
    let checks = http_checks()
        .iter()
        .map(|id| {
            let mut r = check_result(
                id,
                resp,
                &https_redirect,
                &response_override,
                &cors_override,
            );
            let name = id.to_string();
            let name = name.strip_prefix("http.").unwrap_or(&name);
            if r.status != Status::Unmeasured {
                r.evidence = references(name, resp);
                r.evidence.retain(|id| evidence.iter().any(|b| b.id == *id));
            }
            r
        })
        .collect();

    SectionOutcome::Measured { checks, evidence }
}

/// The status and findings of one check, before its references.
fn check_result(
    id: &CheckId,
    resp: &InspectResponse,
    https_redirect: &(Status, Vec<String>),
    response_override: &Option<(Status, String)>,
    cors_override: &Option<(Status, String)>,
) -> CheckResult {
    let quality = &resp.quality.checks;
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
        cors_override
    } else {
        response_override
    };
    if let Some((status, finding)) = probe_override {
        return CheckResult {
            id: id.clone(),
            status: *status,
            findings: vec![finding.clone()],
            evidence: vec![],
        };
    }
    if name == "coop" {
        let unreadable = resp
            .unreadable_headers
            .iter()
            .any(|h| h == "cross-origin-opener-policy");
        let (status, findings) = coop_verdict(&resp.security.coop, unreadable);
        return CheckResult {
            id: id.clone(),
            status,
            findings,
            evidence: vec![],
        };
    }
    let csp = &resp.security.csp;
    if name == "csp"
        && (csp.enforced || csp.report_only)
        && let Some(c) = find_check(quality, name)
    {
        let mut findings = Vec::new();
        if csp.report_only {
            findings.push(CSP_REPORT_ONLY.to_string());
        }
        findings.extend(
            crate::inspect::csp::issues(&csp.directives)
                .into_iter()
                .map(|i| i.message),
        );
        return CheckResult {
            id: id.clone(),
            status: c.status.clone().into(),
            findings,
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
}

/// `http.coop` from the received Cross-Origin-Opener-Policy value, judged here rather than in
/// the V1 security report, whose presence-only verdict stays on the V1 wire. The policy is the
/// token before any `;` (a structured-field token: no surrounding space, case-sensitive);
/// `same-origin`, `same-origin-allow-popups` and `noopener-allow-popups` pass, anything else or
/// no readable header warns. A received header whose first line is not readable as text warns
/// with a finding saying so.
fn coop_verdict(
    coop: &crate::inspect::assembler::HeaderCheck,
    unreadable: bool,
) -> (Status, Vec<String>) {
    if coop.value.is_none() && unreadable {
        return (
            Status::Warn,
            vec!["Cross-Origin-Opener-Policy header is not readable as text".to_string()],
        );
    }
    let Some(value) = coop.value.as_deref() else {
        let finding = coop
            .message
            .clone()
            .unwrap_or_else(|| "Missing cross-origin-opener-policy header".to_string());
        return (Status::Warn, vec![finding]);
    };
    let token = value.trim().split(';').next().unwrap_or("");
    match token {
        "same-origin" | "same-origin-allow-popups" | "noopener-allow-popups" => {
            (Status::Pass, vec![])
        }
        "unsafe-none" => (
            Status::Warn,
            vec!["Permissive policy: unsafe-none".to_string()],
        ),
        _ => (Status::Warn, vec![format!("Unrecognized value: {value}")]),
    }
}

/// The blocks a check judged by: `https_redirect` the port-80 chain and the port-80 probe, and
/// the HTTPS chain when it hit the redirect limit; on a refused, reset or failed-TLS HTTPS probe
/// the eleven response checks judged the error held in `http.server`.
fn references(name: &str, resp: &InspectResponse) -> Vec<BlockId> {
    let block = match name {
        "https_redirect" => {
            // The chain holds the hops; the probe block holds the end of the followed port-80
            // chain or its error, which decides the verdict. Absent blocks are dropped later.
            let mut refs = vec![
                static_block_id("http.port80"),
                static_block_id("http.port80_probe"),
            ];
            if resp.redirect_limit_reached.is_some()
                || find_check(&resp.quality.checks, "redirect_limit").is_some()
            {
                refs.push(static_block_id("http.redirects"));
            }
            return refs;
        }
        "cors" => "http.cors",
        _ if resp.error.is_some() => "http.server",
        "cookie_secure" => "http.cookies",
        "hsts" => {
            return vec![
                static_block_id("http.headers"),
                static_block_id("http.hsts"),
            ];
        }
        "coop" => {
            return vec![
                static_block_id("http.headers"),
                static_block_id("http.isolation"),
            ];
        }
        "csp" => return vec![static_block_id("http.headers"), static_block_id("http.csp")],
        "caching" => {
            return vec![
                static_block_id("http.headers"),
                static_block_id("http.caching"),
            ];
        }
        _ => "http.headers",
    };
    vec![static_block_id(block)]
}

/// The serialized name of a probe error kind, e.g. `refused`.
fn error_kind(kind: ProbeErrorKind) -> Option<String> {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
}

fn static_block_id(id: &str) -> BlockId {
    BlockId::parse(id).expect("static block id")
}

/// The blocks of the inspection. A probe without a response gives no block of its response:
/// the headers and cookies need the HTTPS response, the CORS block the CORS response, and a
/// redirect chain needs a response or at least one hop before the error.
fn blocks(resp: &InspectResponse) -> Vec<Block> {
    let answered = resp.error.is_none();
    let mut blocks = Vec::new();
    if answered || !resp.redirects.is_empty() {
        blocks.push(chain_block("http.redirects", "Redirects", &resp.redirects));
    }
    if let Some(u) = &resp.http_upgrade
        && (u.error.is_none() || !u.redirects.is_empty())
    {
        blocks.push(chain_block(
            "http.port80",
            "Port 80 redirects",
            &u.redirects,
        ));
    }
    if let Some(u) = &resp.http_upgrade {
        blocks.push(port80_probe_block(u));
    }
    if answered {
        blocks.push(headers_block(resp));
        blocks.extend(csp_block(resp));
        blocks.extend(hsts_block(resp));
        blocks.push(caching_block(resp));
        blocks.extend(cdn_block(resp));
        blocks.push(isolation_block(resp));
    }
    blocks.push(server_block(resp));
    if resp.cors.error.is_none() {
        let cors = &resp.cors;
        blocks.push(Block {
            id: static_block_id("http.cors"),
            title: "CORS".into(),
            shape: Shape::Fields(
                vec![
                    (
                        "allows_any_origin".into(),
                        EvValue::Bool(cors.allows_any_origin),
                    ),
                    (
                        "reflects_origin".into(),
                        EvValue::Bool(cors.reflects_origin),
                    ),
                    (
                        "allows_credentials".into(),
                        EvValue::Bool(cors.allows_credentials),
                    ),
                ]
                .into_iter()
                .chain(
                    [
                        ("allow_origin", &cors.allow_origin),
                        ("allow_credentials_header", &cors.allow_credentials_header),
                    ]
                    .into_iter()
                    .filter_map(|(k, v)| Some((k.into(), EvValue::Text(v.clone()?)))),
                )
                .collect(),
            ),
        });
    }
    if answered {
        blocks.push(cookies_block(resp));
    }
    blocks
}

/// The CSP as a tree: the root holds the report-only marker, the issue count and the issues of
/// a directive the policy lacks; one child per directive in policy order. Only with a policy.
fn csp_block(resp: &InspectResponse) -> Option<Block> {
    let csp = &resp.security.csp;
    if !(csp.enforced || csp.report_only) {
        return None;
    }
    let issues = crate::inspect::csp::issues(&csp.directives);
    let mut fields: Fields = vec![
        ("report_only".into(), EvValue::Bool(csp.report_only)),
        ("issue_count".into(), EvValue::Int(issues.len() as i64)),
    ];
    fields.extend(
        issues
            .iter()
            .filter(|i| !csp.directives.contains_key(&i.directive))
            .map(|i| ("issue".into(), EvValue::Text(i.message.clone()))),
    );
    let children = csp
        .directives
        .iter()
        .map(|(name, sources)| {
            let mut fields: Fields = vec![("directive".into(), EvValue::Text(name.clone()))];
            fields.extend(
                sources
                    .iter()
                    .map(|s| ("source".into(), EvValue::Text(s.clone()))),
            );
            fields.extend(
                issues
                    .iter()
                    .filter(|i| i.directive == *name)
                    .map(|i| ("issue".into(), EvValue::Text(i.message.clone()))),
            );
            Node {
                fields,
                children: vec![],
            }
        })
        .collect();
    Some(Block {
        id: static_block_id("http.csp"),
        title: "Content Security Policy".into(),
        shape: Shape::Tree(Node { fields, children }),
    })
}

/// HSTS as parsed, only when the header was received.
fn hsts_block(resp: &InspectResponse) -> Option<Block> {
    if !resp.headers.contains_key("strict-transport-security") {
        return None;
    }
    let hsts = &resp.security.hsts;
    let mut fields: Fields = Vec::new();
    if let Some(max_age) = hsts.max_age.and_then(|v| i64::try_from(v).ok()) {
        fields.push(("max_age".into(), EvValue::Int(max_age)));
    }
    fields.push((
        "include_subdomains".into(),
        EvValue::Bool(hsts.include_sub_domains),
    ));
    fields.push(("preload".into(), EvValue::Bool(hsts.preload)));
    Some(Block {
        id: static_block_id("http.hsts"),
        title: "HSTS".into(),
        shape: Shape::Fields(fields),
    })
}

/// The cache headers the response carried, each only when known. The block stands whenever the
/// HTTPS probe answered, so its presence never turns on a volatile key alone.
fn caching_block(resp: &InspectResponse) -> Block {
    let c = &resp.caching;
    let mut fields: Fields = Vec::new();
    if let Some(cache_control) = &c.cache_control {
        fields.extend(
            cache_control_directives(cache_control)
                .into_iter()
                .map(|d| ("directive".into(), EvValue::Text(d))),
        );
    }
    if let Some(max_age) = c.directives.max_age.and_then(|v| i64::try_from(v).ok()) {
        fields.push(("max_age".into(), EvValue::Int(max_age)));
    }
    if let Some(etag) = &c.etag {
        fields.push(("etag".into(), EvValue::Text(etag.clone())));
        fields.push(("etag_weak".into(), EvValue::Bool(etag.starts_with("W/"))));
    }
    if let Some(last_modified) = c.last_modified.as_deref().and_then(http_date) {
        fields.push(("last_modified".into(), EvValue::Time(last_modified)));
    }
    fields.extend(
        c.vary
            .iter()
            .filter(|v| !v.is_empty())
            .map(|v| ("vary".into(), EvValue::Text(v.clone()))),
    );
    if let Some(age) = c.age.and_then(|v| i64::try_from(v).ok()) {
        fields.push(("age".into(), EvValue::Int(age)));
    }
    Block {
        id: static_block_id("http.caching"),
        title: "Caching".into(),
        shape: Shape::Fields(fields),
    }
}

/// Every directive of a Cache-Control list in received order, repeats kept: the name lowercased,
/// then `=` and the argument as received (trimmed, quotes kept). A comma inside a quoted argument
/// does not split it; empty members are skipped.
fn cache_control_directives(value: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (i, ch) in value.char_indices() {
        match ch {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ',' if !quoted => {
                members.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    members.push(&value[start..]);
    members
        .into_iter()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| match m.split_once('=') {
            Some((name, arg)) => format!("{}={}", name.trim().to_lowercase(), arg.trim()),
            None => m.to_lowercase(),
        })
        .collect()
}

/// An HTTP date (RFC 9110 §5.6.7: IMF-fixdate and the obsolete RFC 850 and asctime forms) as an
/// RFC 3339 UTC time; `None` when it is none of them.
fn http_date(v: &str) -> Option<String> {
    use chrono::{NaiveDateTime, SecondsFormat, TimeZone, Utc};
    let v = v.trim();
    let utc = chrono::DateTime::parse_from_rfc2822(v)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(v, "%A, %d-%b-%y %H:%M:%S GMT")
                .or_else(|_| NaiveDateTime::parse_from_str(v, "%a %b %e %H:%M:%S %Y"))
                .ok()
                .map(|n| Utc.from_utc_datetime(&n))
        })?;
    Some(utc.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// The detected CDN; no block without one.
fn cdn_block(resp: &InspectResponse) -> Option<Block> {
    let cdn = &resp.cdn;
    let provider = cdn.detected.as_ref()?;
    let mut fields: Fields = vec![("provider".into(), EvValue::Text(provider.clone()))];
    if let Some(status) = &cdn.cache_status {
        fields.push(("cache_status".into(), EvValue::Text(status.clone())));
    }
    fields.extend(
        cdn.indicators
            .iter()
            .map(|i| ("indicator".into(), EvValue::Text(i.clone()))),
    );
    Some(Block {
        id: static_block_id("http.cdn"),
        title: "CDN".into(),
        shape: Shape::Fields(fields),
    })
}

/// COOP, COEP and CORP: the value when readable, the header name under `unreadable` when received
/// but not readable as text, under `missing` when not received.
fn isolation_block(resp: &InspectResponse) -> Block {
    let s = &resp.security;
    let mut fields: Fields = Vec::new();
    let mut unreadable: Fields = Vec::new();
    let mut missing: Fields = Vec::new();
    for (key, header, check) in [
        ("coop", "cross-origin-opener-policy", &s.coop),
        ("coep", "cross-origin-embedder-policy", &s.coep),
        ("corp", "cross-origin-resource-policy", &s.corp),
    ] {
        match &check.value {
            Some(v) => fields.push((key.into(), EvValue::Text(v.clone()))),
            None if resp.unreadable_headers.iter().any(|h| h == header) => {
                unreadable.push(("unreadable".into(), EvValue::Text(header.into())));
            }
            None => missing.push(("missing".into(), EvValue::Text(header.into()))),
        }
    }
    fields.extend(unreadable);
    fields.extend(missing);
    Block {
        id: static_block_id("http.isolation"),
        title: "Cross-origin isolation".into(),
        shape: Shape::Fields(fields),
    }
}

/// The port-80 probe: the URL it requested; the status and `Location` of the response that
/// ended the followed port-80 chain, not port 80's own first answer; whether that chain led to
/// HTTPS; and the error kind when it got no response.
fn port80_probe_block(u: &HttpUpgrade) -> Block {
    let mut fields: Fields = Vec::new();
    if let Some(url) = &u.url {
        fields.push(("url".into(), EvValue::Text(url.clone())));
    }
    if let (Some(status), None) = (u.status_code, &u.error) {
        fields.push(("status_code".into(), EvValue::Int(i64::from(status))));
    }
    if let Some(location) = &u.location {
        fields.push(("location".into(), EvValue::Text(location.clone())));
    }
    // Only an answered probe measured whether port 80 redirects to HTTPS.
    if u.error.is_none() {
        fields.push((
            "redirects_to_https".into(),
            EvValue::Bool(u.redirects_to_https),
        ));
    }
    if let Some(kind) = u.error.as_ref().and_then(|e| error_kind(e.kind)) {
        fields.push(("error_kind".into(), EvValue::Text(kind)));
    }
    Block {
        id: static_block_id("http.port80_probe"),
        title: "Port 80 probe".into(),
        shape: Shape::Fields(fields),
    }
}

fn chain_block(id: &str, title: &str, hops: &[RedirectHop]) -> Block {
    let hops = hops
        .iter()
        .map(|hop| {
            let mut fields: Fields = vec![
                ("url".into(), EvValue::Text(hop.url.clone())),
                ("status".into(), EvValue::Int(i64::from(hop.status))),
            ];
            if let Some(location) = &hop.location {
                fields.push(("location".into(), EvValue::Text(location.clone())));
            }
            if !hop.http_version.is_empty() {
                fields.push((
                    "http_version".into(),
                    EvValue::Text(hop.http_version.clone()),
                ));
            }
            fields
        })
        .collect();
    Block {
        id: static_block_id(id),
        title: title.into(),
        shape: Shape::Chain(hops),
    }
}

/// One row per lowercased header name; a repeated header is joined into one value with `, `,
/// except `set-cookie`, which keeps one row per value in received order (`set-cookie`,
/// `set-cookie#2`, ...). Reads the raw headers, or after a JSON replay, which holds one value
/// per name, the V1 `headers`.
fn headers_block(resp: &InspectResponse) -> Block {
    let raw: Vec<(&String, &String)> = if resp.raw_headers.is_empty() {
        resp.headers.iter().collect()
    } else {
        resp.raw_headers.iter().map(|(n, v)| (n, v)).collect()
    };
    let mut joined: IndexMap<String, String> = IndexMap::new();
    let raw_names: std::collections::HashSet<String> = raw
        .iter()
        .map(|(n, _)| n.to_lowercase())
        .filter(|n| n != "set-cookie")
        .collect();
    let raw_names: std::collections::HashSet<&str> = raw_names.iter().map(String::as_str).collect();
    let mut cookies = 0usize;
    for (name, value) in raw {
        let mut name = name.to_lowercase();
        if name == "set-cookie" {
            // A server may send a header literally named `set-cookie#<n>` (`#` is a token
            // character): skip to the next free ordinal rather than overwrite it.
            loop {
                cookies += 1;
                if cookies > 1 {
                    name = format!("set-cookie#{cookies}");
                }
                if !joined.contains_key(&name) && !raw_names.contains(name.as_str()) {
                    break;
                }
            }
            joined.insert(name, value.clone());
            continue;
        }
        joined
            .entry(name)
            .and_modify(|v| {
                v.push_str(", ");
                v.push_str(value);
            })
            .or_insert_with(|| value.clone());
    }
    Block {
        id: static_block_id("http.headers"),
        title: "Response headers".into(),
        shape: Shape::Table {
            key: "name".into(),
            columns: ["name", "value"].map(String::from).into(),
            rows: joined
                .into_iter()
                .map(|(name, value)| vec![EvValue::Text(name), EvValue::Text(value)])
                .collect(),
        },
    }
}

/// The server that answered the HTTPS probe; without a response, the target and the probe's
/// error kind. `address`, `org` and `network_type` describe the first-hop target, the address
/// V1 reports, not the host a redirect ended on. `inspection_ms` is the duration of the whole
/// inspection, all three probes and the enrichment lookup, not of one response.
fn server_block(resp: &InspectResponse) -> Block {
    let mut fields: Fields = Vec::new();
    if resp.error.is_none() {
        fields.push(("final_url".into(), EvValue::Text(resp.final_url.clone())));
        fields.push(("status_code".into(), EvValue::Int(i64::from(resp.status))));
        fields.push((
            "http_version".into(),
            EvValue::Text(resp.http_version.clone()),
        ));
    }
    fields.push(("inspection_ms".into(), EvValue::Duration(resp.duration_ms)));
    if let Ok(ip) = resp.enrichment.ip.parse() {
        fields.push(("address".into(), EvValue::Addr(ip)));
    }
    if let Some(org) = &resp.enrichment.org {
        fields.push(("org".into(), EvValue::Text(org.clone())));
    }
    if let Some(network_type) = &resp.enrichment.ip_type {
        fields.push(("network_type".into(), EvValue::Text(network_type.clone())));
    }
    if let Some(kind) = resp.error.as_ref().and_then(|e| error_kind(e.kind)) {
        fields.push(("error_kind".into(), EvValue::Text(kind)));
    }
    Block {
        id: static_block_id("http.server"),
        title: "Server".into(),
        shape: Shape::Fields(fields),
    }
}

/// One row per `Set-Cookie`, keyed `<name>; <domain>; <path>` with an empty part for an absent
/// attribute; a repeat of the same key gets the suffix `#<n>`, its ordinal among them. An absent
/// `SameSite` is an empty `same_site`.
fn cookies_block(resp: &InspectResponse) -> Block {
    let mut keys = std::collections::HashSet::new();
    let mut rows: Vec<Row> = Vec::new();
    for c in &resp.cookies {
        let base = format!(
            "{}; {}; {}",
            c.name,
            c.domain.as_deref().unwrap_or_default(),
            c.path.as_deref().unwrap_or_default()
        );
        let mut key = base.clone();
        let mut n = 1;
        while !keys.insert(key.clone()) {
            n += 1;
            key = format!("{base}#{n}");
        }
        rows.push(vec![
            EvValue::Text(key),
            EvValue::Text(c.name.clone()),
            EvValue::Bool(c.secure),
            EvValue::Bool(c.httponly),
            EvValue::Text(c.samesite.clone().unwrap_or_default()),
        ]);
    }
    Block {
        id: static_block_id("http.cookies"),
        title: "Cookies".into(),
        shape: Shape::Table {
            key: "cookie".into(),
            columns: ["cookie", "name", "secure", "http_only", "same_site"]
                .map(String::from)
                .into(),
            rows,
        },
    }
}

const CSP_REPORT_ONLY: &str =
    "Content-Security-Policy is report-only: browsers report violations but enforce nothing";
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
            url: None,
            location: None,
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

    fn translated(resp: &InspectResponse) -> (Vec<CheckResult>, Vec<Block>) {
        match translate(resp) {
            SectionOutcome::Measured { checks, evidence } => (checks, evidence),
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
    fn set_cookie_rows_skip_a_received_set_cookie_hash_header() {
        let mut resp = response(None, &[]);
        resp.raw_headers = [
            ("set-cookie#2", "x"),
            ("set-cookie", "a=1"),
            ("set-cookie", "b=2"),
        ]
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .into();
        let Shape::Table { rows, .. } = headers_block(&resp).shape else {
            panic!("http.headers is not a table");
        };
        let row = |k: &str| {
            rows.iter()
                .find(|r| r[0] == EvValue::Text(k.into()))
                .map(|r| r[1].clone())
        };
        assert_eq!(row("set-cookie#2"), Some(EvValue::Text("x".into())));
        assert_eq!(row("set-cookie"), Some(EvValue::Text("a=1".into())));
        assert_eq!(row("set-cookie#3"), Some(EvValue::Text("b=2".into())));
    }

    #[test]
    fn http_date_reads_the_three_forms() {
        for v in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
        ] {
            assert_eq!(http_date(v).as_deref(), Some("1994-11-06T08:49:37Z"), "{v}");
        }
        assert_eq!(http_date("yesterday"), None);
    }

    #[test]
    fn thirteen_checks_always_present() {
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
                "http.coop",
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
    fn headline_inputs() {
        let resp = response(
            Some(true),
            &[
                ("hsts", CheckStatus::Pass),
                ("csp", CheckStatus::Fail),
                ("cors", CheckStatus::Pass),
            ],
        );
        let (_, evidence) = translated(&resp);
        let probe = evidence
            .iter()
            .find(|b| b.id.to_string() == "http.port80_probe")
            .expect("port-80 probe block");
        let Shape::Fields(fields) = &probe.shape else {
            panic!("http.port80_probe is not fields: {probe:?}");
        };
        assert!(fields.contains(&("redirects_to_https".into(), EvValue::Bool(true))));
        assert_eq!(status_of(&resp, "http.hsts"), Status::Pass);
        assert_eq!(status_of(&resp, "http.csp"), Status::Fail);
        assert_eq!(status_of(&resp, "http.cors"), Status::Pass);
    }
}
