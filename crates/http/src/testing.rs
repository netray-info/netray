use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use netray_engine::{BoxFuture, EvidencePath, Facts, Module, RunContext, SectionOutcome};
use netray_model::evidence::{Block, Shape, Value as EvValue};
use netray_model::{CheckId, Protocol};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use url::{Host, Url};

use crate::error::AppError;
use crate::inspect::assembler::{InspectResponse, ProbeError, ProbeErrorKind, RedirectHop};
use crate::inspect::{EnrichmentData, InspectResult, TaskResult};
use crate::job::HttpInspect;
use crate::module::{http_checks, translate};

struct GoldenModule(SectionOutcome);

impl Module for GoldenModule {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }

    fn checks(&self) -> &'static [CheckId] {
        http_checks()
    }

    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }

    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move { self.0.clone() })
    }
}

/// An HTTP module that answers every run with the translation of `contract_json`, a spectra
/// inspect response.
pub fn golden_module(contract_json: &str) -> Box<dyn Module> {
    let resp: InspectResponse = serde_json::from_str(contract_json)
        .expect("golden contract decodes as an inspect response");
    Box::new(GoldenModule(translate(&resp)))
}

const SPECTRA_JSON: &str = include_str!("../../../tests/fixtures/contracts/spectra-inspect.json");

/// An offline `HttpInspect`: names resolve from a map (default `93.184.215.14`), the real target
/// policy applies, and every inspection answers the spectra contract fixture.
#[derive(Default)]
pub struct StubHttpInspect {
    hosts: HashMap<String, Vec<IpAddr>>,
}

impl StubHttpInspect {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_host(mut self, name: &str, addrs: Vec<IpAddr>) -> Self {
        self.hosts.insert(name.to_lowercase(), addrs);
        self
    }
}

impl HttpInspect for StubHttpInspect {
    fn target<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<SocketAddr, AppError>> {
        Box::pin(async move {
            let addrs = match url.host() {
                Some(Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
                Some(Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
                Some(Host::Domain(name)) => self
                    .hosts
                    .get(&name.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| vec![IpAddr::V4(Ipv4Addr::new(93, 184, 215, 14))]),
                None => return Err(AppError::InvalidUrl("URL has no host".to_string())),
            };
            crate::input::pick_target(&addrs, url.port_or_known_default().unwrap_or(443))
        })
    }

    fn inspect<'a>(
        &'a self,
        url: &'a Url,
        _target: SocketAddr,
        _start: Instant,
    ) -> BoxFuture<'a, Result<InspectResponse, AppError>> {
        Box::pin(async move {
            let mut resp: InspectResponse = serde_json::from_str(SPECTRA_JSON)
                .expect("the spectra contract fixture decodes as an inspect response");
            resp.url = url.to_string();
            Ok(resp)
        })
    }
}

fn block<'a>(blocks: &'a [Block], id: &str) -> Option<&'a Block> {
    blocks.iter().find(|b| b.id.to_string() == id)
}

fn fields<'a>(blocks: &'a [Block], id: &str) -> &'a [(String, EvValue)] {
    match block(blocks, id).map(|b| &b.shape) {
        Some(Shape::Fields(f)) => f,
        _ => &[],
    }
}

fn text(fields: &[(String, EvValue)], key: &str) -> Option<String> {
    fields.iter().find_map(|(k, v)| match v {
        EvValue::Text(t) if k == key => Some(t.clone()),
        _ => None,
    })
}

fn int(fields: &[(String, EvValue)], key: &str) -> Option<i64> {
    fields.iter().find_map(|(k, v)| match v {
        EvValue::Int(i) if k == key => Some(*i),
        _ => None,
    })
}

fn flag(fields: &[(String, EvValue)], key: &str) -> Option<bool> {
    fields.iter().find_map(|(k, v)| match v {
        EvValue::Bool(b) if k == key => Some(*b),
        _ => None,
    })
}

fn hops(blocks: &[Block], id: &str) -> Vec<RedirectHop> {
    let Some(Shape::Chain(chain)) = block(blocks, id).map(|b| &b.shape) else {
        return vec![];
    };
    chain
        .iter()
        .map(|hop| RedirectHop {
            url: text(hop, "url").unwrap_or_default(),
            status: int(hop, "status")
                .and_then(|s| u16::try_from(s).ok())
                .unwrap_or(0),
            location: text(hop, "location"),
            http_version: text(hop, "http_version").unwrap_or_default(),
        })
        .collect()
}

fn probe_error(fields: &[(String, EvValue)]) -> Option<ProbeError> {
    let kind: ProbeErrorKind =
        serde_json::from_value(serde_json::Value::String(text(fields, "error_kind")?)).ok()?;
    Some(ProbeError {
        kind,
        message: format!("Request failed: {}", text(fields, "error_kind")?),
    })
}

/// The `http.headers` rows as a header map: one entry per row, the `set-cookie#<n>` rows back
/// as repeats of `set-cookie`.
fn header_map(blocks: &[Block]) -> HeaderMap {
    let mut map = HeaderMap::new();
    let Some(Shape::Table { rows, .. }) = block(blocks, "http.headers").map(|b| &b.shape) else {
        return map;
    };
    for row in rows {
        let (Some(EvValue::Text(name)), Some(EvValue::Text(value))) = (row.first(), row.get(1))
        else {
            continue;
        };
        let name = match name.strip_prefix("set-cookie#") {
            Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => "set-cookie",
            _ => name.as_str(),
        };
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            map.append(name, value);
        }
    }
    map
}

fn header_of(pairs: &[(&'static str, Option<String>)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        if let Some(Ok(value)) = value.as_deref().map(HeaderValue::from_str) {
            map.insert(HeaderName::from_static(name), value);
        }
    }
    map
}

/// The HTTP module's outcome rebuilt from `http.*` evidence blocks: the inspection result
/// reassembled from the headers table, the chains, the probe, the server and the CORS blocks,
/// and translated as a live run would. Unknown keys are ignored; what a block does not carry
/// (probe error messages, the redirect limit) takes its default.
pub async fn from_evidence(domain: &str, blocks: &[Block]) -> SectionOutcome {
    let url = crate::input::parse_url(&format!("https://{domain}"))
        .expect("from_evidence: domain forms an https URL");
    let server = fields(blocks, "http.server");
    let address: Option<IpAddr> = server.iter().find_map(|(k, v)| match v {
        EvValue::Addr(a) if k == "address" => Some(*a),
        _ => None,
    });
    let addr = SocketAddr::new(address.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)), 443);

    let https_error = probe_error(server);
    let https = TaskResult {
        final_url: text(server, "final_url").unwrap_or_else(|| url.to_string()),
        status: int(server, "status_code")
            .and_then(|s| u16::try_from(s).ok())
            .unwrap_or(0),
        http_version: text(server, "http_version").unwrap_or_default(),
        headers: header_map(blocks),
        redirects: hops(blocks, "http.redirects"),
        redirect_limit_reached: false,
        error: https_error.clone(),
    };

    let http_upgrade = block(blocks, "http.port80_probe").map(|_| {
        let probe = fields(blocks, "http.port80_probe");
        let redirects = hops(blocks, "http.port80");
        let probe_url = text(probe, "url").unwrap_or_else(|| format!("http://{domain}/"));
        let location = text(probe, "location");
        let mut final_url = redirects
            .last()
            .and_then(|h| h.location.clone())
            .unwrap_or_else(|| probe_url.clone());
        if flag(probe, "redirects_to_https") == Some(true)
            && !final_url.starts_with("https://")
            && !redirects.iter().any(|h| {
                h.location
                    .as_deref()
                    .is_some_and(|l| l.starts_with("https://"))
            })
        {
            final_url = location
                .clone()
                .filter(|l| l.starts_with("https://"))
                .unwrap_or_else(|| url.to_string());
        }
        TaskResult {
            final_url,
            status: int(probe, "status_code")
                .and_then(|s| u16::try_from(s).ok())
                .unwrap_or(0),
            http_version: String::new(),
            headers: header_of(&[("location", location)]),
            redirects,
            redirect_limit_reached: false,
            error: probe_error(probe),
        }
    });

    let cors_block = block(blocks, "http.cors");
    let cors_fields = fields(blocks, "http.cors");
    let cors = TaskResult {
        final_url: url.to_string(),
        status: 0,
        http_version: String::new(),
        headers: header_of(&[
            (
                "access-control-allow-origin",
                text(cors_fields, "allow_origin"),
            ),
            (
                "access-control-allow-credentials",
                text(cors_fields, "allow_credentials_header"),
            ),
        ]),
        redirects: vec![],
        redirect_limit_reached: false,
        error: match cors_block {
            Some(_) => None,
            None => Some(https_error.unwrap_or(ProbeError {
                kind: ProbeErrorKind::Reset,
                message: "Request failed: reset".to_string(),
            })),
        },
    };

    let duration_ms = server
        .iter()
        .find_map(|(k, v)| match v {
            EvValue::Duration(d) if k == "inspection_ms" => Some(*d),
            _ => None,
        })
        .unwrap_or(0);
    let enrichment = EnrichmentData {
        org: text(server, "org"),
        ip_type: text(server, "network_type"),
        threat: None,
        role: None,
    };
    let mut resp = crate::inspect::assemble_response(
        &url,
        addr,
        InspectResult {
            https,
            http_upgrade,
            cors,
        },
        enrichment,
        None,
        duration_ms,
    );
    if address.is_none() {
        resp.enrichment.ip.clear();
    }
    translate(&resp)
}
