use std::convert::Infallible;
use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use governor::{Quota, RateLimiter};
use indexmap::IndexMap;
use netray_common::enrichment::EnrichmentClient;
use netray_common::rate_limit::{KeyedLimiter, check_keyed_cost};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{
    Block, BlockId, Fields, Register, Row, Shape, ShapeKind, Value as EvValue, ValueType,
};
use netray_model::{CheckId, CheckResult, Protocol, Status};
use serde_json::json;

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
/// response headers, the server that answered (or the error kind of the probe), the CORS probe
/// and the cookies.
pub const REGISTER: Register = Register {
    blocks: &[
        ("http.redirects", ShapeKind::Chain),
        ("http.port80", ShapeKind::Chain),
        ("http.port80_probe", ShapeKind::Fields),
        ("http.headers", ShapeKind::Table),
        ("http.server", ShapeKind::Fields),
        ("http.cors", ShapeKind::Fields),
        ("http.cookies", ShapeKind::Table),
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
        ("http.server", "response_ms", ValueType::Duration),
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
    ],
};

const VOLATILE: &[EvidencePath] = &[
    EvidencePath("http.server/response_ms"),
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
        evidence,
    }
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

/// The blocks a check judged by: `https_redirect` the port-80 chain and the port-80 probe, and
/// the HTTPS chain when it hit the redirect limit; on a refused, reset or failed-TLS HTTPS probe
/// the ten response checks judged the error held in `http.server`.
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

/// One row per lowercased header name; a repeated header is joined into one value, `set-cookie`
/// line by line, any other with `, `. Reads the raw headers, or after a JSON replay, which holds
/// one value per name, the V1 `headers`.
fn headers_block(resp: &InspectResponse) -> Block {
    let raw: Vec<(&String, &String)> = if resp.raw_headers.is_empty() {
        resp.headers.iter().collect()
    } else {
        resp.raw_headers.iter().map(|(n, v)| (n, v)).collect()
    };
    let mut joined: IndexMap<String, String> = IndexMap::new();
    for (name, value) in raw {
        let name = name.to_lowercase();
        let sep = if name == "set-cookie" { "\n" } else { ", " };
        joined
            .entry(name)
            .and_modify(|v| {
                v.push_str(sep);
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
/// V1 reports, not the host a redirect ended on. `response_ms` is the duration of the whole
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
    fields.push(("response_ms".into(), EvValue::Duration(resp.duration_ms)));
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

    fn translated(resp: &InspectResponse) -> (Vec<CheckResult>, serde_json::Value) {
        match translate(resp) {
            SectionOutcome::Measured {
                checks,
                presentation,
                evidence: _,
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
