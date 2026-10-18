//! The inspector frame (spec v2-inspectors, requirements 9-14): `GET /inspect/{tool}` renders the
//! form, `POST /inspect/{tool}/run` streams one job. The jobs themselves reach the routes only
//! through [`InspectorJobs`].

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::PathRejection;
use axum::extract::{Form, FromRequest, Path, Query, Request, State};
use axum::http::header::{CACHE_CONTROL, RETRY_AFTER};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, FuturesUnordered};
use netray_common::ip_extract::IpExtractor;
use netray_model::evidence::Block;
use percent_encoding::{percent_decode_str, utf8_percent_encode};
use serde::de::{Deserialize, Deserializer};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout_at};
use url::{Host, Url};

use super::errors::LOCATION_SET;
use super::headers::secure;
use super::{Frame, not_found, page, server_error};
use crate::components::{self, QueryInputView};
use crate::evidence::{block_view, live_run_line};
use crate::stream::{end, fragment};

const RESULT_ID: &str = "inspect-result";
const BACKLINK_ID: &str = "inspect-backlink";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Dns,
    Tls,
    Http,
    Ip,
}

impl Tool {
    const ALL: [Tool; 4] = [Tool::Dns, Tool::Tls, Tool::Http, Tool::Ip];

    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Dns => "dns",
            Tool::Tls => "tls",
            Tool::Http => "http",
            Tool::Ip => "ip",
        }
    }

    /// The form field that carries the input.
    pub fn field(self) -> &'static str {
        match self {
            Tool::Dns => "q",
            Tool::Tls => "h",
            Tool::Http => "url",
            Tool::Ip => "ip",
        }
    }

    fn parse(name: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.as_str() == name)
    }

    fn index(self) -> usize {
        self as usize
    }

    fn heading(self) -> &'static str {
        match self {
            Tool::Dns => "DNS lookup",
            Tool::Tls => "TLS inspector",
            Tool::Http => "HTTP inspector",
            Tool::Ip => "IP lookup",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Tool::Dns => "example.com A",
            Tool::Tls => "example.com:443",
            Tool::Http => "https://example.com/",
            Tool::Ip => "192.0.2.1",
        }
    }
}

/// One inspector's budget: `[inspect.<tool>]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct InspectLimits {
    pub deadline_ms: u64,
    pub max_concurrent: u32,
}

impl Default for InspectLimits {
    fn default() -> Self {
        Self::with_deadline(30_000)
    }
}

impl InspectLimits {
    const fn with_deadline(deadline_ms: u64) -> Self {
        Self {
            deadline_ms,
            max_concurrent: 8,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsPatch {
    deadline_ms: Option<u64>,
    max_concurrent: Option<u32>,
}

fn merged<'de, D: Deserializer<'de>>(d: D, base: InspectLimits) -> Result<InspectLimits, D::Error> {
    let patch = LimitsPatch::deserialize(d)?;
    Ok(InspectLimits {
        deadline_ms: patch.deadline_ms.unwrap_or(base.deadline_ms),
        max_concurrent: patch.max_concurrent.unwrap_or(base.max_concurrent),
    })
}

fn dns_limits<'de, D: Deserializer<'de>>(d: D) -> Result<InspectLimits, D::Error> {
    merged(d, InspectConfig::default().dns)
}

fn tls_limits<'de, D: Deserializer<'de>>(d: D) -> Result<InspectLimits, D::Error> {
    merged(d, InspectConfig::default().tls)
}

fn http_limits<'de, D: Deserializer<'de>>(d: D) -> Result<InspectLimits, D::Error> {
    merged(d, InspectConfig::default().http)
}

fn ip_limits<'de, D: Deserializer<'de>>(d: D) -> Result<InspectLimits, D::Error> {
    merged(d, InspectConfig::default().ip)
}

/// `[inspect]` of the lens config: a partial table keeps the other defaults.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct InspectConfig {
    #[serde(deserialize_with = "dns_limits")]
    pub dns: InspectLimits,
    #[serde(deserialize_with = "tls_limits")]
    pub tls: InspectLimits,
    #[serde(deserialize_with = "http_limits")]
    pub http: InspectLimits,
    #[serde(deserialize_with = "ip_limits")]
    pub ip: InspectLimits,
}

impl Default for InspectConfig {
    fn default() -> Self {
        Self {
            dns: InspectLimits::with_deadline(30_000),
            tls: InspectLimits::with_deadline(15_000),
            http: InspectLimits::with_deadline(30_000),
            ip: InspectLimits::with_deadline(5_000),
        }
    }
}

impl InspectConfig {
    /// Refuses a zero budget, naming the key.
    pub fn validate(&self) -> Result<(), String> {
        for tool in Tool::ALL {
            let limits = self.limits(tool);
            let name = tool.as_str();
            if limits.deadline_ms < 1 {
                return Err(format!("inspect.{name}.deadline_ms must be at least 1"));
            }
            if limits.max_concurrent < 1 {
                return Err(format!("inspect.{name}.max_concurrent must be at least 1"));
            }
        }
        Ok(())
    }

    pub fn limits(&self, tool: Tool) -> InspectLimits {
        match tool {
            Tool::Dns => self.dns,
            Tool::Tls => self.tls,
            Tool::Http => self.http,
            Tool::Ip => self.ip,
        }
    }
}

/// What a POST hands the job seam.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub tool: Tool,
    pub input: String,
    pub client: IpAddr,
}

/// One part of a job, streamed as it finishes.
pub struct JobPart {
    pub label: String,
    pub output: BoxFuture<'static, Result<Vec<Block>, String>>,
}

pub struct Job {
    parts: Vec<JobPart>,
}

impl Job {
    pub fn new(parts: Vec<JobPart>) -> Job {
        Job { parts }
    }
}

/// The seam to the job functions (requirement 13); the real implementation lives in the binary.
pub trait InspectorJobs: Send + Sync + 'static {
    /// The shared per-client and per-target limiter: `Err(retry_after)` refuses.
    fn admit(&self, tool: Tool, client: IpAddr, input: &str) -> Result<(), Duration>;
    /// Starts the job; `Err` is the job function's validation message.
    fn start(&self, req: JobRequest) -> Result<Job, String>;
}

#[derive(Clone)]
pub struct InspectDeps {
    jobs: Arc<dyn InspectorJobs>,
    config: InspectConfig,
    extractor: Arc<IpExtractor>,
    slots: [Arc<Semaphore>; 4],
}

impl InspectDeps {
    /// Trusts no proxy; see [`InspectDeps::with_ip_extractor`].
    pub fn new(jobs: Arc<dyn InspectorJobs>, config: InspectConfig) -> Self {
        let slots = Tool::ALL
            .map(|tool| Arc::new(Semaphore::new(config.limits(tool).max_concurrent as usize)));
        Self {
            jobs,
            config,
            extractor: Arc::new(IpExtractor::new(&[])),
            slots,
        }
    }

    pub fn with_ip_extractor(mut self, extractor: Arc<IpExtractor>) -> Self {
        self.extractor = extractor;
        self
    }
}

#[derive(Template)]
#[template(path = "pages/inspector.html")]
struct InspectorPage {
    frame: Frame,
    heading: &'static str,
    query: QueryInputView,
    stream_url: String,
    autostart: bool,
    link: Option<(String, String)>,
}

#[derive(Template)]
#[template(path = "partials/inspect_backlink.html")]
struct BacklinkFragment {
    link: Option<(String, String)>,
}

#[derive(Template)]
#[template(path = "partials/inspect_start.html")]
struct StartFragment {
    run_line: String,
    parts: Vec<(usize, String)>,
}

#[derive(Template)]
#[template(path = "partials/inspect_part.html")]
struct PartFragment {
    n: usize,
    html: Option<String>,
    text: String,
}

#[derive(Template)]
#[template(path = "partials/inspect_notice.html")]
struct NoticeFragment {
    message: String,
}

/// `GET /inspect/{tool}` and `POST /inspect/{tool}/run`; any other tool is the 404 page.
pub fn inspect_router(deps: InspectDeps) -> Router {
    Router::new()
        .route("/inspect/{tool}", get(show))
        .route("/inspect/{tool}/run", post(run))
        .with_state(deps)
        .layer(axum::middleware::map_response(
            |response: Response| async move { secure(response) },
        ))
}

async fn show(tool: Result<Path<String>, PathRejection>, uri: Uri) -> Response {
    let Some(tool) = tool.ok().and_then(|Path(name)| Tool::parse(&name)) else {
        return not_found().await;
    };
    let prefill = Query::<HashMap<String, String>>::try_from_uri(&uri)
        .ok()
        .and_then(|Query(mut q)| q.remove(tool.field()))
        .filter(|v| !v.is_empty());
    let link = prefill.as_deref().and_then(|input| backlink(tool, input));
    let name = tool.as_str();
    page(
        StatusCode::OK,
        "no-cache",
        &InspectorPage {
            frame: Frame::new(
                &format!("{} - netray", tool.heading()),
                "Run one check live and read the evidence as it arrives.",
                &format!("/inspect/{name}"),
            ),
            heading: tool.heading(),
            query: QueryInputView {
                action: format!("/inspect/{name}"),
                name: tool.field().to_string(),
                label: "Target".to_string(),
                placeholder: tool.placeholder().to_string(),
                value: prefill.clone(),
                submit: "Run".to_string(),
            },
            stream_url: format!("/inspect/{name}/run"),
            autostart: prefill.is_some(),
            link,
        },
    )
}

/// The back-link target for an input: the `/{domain}` path and the domain, when it has one.
fn backlink(tool: Tool, input: &str) -> Option<(String, String)> {
    let domain = registrable_domain(&target_host(tool, input)?)?;
    let href = format!("/{}", utf8_percent_encode(&domain, LOCATION_SET));
    Some((href, domain))
}

/// The caller address: `CF-Connecting-IP` dropped, then the configured extractor.
fn client(extractor: &IpExtractor, headers: &HeaderMap, peer: Option<SocketAddr>) -> IpAddr {
    let mut headers = headers.clone();
    headers.remove("cf-connecting-ip");
    let peer = peer.unwrap_or_else(|| SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0));
    extractor.extract(&headers, peer)
}

type Events = BoxStream<'static, Result<Event, Infallible>>;

fn sse(status: StatusCode, events: Vec<Event>) -> Response {
    let stream: Events = futures::stream::iter(events.into_iter().map(Ok)).boxed();
    (status, Sse::new(stream)).into_response()
}

fn notice(status: StatusCode, message: String) -> Response {
    match (NoticeFragment { message }).render() {
        Ok(html) => sse(status, vec![fragment(RESULT_ID, &html), end()]),
        Err(_) => server_error(),
    }
}

async fn run(
    State(deps): State<InspectDeps>,
    tool: Result<Path<String>, PathRejection>,
    req: Request,
) -> Response {
    let Some(tool) = tool.ok().and_then(|Path(name)| Tool::parse(&name)) else {
        return not_found().await;
    };
    let (parts, body) = req.into_parts();
    if netray_common::prefetch::is_prefetch(&parts.headers) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(CACHE_CONTROL, "no-store")],
        )
            .into_response();
    }
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0);
    let client = client(&deps.extractor, &parts.headers, peer);
    let req = Request::from_parts(parts, body);
    let mut form = match Form::<HashMap<String, String>>::from_request(req, &()).await {
        Ok(Form(form)) => form,
        Err(rejection) => return rejection.into_response(),
    };
    let input = form.remove(tool.field()).unwrap_or_default();

    if let Err(retry_after) = deps.jobs.admit(tool, client, &input) {
        let secs = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
        let mut response = notice(
            StatusCode::TOO_MANY_REQUESTS,
            format!("too many requests; try again in {secs} s"),
        );
        response.headers_mut().insert(RETRY_AFTER, secs.into());
        return response;
    }
    let Ok(permit) = deps.slots[tool.index()].clone().try_acquire_owned() else {
        return notice(
            StatusCode::SERVICE_UNAVAILABLE,
            "busy, try again in a moment".to_string(),
        );
    };
    let link = backlink(tool, &input);
    let job = match deps.jobs.start(JobRequest {
        tool,
        input,
        client,
    }) {
        Ok(job) => job,
        Err(message) => return notice(StatusCode::OK, message),
    };
    let deadline = Instant::now() + Duration::from_millis(deps.config.limits(tool).deadline_ms);
    match stream_job(job, link, permit, deadline) {
        Some(events) => (StatusCode::OK, Sse::new(events)).into_response(),
        None => server_error(),
    }
}

type PartOutput = (usize, Result<Vec<Block>, String>);

struct Run {
    _permit: OwnedSemaphorePermit,
    pending: FuturesUnordered<BoxFuture<'static, PartOutput>>,
    waiting: BTreeSet<usize>,
    queue: VecDeque<Event>,
    deadline: Instant,
    ended: bool,
}

fn part_event(n: usize, result: Result<Vec<Block>, String>) -> Option<Event> {
    let (html, text) = match result {
        Ok(blocks) => {
            let mut html = String::new();
            for block in &blocks {
                html.push_str(&components::evidence_block(&block_view(block)).ok()?);
            }
            (Some(html), String::new())
        }
        Err(text) => (None, text),
    };
    let html = (PartFragment { n, html, text }).render().ok()?;
    Some(fragment(&format!("inspect-part-{n}"), &html))
}

fn stream_job(
    job: Job,
    link: Option<(String, String)>,
    permit: OwnedSemaphorePermit,
    deadline: Instant,
) -> Option<Events> {
    let start = StartFragment {
        run_line: components::run_line(&live_run_line()).ok()?,
        parts: job
            .parts
            .iter()
            .enumerate()
            .map(|(n, part)| (n, part.label.clone()))
            .collect(),
    }
    .render()
    .ok()?;
    let backlink = (BacklinkFragment { link }).render().ok()?;
    let pending: FuturesUnordered<_> = job
        .parts
        .into_iter()
        .enumerate()
        .map(|(n, part)| -> BoxFuture<'static, PartOutput> {
            Box::pin(async move { (n, part.output.await) })
        })
        .collect();
    let run = Run {
        _permit: permit,
        waiting: (0..pending.len()).collect(),
        pending,
        queue: VecDeque::from([
            fragment(RESULT_ID, &start),
            fragment(BACKLINK_ID, &backlink),
        ]),
        deadline,
        ended: false,
    };
    let events = futures::stream::unfold(run, |mut run| async move {
        loop {
            if let Some(event) = run.queue.pop_front() {
                return Some((Ok(event), run));
            }
            if run.ended {
                return None;
            }
            if run.pending.is_empty() {
                run.ended = true;
                return Some((Ok(end()), run));
            }
            match timeout_at(run.deadline, run.pending.next()).await {
                Ok(Some((n, result))) => {
                    run.waiting.remove(&n);
                    let event = part_event(n, result)
                        .unwrap_or_else(|| fragment(&format!("inspect-part-{n}"), "render failed"));
                    return Some((Ok(event), run));
                }
                Ok(None) => run.pending.clear(),
                Err(_) => {
                    run.pending.clear();
                    let late: Vec<usize> = std::mem::take(&mut run.waiting).into_iter().collect();
                    for n in late {
                        if let Some(event) = part_event(n, Err("timed out".to_string())) {
                            run.queue.push_back(event);
                        }
                    }
                }
            }
        }
    });
    Some(events.boxed())
}

/// The host a posted or prefilled input names, lower-cased and without a trailing dot; `None`
/// for an address or an input with no host.
fn target_host(tool: Tool, input: &str) -> Option<String> {
    let raw = match tool {
        Tool::Dns => input.split_whitespace().next()?.to_string(),
        Tool::Tls => tls_host(input)?,
        Tool::Http => http_host(input)?,
        Tool::Ip => return None,
    };
    let host = raw.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<IpAddr>().is_ok() {
        return None;
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return None;
    }
    Some(host)
}

/// The TLS inspector's order: percent-decode, then bracket, then `host:port`.
fn tls_host(input: &str) -> Option<String> {
    let decoded = percent_decode_str(input).decode_utf8().ok()?;
    let input = decoded.trim();
    if input.starts_with('[') {
        return None;
    }
    match input.rfind(':') {
        Some(colon) => {
            let after = &input[colon + 1..];
            let is_ports =
                !after.is_empty() && after.chars().all(|c| c.is_ascii_digit() || c == ',');
            is_ports.then(|| input[..colon].to_string())
        }
        None => Some(input.to_string()),
    }
}

/// The HTTP inspector's reading of the URL: `https://` assumed, a domain host only.
fn http_host(input: &str) -> Option<String> {
    let input = input.trim();
    let url = if input.contains("://") {
        Url::parse(input)
    } else {
        Url::parse(&format!("https://{input}"))
    }
    .ok()?;
    match url.host()? {
        Host::Domain(domain) => Some(domain.to_string()),
        _ => None,
    }
}

/// The registrable domain of a host name (Public Suffix List); `None` for a public suffix.
pub fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let domain = psl::domain_str(&host)?;
    netray_common::domain::validate(domain).ok()
}
