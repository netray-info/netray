//! The inspector frame (spec v2-inspectors, requirements 9-14): `GET /inspect/{tool}` renders the
//! form, `POST /inspect/{tool}/run` streams one job. The jobs themselves reach the routes only
//! through [`InspectorJobs`].

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::convert::Infallible;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::Router;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::rejection::PathRejection;
use axum::extract::{Form, FromRequest, Path, Query, Request, State};
use axum::http::header::{CACHE_CONTROL, RETRY_AFTER};
use axum::http::{StatusCode, Uri};
use axum::response::sse::Event;
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, FuturesUnordered};
use netray_common::ip_extract::IpExtractor;
use netray_engine::profile::Profiles;
use netray_model::evidence::Block;
use netray_model::{CheckResult, Grade, Protocol, Status};
use percent_encoding::{percent_decode_str, utf8_percent_encode};
use serde::de::{Deserialize, Deserializer};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout_at};
use url::{Host, Url};

use super::errors::LOCATION_SET;
use super::headers::secure;
use super::{Frame, not_found, page, server_error};
use crate::api::InspectJobs;
use crate::client::client_ip;
use crate::components::{self, QueryInputView, RunLineView, SpectrumView};
use crate::entries::view::grade_letter;
use crate::evidence::{
    Badge, EvidenceRow, block_view_with, live_rows, live_run_title, shared_run_title,
};
use crate::export::{self, ExportInput};
use crate::links;
use crate::pages::result::views::line_code;
use crate::spectrum::{RunInput, layout};
use crate::stream::{end, fragment};

mod shared;

use shared::SharedRun;
pub use shared::SharedRuns;

const RESULT_ID: &str = "inspect-result";
const BAND_ID: &str = "inspect-band";
const SHARE_ID: &str = "inspect-share";
const EXPORT_ID: &str = "inspect-export";
const EXPIRED: &str =
    "This shared result has expired or never existed; results are kept for one hour.";
/// The part text of an API result that holds no block (a check that timed out or was not
/// applicable).
const NO_API_EVIDENCE: &str = "This API result holds no evidence.";
const NO_API_BAND: &str = "The band is not part of an API result.";
const TABS_ID: &str = "inspector-tabs";
const BAND_WIDTH: u32 = 432;
/// The longest the run route waits for the back link's grade before it starts the job.
const GRADE_READ_BUDGET: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Dns,
    Tls,
    Http,
    Email,
    Ip,
}

impl Tool {
    pub(crate) const ALL: [Tool; 5] = [Tool::Dns, Tool::Tls, Tool::Http, Tool::Email, Tool::Ip];

    pub fn as_str(self) -> &'static str {
        match self {
            Tool::Dns => "dns",
            Tool::Tls => "tls",
            Tool::Http => "http",
            Tool::Email => "email",
            Tool::Ip => "ip",
        }
    }

    /// The form field that carries the input.
    pub fn field(self) -> &'static str {
        match self {
            Tool::Dns => "q",
            Tool::Tls => "h",
            Tool::Http => "url",
            Tool::Email => "d",
            Tool::Ip => "ip",
        }
    }

    fn parse(name: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.as_str() == name)
    }

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Tool::Dns => "DNS inspector",
            Tool::Tls => "TLS inspector",
            Tool::Http => "HTTP inspector",
            Tool::Email => "Email inspector",
            Tool::Ip => "IP inspector",
        }
    }

    fn tagline(self) -> &'static str {
        match self {
            Tool::Dns => "DNS, resolved",
            Tool::Tls => "TLS, illuminated",
            Tool::Http => "HTTP, inspected",
            Tool::Email => "Email, authenticated",
            Tool::Ip => "IP, located",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Dns => "Query",
            Tool::Tls => "Host",
            Tool::Http => "URL",
            Tool::Email => "Domain",
            Tool::Ip => "Address",
        }
    }

    fn examples(self) -> &'static [&'static str] {
        match self {
            Tool::Dns => &["example.com A", "_dmarc.example.com TXT"],
            Tool::Tls => &["example.com", "netray.info"],
            Tool::Http => &["https://example.com/", "https://www.example.com/"],
            Tool::Email => &["example.com", "netray.info"],
            Tool::Ip => &["192.0.2.1", "2001:db8::1"],
        }
    }

    fn what(self) -> &'static str {
        match self {
            Tool::Dns => {
                "Resolves a name live, with the record types, flags and resolvers of a dig query."
            }
            Tool::Tls => {
                "Connects to a host and reads its certificate chain, protocol versions and ciphers."
            }
            Tool::Http => "Fetches a URL and reads its redirects, security headers and cookies.",
            Tool::Email => {
                "Reads a domain's mail posture: MX, SPF, DKIM, DMARC, MTA-STS and the rest of its email authentication."
            }
            Tool::Ip => "Looks up an address: its network, location, reverse name and reputation.",
        }
    }

    fn syntax(self) -> &'static str {
        match self {
            Tool::Dns => "A name, then optional record types, +flags and @resolvers.",
            Tool::Tls => "A host, optionally with a port: example.com or example.com:443.",
            Tool::Http => "A full URL, scheme included.",
            Tool::Email => "A domain, then DKIM selectors separated by spaces or commas.",
            Tool::Ip => "An IPv4 or IPv6 address.",
        }
    }

    fn guide(self) -> (&'static str, &'static str) {
        match self {
            Tool::Dns => ("/guide/record-types", "Record types guide"),
            Tool::Tls => ("/guide/tls-protocol", "TLS protocol guide"),
            Tool::Http => ("/guide/security-headers", "Security headers guide"),
            Tool::Email => ("/guide/email-auth", "Email authentication guide"),
            Tool::Ip => ("/guide/ip-enrichment", "IP enrichment guide"),
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            Tool::Dns => "example.com A",
            Tool::Tls => "example.com:443",
            Tool::Http => "https://example.com/",
            Tool::Email => "example.com",
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

fn email_limits<'de, D: Deserializer<'de>>(d: D) -> Result<InspectLimits, D::Error> {
    merged(d, InspectConfig::default().email)
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
    #[serde(deserialize_with = "email_limits")]
    pub email: InspectLimits,
    #[serde(deserialize_with = "ip_limits")]
    pub ip: InspectLimits,
}

impl Default for InspectConfig {
    fn default() -> Self {
        Self {
            dns: InspectLimits::with_deadline(30_000),
            tls: InspectLimits::with_deadline(15_000),
            http: InspectLimits::with_deadline(30_000),
            // Past the email inspection's own 30 s cap, so the page shows what it finished.
            email: InspectLimits::with_deadline(35_000),
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
            Tool::Email => self.email,
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
    /// The route's deadline for this job (`[inspect.<tool>] deadline_ms` from the request).
    pub deadline: std::time::Instant,
    /// The key the finished run is shared under; `None` for the caller's own address.
    pub share_key: Option<String>,
}

/// One part of a job, streamed as it finishes.
pub struct JobPart {
    pub label: String,
    pub output: BoxFuture<'static, Result<Vec<Block>, String>>,
}

type BandChecks = BoxFuture<'static, Result<Vec<CheckResult>, String>>;

/// Why a job's admission gate refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    RateLimited(Duration),
    Invalid(String),
}

type Gate = BoxFuture<'static, Result<(), Refusal>>;

pub struct Job {
    parts: Vec<JobPart>,
    band: Option<(Protocol, BandChecks)>,
    gate: Option<Gate>,
}

impl Job {
    pub fn new(parts: Vec<JobPart>) -> Job {
        Job {
            parts,
            band: None,
            gate: None,
        }
    }

    /// Awaited by the route before it answers: `RateLimited` answers 429, `Invalid` one notice,
    /// a gate past the deadline one "timed out" notice. `Ok` streams the job.
    pub fn with_gate(mut self, gate: Gate) -> Job {
        self.gate = Some(gate);
        self
    }

    /// The protocol's live band: the job's checks, streamed after the parts. `Err(text)` renders
    /// that text in the band region.
    pub fn with_band(mut self, protocol: Protocol, checks: BandChecks) -> Job {
        self.band = Some((protocol, checks));
        self
    }
}

/// The seam to the job functions (requirement 13); the real implementation is
/// `netray_web::api::InspectJobs`.
pub trait InspectorJobs: Send + Sync + 'static {
    /// The shared per-client and per-target limiter: `Err(retry_after)` refuses.
    fn admit(&self, tool: Tool, client: IpAddr, input: &str) -> Result<(), Duration>;
    /// Starts the job; `Err` is the job function's validation message.
    fn start(&self, req: JobRequest) -> Result<Job, String>;
    /// The badges of each address the lookup knows; an address it does not know is left out.
    fn enrich(&self, addrs: Vec<IpAddr>) -> BoxFuture<'static, Vec<(IpAddr, Vec<Badge>)>> {
        let _ = addrs;
        Box::pin(async { Vec::new() })
    }
}

#[derive(Clone)]
pub struct InspectDeps {
    jobs: Arc<dyn InspectorJobs>,
    config: InspectConfig,
    extractor: Arc<IpExtractor>,
    slots: [Arc<Semaphore>; 5],
    history: Option<Arc<netray_store::HistoryStore>>,
    shared: SharedRuns,
    dns_results: Option<Arc<dyn DnsResults>>,
}

/// A DNS result the API's cache holds under a key, as the shell renders it.
pub struct CachedDns {
    pub query: String,
    pub mode: String,
    pub blocks: Vec<Block>,
}

/// The API's DNS result cache, as the shell reads it: a key only it holds opens in the DNS
/// inspector.
pub trait DnsResults: Send + Sync + 'static {
    fn result<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<CachedDns>>;
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
            history: None,
            shared: SharedRuns::new(),
            dns_results: None,
        }
    }

    pub fn with_ip_extractor(mut self, extractor: Arc<IpExtractor>) -> Self {
        self.extractor = extractor;
        self
    }

    /// The store the run route reads the latest grade from for the back link; it never writes.
    pub fn with_history(mut self, history: Option<Arc<netray_store::HistoryStore>>) -> Self {
        self.history = history;
        self
    }

    /// Replaces the cache of shared runs.
    pub fn with_shared_runs(mut self, shared: SharedRuns) -> Self {
        self.shared = shared;
        self
    }

    /// The API's DNS result cache: a key only it holds opens in the DNS inspector.
    pub fn with_dns_results(mut self, dns_results: impl DnsResults) -> Self {
        self.dns_results = Some(Arc::new(dns_results));
        self
    }
}

struct TabView {
    label: &'static str,
    href: String,
    current: bool,
}

struct ExampleView {
    href: String,
    text: &'static str,
}

struct ShellView {
    tool: &'static str,
    field: &'static str,
    name: &'static str,
    tagline: &'static str,
    query: QueryInputView,
    stream_url: String,
    autostart: bool,
    tabs: String,
    examples: Vec<ExampleView>,
    slot: Option<String>,
    result: Option<String>,
    notice: Option<&'static str>,
    suggest: Option<SuggestView>,
    help: HelpView,
}

/// The help dialog's per-tool content; the DNS lists are empty for the other tools.
struct HelpView {
    what: &'static str,
    syntax: &'static str,
    guide_href: &'static str,
    guide_label: &'static str,
    resolvers: Vec<String>,
    flags: Vec<String>,
    types: Vec<String>,
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

fn help(tool: Tool, suggest: Option<&SuggestView>) -> HelpView {
    let (guide_href, guide_label) = tool.guide();
    HelpView {
        what: tool.what(),
        syntax: tool.syntax(),
        guide_href,
        guide_label,
        resolvers: suggest.map(|s| words(&s.servers)).unwrap_or_default(),
        flags: suggest.map(|s| words(&s.flags)).unwrap_or_default(),
        types: suggest.map(|s| words(&s.types)).unwrap_or_default(),
    }
}

/// The autocomplete tables of the DNS shell, each a space-separated list.
struct SuggestView {
    name: &'static str,
    types: String,
    flags: String,
    servers: String,
}

#[derive(Template)]
#[template(path = "pages/inspector.html")]
struct InspectorPage {
    frame: Frame,
    shell: ShellView,
}

#[derive(Template)]
#[template(path = "partials/inspect_dns_builder.html")]
struct DnsBuilder;

#[derive(Template)]
#[template(path = "partials/inspect_tabs.html")]
struct TabsFragment {
    tabs: Vec<TabView>,
}

#[derive(Template)]
#[template(path = "partials/inspect_start.html")]
struct StartFragment {
    title: RunLineView,
    target: String,
    link: Option<(String, String)>,
    grade: Option<&'static str>,
    parts: Vec<(usize, String, Option<String>)>,
    band: BandSlot,
    share: String,
    export: String,
    related: Option<String>,
}

/// The band region of the start fragment: none, a placeholder the stream fills, or rendered.
enum BandSlot {
    Absent,
    Pending,
    Done(String),
}

#[derive(Template)]
#[template(path = "partials/inspect_share.html")]
struct ShareFragment {
    tool: &'static str,
    key: Option<String>,
}

#[derive(Template)]
#[template(path = "partials/inspect_part.html")]
struct PartFragment {
    n: usize,
    label: String,
    html: Option<String>,
    text: String,
}

#[derive(Template)]
#[template(path = "partials/inspect_band.html")]
struct BandFragment {
    band: Option<BandView>,
    text: String,
}

/// The band region of a finished run: the zoomed plate, its named lines and the live check list.
struct BandView {
    spectrum: String,
    lines: Vec<(String, String, &'static str)>,
    caption: String,
    heading: String,
    meta: Option<String>,
    rows: Vec<EvidenceRow>,
}

#[derive(Template)]
#[template(path = "partials/inspect_notice.html")]
struct NoticeFragment {
    name: &'static str,
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

/// What a share key opens.
enum Shared {
    Found { input: String, html: String },
    Missing,
    Broken,
}

async fn show(
    State(deps): State<InspectDeps>,
    tool: Result<Path<String>, PathRejection>,
    uri: Uri,
) -> Response {
    let Some(tool) = tool.ok().and_then(|Path(name)| Tool::parse(&name)) else {
        return not_found().await;
    };
    let mut query = Query::<HashMap<String, String>>::try_from_uri(&uri)
        .map(|Query(q)| q)
        .unwrap_or_default();
    // A blank `ip` asks for the visitor's own address (the guides' "Leave blank for your IP").
    let own_ip = tool == Tool::Ip && query.get(tool.field()).is_some_and(|v| v.is_empty());
    let mut prefill = query.remove(tool.field()).filter(|v| !v.is_empty());
    let (mut status, mut result, mut notice) = (StatusCode::OK, None, None);
    if let Some(key) = query.remove("r") {
        match shared_result(&deps, tool, &key).await {
            Shared::Found { input, html } => {
                prefill = Some(input).filter(|v| !v.is_empty());
                result = Some(html);
            }
            Shared::Missing => {
                status = StatusCode::NOT_FOUND;
                prefill = None;
                notice = Some(EXPIRED);
            }
            Shared::Broken => return server_error(),
        }
    }
    let name = tool.as_str();
    let slot = match tool {
        Tool::Dns => (DnsBuilder).render().ok(),
        _ => None,
    };
    let Ok(tabs) = (TabsFragment {
        tabs: tabs(tool, prefill.as_deref()),
    })
    .render() else {
        return server_error();
    };
    let suggest = suggest(tool);
    let shell = ShellView {
        tool: name,
        field: tool.field(),
        name: tool.name(),
        tagline: tool.tagline(),
        query: QueryInputView {
            action: format!("/inspect/{name}"),
            name: tool.field().to_string(),
            label: tool.label().to_string(),
            placeholder: tool.placeholder().to_string(),
            value: prefill.clone(),
            submit: "Inspect".to_string(),
        },
        stream_url: format!("/inspect/{name}/run"),
        autostart: (prefill.is_some() || own_ip) && result.is_none(),
        tabs,
        examples: tool
            .examples()
            .iter()
            .map(|text| ExampleView {
                href: format!(
                    "/inspect/{name}?{}={}",
                    tool.field(),
                    utf8_percent_encode(text, LOCATION_SET)
                ),
                text,
            })
            .collect(),
        slot,
        result,
        notice,
        help: help(tool, suggest.as_ref()),
        suggest,
    };
    page(
        status,
        "no-cache",
        &InspectorPage {
            frame: Frame::new(
                &format!("{} - netray", tool.name()),
                "Run one check live and read the evidence as it arrives.",
                &format!("/inspect/{name}"),
            ),
            shell,
        },
    )
}

/// The run a share key names: a stored run of this tool, or for DNS a result of the API's cache.
async fn shared_result(deps: &InspectDeps, tool: Tool, key: &str) -> Shared {
    if !InspectJobs::is_share_key(key) {
        return Shared::Missing;
    }
    if let Some(run) = deps.shared.get(key).await.filter(|run| run.tool == tool) {
        return match shared_html(&run, key) {
            Some(html) => Shared::Found {
                input: run.input.clone(),
                html,
            },
            None => Shared::Broken,
        };
    }
    let cached = match (tool, &deps.dns_results) {
        (Tool::Dns, Some(cache)) => cache.result(key).await,
        _ => None,
    };
    let Some(cached) = cached else {
        return Shared::Missing;
    };
    // Rule M: a part is never empty; an API result without a block says so.
    let blocks = if cached.blocks.is_empty() {
        Err(NO_API_EVIDENCE.to_string())
    } else {
        Ok(cached.blocks)
    };
    let band = (BandFragment {
        band: None,
        text: NO_API_BAND.to_string(),
    })
    .render()
    .ok();
    let html = band.and_then(|band| {
        let title = shared_run_title(None);
        let export = export_html(
            Tool::Dns,
            &cached.query,
            &title,
            None,
            blocks.as_ref().map_or(&[][..], Vec::as_slice),
            &export_errors(
                std::iter::once((cached.mode.as_str(), Some(&blocks))),
                None::<&Result<(), String>>,
            ),
            true,
        )?;
        finished_html(
            title,
            cached.query.clone(),
            backlink(Tool::Dns, &cached.query),
            None,
            vec![(
                cached.mode.clone(),
                part_html(0, &cached.mode, &blocks, &[]),
            )],
            BandSlot::Done(band),
            (share_html(Tool::Dns, Some(key.to_string()))?, export, None),
        )
    });
    match html {
        Some(html) => Shared::Found {
            input: cached.query,
            html,
        },
        None => Shared::Broken,
    }
}

/// The autocomplete tables: the DNS crate's own record types, flags and server names.
fn suggest(tool: Tool) -> Option<SuggestView> {
    if tool != Tool::Dns {
        return None;
    }
    let (types, flags, servers) = InspectJobs::dns_tables();
    Some(SuggestView {
        name: "dns",
        types,
        flags,
        servers,
    })
}

/// The five tabs; the target of `input` rides along where the other inspector takes it.
fn tabs(current: Tool, input: Option<&str>) -> Vec<TabView> {
    let host = input.and_then(|i| target_host(current, i));
    let address = input.and_then(|i| target_address(current, i));
    let enc = |v: &str| utf8_percent_encode(v, LOCATION_SET).to_string();
    let carried = |key: &str, tool: &str| -> Option<String> {
        let value = match (tool, &host, &address) {
            ("dns", Some(h), _) => h.clone(),
            ("tls", Some(h), _) if is_ldh_host(h) => format!("{h}:443"),
            ("http", Some(h), _) if is_ldh_host(h) => format!("https://{h}/"),
            ("email", Some(h), _) => registrable_domain(h)?,
            ("ip", _, Some(a)) => a.to_string(),
            _ => return None,
        };
        Some(format!("/inspect/{tool}?{key}={}", enc(&value)))
    };
    [
        ("DNS", "dns", "q"),
        ("TLS", "tls", "h"),
        ("HTTP", "http", "url"),
        ("EMAIL", "email", "d"),
        ("IP", "ip", "ip"),
    ]
    .into_iter()
    .map(|(label, tool, key)| {
        let is_current = tool == current.as_str();
        let href = if is_current { None } else { carried(key, tool) };
        TabView {
            label,
            href: href.unwrap_or_else(|| format!("/inspect/{tool}")),
            current: is_current,
        }
    })
    .collect()
}

/// The back-link target for an input: the `/{domain}` path and the domain, when it has one.
fn backlink(tool: Tool, input: &str) -> Option<(String, String)> {
    let domain = registrable_domain(&target_host(tool, input)?)?;
    let href = format!("/{}", utf8_percent_encode(&domain, LOCATION_SET));
    Some((href, domain))
}

type Events = BoxStream<'static, Result<Event, Infallible>>;

fn sse(status: StatusCode, events: Vec<Event>) -> Response {
    let stream: Events = futures::stream::iter(events.into_iter().map(Ok)).boxed();
    (status, Sse::new(stream)).into_response()
}

fn too_many_requests(tool: Tool, retry_after: Duration) -> Response {
    let secs = retry_after.as_secs() + u64::from(retry_after.subsec_nanos() > 0);
    let mut response = notice(
        tool,
        StatusCode::TOO_MANY_REQUESTS,
        format!("too many requests; try again in {secs} s"),
    );
    response.headers_mut().insert(RETRY_AFTER, secs.into());
    response
}

fn notice(tool: Tool, status: StatusCode, message: String) -> Response {
    let name = tool.name();
    match (NoticeFragment { name, message }).render() {
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
    let client = client_ip(&deps.extractor, &parts.headers, peer);
    let req = Request::from_parts(parts, body);
    let mut form = match Form::<HashMap<String, String>>::from_request(req, &()).await {
        Ok(Form(form)) => form,
        Err(rejection) => return rejection.into_response(),
    };
    let input = form.remove(tool.field()).unwrap_or_default();

    let Ok(permit) = deps.slots[tool.index()].clone().try_acquire_owned() else {
        return notice(
            tool,
            StatusCode::SERVICE_UNAVAILABLE,
            "busy, try again in a moment".to_string(),
        );
    };
    if let Err(retry_after) = deps.jobs.admit(tool, client, &input) {
        return too_many_requests(tool, retry_after);
    }
    let share_key = (!(tool == Tool::Ip && input.trim().is_empty())).then(InspectJobs::share_key);
    let deadline = Instant::now() + Duration::from_millis(deps.config.limits(tool).deadline_ms);
    let link = backlink(tool, &input);
    let grade = match (&deps.history, &link) {
        (Some(history), Some((_, domain))) => {
            // The grade is decoration: a slow store costs it, never the job's budget.
            let bound = deadline.min(Instant::now() + GRADE_READ_BUDGET);
            match timeout_at(bound, history.head(domain)).await {
                Ok(Ok(head)) => head.and_then(|head| letter(head.grade)),
                Ok(Err(e)) => {
                    tracing::warn!(domain = %domain, error = %e, "history lookup failed");
                    None
                }
                Err(_) => {
                    tracing::warn!(domain = %domain, "history lookup timed out");
                    None
                }
            }
        }
        _ => None,
    };
    let target = target_heading(tool, &input, client);
    let tabs = TabsFragment {
        tabs: tabs(tool, Some(input.as_str()).filter(|i| !i.trim().is_empty())),
    }
    .render()
    .ok();
    let posted = input.clone();
    let mut job = match deps.jobs.start(JobRequest {
        tool,
        input,
        client,
        deadline: deadline.into_std(),
        share_key: share_key.clone(),
    }) {
        Ok(job) => job,
        Err(message) => return notice(tool, StatusCode::OK, message),
    };
    if let Some(gate) = job.gate.take() {
        match timeout_at(deadline, gate).await {
            Ok(Ok(())) => {}
            Ok(Err(Refusal::RateLimited(retry_after))) => {
                return too_many_requests(tool, retry_after);
            }
            Ok(Err(Refusal::Invalid(message))) => return notice(tool, StatusCode::OK, message),
            Err(_) => return notice(tool, StatusCode::OK, "timed out".to_string()),
        }
    }
    match stream_job(
        job,
        deps.jobs.clone(),
        StartHead {
            tool,
            input: posted,
            target,
            link,
            grade,
            tabs,
            share_key,
        },
        permit,
        deadline,
        deps.shared.clone(),
    ) {
        Some(events) => (StatusCode::OK, Sse::new(events)).into_response(),
        None => server_error(),
    }
}

type PartOutput = (usize, Result<Vec<Block>, String>);

type PartResult = Result<Vec<Block>, String>;
type BandResult = Result<Vec<CheckResult>, String>;

struct Run {
    _permit: OwnedSemaphorePermit,
    jobs: Arc<dyn InspectorJobs>,
    badges: Vec<(IpAddr, Vec<Badge>)>,
    pending: FuturesUnordered<BoxFuture<'static, PartOutput>>,
    waiting: BTreeSet<usize>,
    queue: VecDeque<Event>,
    labels: Vec<String>,
    deadline: Instant,
    blocks: Vec<Block>,
    band: Option<(Protocol, BandChecks)>,
    results: Vec<Option<PartResult>>,
    band_out: Option<(Protocol, BandResult)>,
    shared: SharedRuns,
    share_key: Option<String>,
    tool: Tool,
    input: String,
    target: String,
    link: Option<(String, String)>,
    grade: Option<&'static str>,
    started: i64,
    exported: bool,
    ended: bool,
}

impl Run {
    /// The finished run as data, for its share key.
    fn finished(&mut self) -> SharedRun {
        let parts = self
            .labels
            .iter()
            .cloned()
            .zip(
                self.results
                    .iter_mut()
                    .map(|r| r.take().unwrap_or_else(|| Err("timed out".to_string()))),
            )
            .collect();
        SharedRun {
            tool: self.tool,
            input: std::mem::take(&mut self.input),
            target: self.target.clone(),
            started: self.started,
            link: self.link.clone(),
            grade: self.grade,
            parts,
            band: self.band_out.take(),
            badges: std::mem::take(&mut self.badges),
        }
    }
}

/// A part's section: its blocks or its error text.
fn part_html(
    n: usize,
    label: &str,
    result: &PartResult,
    badges: &[(IpAddr, Vec<Badge>)],
) -> Option<String> {
    let decorate = links::decorate(badges);
    let (html, text) = match result {
        Ok(blocks) => {
            let mut html = String::new();
            for block in blocks {
                html.push_str(
                    &components::evidence_block(&block_view_with(block, &decorate)).ok()?,
                );
            }
            (Some(html), String::new())
        }
        Err(text) => (None, text.clone()),
    };
    let label = label.to_string();
    (PartFragment {
        n,
        label,
        html,
        text,
    })
    .render()
    .ok()
}

fn part_event(
    n: usize,
    label: &str,
    result: &PartResult,
    badges: &[(IpAddr, Vec<Badge>)],
) -> Option<Event> {
    let html = part_html(n, label, result, badges)?;
    Some(fragment(&format!("inspect-part-{n}"), &html))
}

/// The band region: the plate and the check list, or the error text.
fn band_region_html(protocol: Protocol, blocks: &[Block], result: &BandResult) -> Option<String> {
    let (band, text) = match result
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|checks| band_view(protocol, checks, blocks))
    {
        Ok(band) => (Some(band), String::new()),
        Err(text) => (None, text),
    };
    (BandFragment { band, text }).render().ok()
}

fn band_event(protocol: Protocol, blocks: &[Block], result: &BandResult) -> Option<Event> {
    let html = band_region_html(protocol, blocks, result)?;
    Some(fragment(BAND_ID, &html))
}

fn share_html(tool: Tool, key: Option<String>) -> Option<String> {
    (ShareFragment {
        tool: tool.as_str(),
        key,
    })
    .render()
    .ok()
}

/// The export slot: disabled while the run streams, with the three exports once it is `ready`.
fn export_html(
    tool: Tool,
    target: &str,
    title: &RunLineView,
    band: Option<(Protocol, &[CheckResult])>,
    blocks: &[Block],
    errors: &[(String, String)],
    ready: bool,
) -> Option<String> {
    let title: Vec<String> = title.parts.iter().map(|p| p.to_uppercase()).collect();
    let band = band.map(|(p, c)| ([p], c));
    let input = ExportInput {
        target,
        title: &title,
        band: band.as_ref().map(|(p, c)| (&p[..], *c)),
        blocks,
        errors,
    };
    let exports = ready.then(|| {
        (
            export::markdown(&input),
            export::csv(&input),
            export::json(&input),
        )
    });
    super::export_slot(
        format!("netray-{}-{}", tool.as_str(), export::slug(target)),
        exports,
    )
}

/// The sections of a run that ended without a result, for the exports (Rule M): a part with its
/// error text, or "timed out" when it never answered, and the band as "checks".
fn export_errors<'a, T: 'a, B: 'a>(
    parts: impl Iterator<Item = (&'a str, Option<&'a Result<T, String>>)>,
    band: Option<&'a Result<B, String>>,
) -> Vec<(String, String)> {
    let mut errors: Vec<(String, String)> = parts
        .filter_map(|(label, result)| match result {
            Some(Ok(_)) => None,
            Some(Err(e)) => Some((label.to_string(), e.clone())),
            None => Some((label.to_string(), "timed out".to_string())),
        })
        .collect();
    if let Some(Err(e)) = band {
        errors.push(("checks".to_string(), e.clone()));
    }
    errors
}

/// A finished run's result region, in the markup the stream leaves behind.
fn finished_html(
    title: RunLineView,
    target: String,
    link: Option<(String, String)>,
    grade: Option<&'static str>,
    parts: Vec<(String, Option<String>)>,
    band: BandSlot,
    (share, export, related): (String, String, Option<String>),
) -> Option<String> {
    let mut rendered = Vec::with_capacity(parts.len());
    for (n, (label, html)) in parts.into_iter().enumerate() {
        rendered.push((n, label, Some(html?)));
    }
    StartFragment {
        title,
        target,
        link,
        grade,
        parts: rendered,
        band,
        share,
        export,
        related,
    }
    .render()
    .ok()
}

/// A stored run's result region.
fn shared_html(run: &SharedRun, key: &str) -> Option<String> {
    let blocks: Vec<Block> = run
        .parts
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok())
        .flatten()
        .cloned()
        .collect();
    let band = match &run.band {
        Some((protocol, result)) => BandSlot::Done(band_region_html(*protocol, &blocks, result)?),
        None => BandSlot::Absent,
    };
    let parts = run
        .parts
        .iter()
        .enumerate()
        .map(|(n, (label, result))| (label.clone(), part_html(n, label, result, &run.badges)))
        .collect();
    let export = export_html(
        run.tool,
        &run.target,
        &live_run_title(run.started),
        run.band
            .as_ref()
            .and_then(|(p, r)| r.as_ref().ok().map(|c| (*p, c.as_slice()))),
        &blocks,
        &export_errors(
            run.parts.iter().map(|(label, r)| (label.as_str(), Some(r))),
            run.band.as_ref().map(|(_, r)| r),
        ),
        true,
    )?;
    finished_html(
        shared_run_title(Some(run.started)),
        run.target.clone(),
        run.link.clone(),
        run.grade,
        parts,
        band,
        (
            share_html(run.tool, Some(key.to_string()))?,
            export,
            related(run.tool, &run.input),
        ),
    )
}

/// The DNS inspector query of the records that belong beside a TLS run of a host: its CAA.
fn related(tool: Tool, input: &str) -> Option<String> {
    if tool != Tool::Tls {
        return None;
    }
    target_host(tool, input).map(|host| links::dns_query_href(&format!("{host} CAA")))
}

fn band_view(
    protocol: Protocol,
    checks: &[CheckResult],
    blocks: &[Block],
) -> Result<BandView, String> {
    let spectrum = band_html(protocol, checks)?;
    let specs = netray_engine::catalogue::catalogue().for_protocol(protocol);
    let specs = specs.entries();
    // A line's name carries its status tone, as board G colours a warning line's label.
    let lines: Vec<(String, String, &'static str)> = specs
        .iter()
        .map(|spec| {
            let status = checks.iter().find(|c| c.id == spec.id).map(|c| c.status);
            let tone = match status {
                Some(Status::Warn) => "warn",
                Some(Status::Fail) => "fail",
                _ => "none",
            };
            (
                spec.short.clone(),
                line_code(&spec.id).unwrap_or_default(),
                tone,
            )
        })
        .collect();
    let caption = match (lines.first(), lines.last()) {
        (Some((_, first, _)), Some((_, last, _))) => {
            let (tag, from) = first.split_once('\u{b7}').unwrap_or((first, ""));
            let to = last.split_once('\u{b7}').map_or("", |(_, n)| n);
            if lines.len() == 1 {
                format!(
                    "The {tag} band of the spectrum, zoomed: the same line as position {first} on the result page."
                )
            } else {
                format!(
                    "The {tag} band of the spectrum, zoomed: the same {} lines as positions {tag}\u{b7}{from}\u{2013}{to} on the result page.",
                    lines.len()
                )
            }
        }
        _ => String::new(),
    };
    let status_of = |spec: &netray_engine::catalogue::CheckSpec| {
        checks.iter().find(|c| c.id == spec.id).map(|c| c.status)
    };
    let count = |want: Status| specs.iter().filter(|s| status_of(s) == Some(want)).count();
    let (fails, warns, passes) = (
        count(Status::Fail),
        count(Status::Warn),
        count(Status::Pass),
    );
    let (heading, meta) = match fails + warns {
        0 if passes == specs.len() && passes == 1 => ("The line passes".to_string(), None),
        0 if passes == specs.len() => (format!("All {passes} lines pass"), None),
        0 => (
            "No line needs attention".to_string(),
            Some(format!("{} not measured", specs.len() - passes)),
        ),
        1 => (
            "1 line needs attention".to_string(),
            Some(format!("{fails} fail \u{b7} {warns} warn")),
        ),
        n => (
            format!("{n} lines need attention"),
            Some(format!("{fails} fail \u{b7} {warns} warn")),
        ),
    };
    Ok(BandView {
        spectrum,
        lines,
        caption,
        heading,
        meta,
        rows: live_rows(checks, blocks, protocol),
    })
}

fn band_html(protocol: Protocol, checks: &[CheckResult]) -> Result<String, String> {
    let input = RunInput {
        statuses: checks.iter().map(|c| (c.id.clone(), c.status)).collect(),
        sections: BTreeMap::new(),
        profile: Profiles::current(),
    };
    let catalogue = netray_engine::catalogue::catalogue().for_protocol(protocol);
    let l = layout(&input, &[], &catalogue, BAND_WIDTH)
        .map_err(|_| "the band could not be laid out".to_string())?;
    let label = format!("{} checks", protocol.as_str().to_ascii_uppercase());
    components::spectrum(&SpectrumView::from_layout(&l, &label, 0)).map_err(|e| e.to_string())
}

/// What the start fragment's head shows: the target, the back link and the stored grade.
struct StartHead {
    tool: Tool,
    input: String,
    target: String,
    link: Option<(String, String)>,
    grade: Option<&'static str>,
    tabs: Option<String>,
    share_key: Option<String>,
}

/// The letter of a stored grade; `None` for a grade that is not a letter.
fn letter(grade: Grade) -> Option<&'static str> {
    match grade {
        Grade::Incomplete => None,
        grade => Some(grade_letter(grade)),
    }
}

fn stream_job(
    job: Job,
    jobs: Arc<dyn InspectorJobs>,
    head: StartHead,
    permit: OwnedSemaphorePermit,
    deadline: Instant,
    shared: SharedRuns,
) -> Option<Events> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let labels: Vec<String> = job.parts.iter().map(|part| part.label.clone()).collect();
    let start = StartFragment {
        title: live_run_title(now),
        target: head.target.clone(),
        link: head.link.clone(),
        grade: head.grade,
        parts: job
            .parts
            .iter()
            .enumerate()
            .map(|(n, part)| (n, part.label.clone(), None))
            .collect(),
        band: if job.band.is_some() {
            BandSlot::Pending
        } else {
            BandSlot::Absent
        },
        share: share_html(head.tool, None)?,
        export: export_html(
            head.tool,
            &head.target,
            &live_run_title(now),
            None,
            &[],
            &[],
            false,
        )?,
        related: related(head.tool, &head.input),
    }
    .render()
    .ok()?;
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
        jobs,
        badges: Vec::new(),
        waiting: (0..pending.len()).collect(),
        pending,
        queue: [
            Some(fragment(RESULT_ID, &start)),
            head.tabs.map(|t| fragment(TABS_ID, &t)),
        ]
        .into_iter()
        .flatten()
        .collect(),
        results: (0..labels.len()).map(|_| None).collect(),
        labels,
        deadline,
        blocks: Vec::new(),
        band: job.band,
        band_out: None,
        shared,
        share_key: head.share_key,
        tool: head.tool,
        input: head.input,
        target: head.target,
        link: head.link,
        grade: head.grade,
        started: now,
        exported: false,
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
                if let Some((protocol, checks)) = run.band.take() {
                    let result = timeout_at(run.deadline, checks)
                        .await
                        .unwrap_or_else(|_| Err("timed out".to_string()));
                    run.queue.push_back(
                        band_event(protocol, &run.blocks, &result)
                            .unwrap_or_else(|| fragment(BAND_ID, "render failed")),
                    );
                    run.band_out = Some((protocol, result));
                    continue;
                }
                if !run.exported {
                    run.exported = true;
                    let blocks: Vec<Block> = run
                        .results
                        .iter()
                        .filter_map(|r| r.as_ref().and_then(|r| r.as_ref().ok()))
                        .flatten()
                        .cloned()
                        .collect();
                    let band = run
                        .band_out
                        .as_ref()
                        .and_then(|(p, r)| r.as_ref().ok().map(|c| (*p, c.as_slice())));
                    let export = export_html(
                        run.tool,
                        &run.target,
                        &live_run_title(run.started),
                        band,
                        &blocks,
                        &export_errors(
                            run.labels
                                .iter()
                                .zip(&run.results)
                                .map(|(label, r)| (label.as_str(), r.as_ref())),
                            run.band_out.as_ref().map(|(_, r)| r),
                        ),
                        true,
                    );
                    if let Some(key) = run.share_key.take() {
                        let finished = run.finished();
                        run.shared.insert(key.clone(), finished).await;
                        if let Some(html) = share_html(run.tool, Some(key)) {
                            run.queue.push_back(fragment(SHARE_ID, &html));
                        }
                    }
                    run.queue.push_back(fragment(
                        EXPORT_ID,
                        export.as_deref().unwrap_or("render failed"),
                    ));
                    continue;
                }
                run.ended = true;
                return Some((Ok(end()), run));
            }
            match timeout_at(run.deadline, run.pending.next()).await {
                Ok(Some((n, result))) => {
                    run.waiting.remove(&n);
                    if let Ok(blocks) = &result {
                        run.blocks.extend(blocks.iter().cloned());
                        let mut addrs: Vec<IpAddr> = Vec::new();
                        for addr in blocks.iter().flat_map(links::addresses) {
                            if !addrs.contains(&addr) && !run.badges.iter().any(|(a, _)| *a == addr)
                            {
                                addrs.push(addr);
                            }
                        }
                        // The IP inspector's own lookup is its evidence: no second lookup.
                        if run.tool != Tool::Ip && !addrs.is_empty() {
                            let found = timeout_at(run.deadline, run.jobs.enrich(addrs))
                                .await
                                .unwrap_or_default();
                            run.badges.extend(found);
                        }
                    }
                    let event = part_event(n, &run.labels[n], &result, &run.badges)
                        .unwrap_or_else(|| fragment(&format!("inspect-part-{n}"), "render failed"));
                    run.results[n] = Some(result);
                    return Some((Ok(event), run));
                }
                Ok(None) => run.pending.clear(),
                Err(_) => {
                    run.pending.clear();
                    let late: Vec<usize> = std::mem::take(&mut run.waiting).into_iter().collect();
                    for n in late {
                        let timed_out = Err("timed out".to_string());
                        if let Some(event) = part_event(n, &run.labels[n], &timed_out, &[]) {
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
        Tool::Email => email_input(input).0,
        Tool::Tls => tls_host(input)?,
        Tool::Http => http_host(input)?,
        Tool::Ip => return None,
    };
    let host = raw.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.parse::<IpAddr>().is_ok() {
        return None;
    }
    netray_common::domain::validate(&host).ok()
}

/// The address a posted or prefilled input names, when it names one rather than a host.
fn target_address(tool: Tool, input: &str) -> Option<IpAddr> {
    let raw = match tool {
        Tool::Dns => input.split_whitespace().next()?.to_string(),
        Tool::Tls => tls_host(input)?,
        Tool::Http => match http_url(input)?.host()? {
            Host::Ipv4(a) => return Some(a.into()),
            Host::Ipv6(a) => return Some(a.into()),
            Host::Domain(_) => return None,
        },
        Tool::Email => return None,
        Tool::Ip => input.trim().to_string(),
    };
    raw.parse().ok()
}

/// The heading of a run: the target as the job reads it. An empty IP input names the caller, an
/// HTTP input its URL with `https://` assumed, a TLS input its host with 443 when it names no port.
/// The email inspector's reading of its input, shared by the page and the job: the domain (in
/// its UTS #46 form when it maps, as the result page reads a domain), then the DKIM selectors;
/// tokens are separated by spaces or commas, empty ones dropped.
pub fn email_input(input: &str) -> (String, Vec<String>) {
    let mut tokens = input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty());
    let raw = tokens.next().unwrap_or_default();
    let domain = netray_common::domain::validate(raw).unwrap_or_else(|_| raw.to_string());
    (domain, tokens.map(str::to_string).collect())
}

fn target_heading(tool: Tool, input: &str, client: IpAddr) -> String {
    let trimmed = input.trim();
    match tool {
        Tool::Ip if trimmed.is_empty() => client.to_string(),
        Tool::Http => http_url(trimmed).map_or_else(|| trimmed.to_string(), |url| url.to_string()),
        Tool::Tls => tls_heading(input),
        Tool::Email if !email_input(input).0.is_empty() => email_input(input).0,
        _ => trimmed.to_string(),
    }
}

fn tls_heading(input: &str) -> String {
    let decoded = percent_decode_str(input)
        .decode_utf8()
        .map_or_else(|_| input.trim().to_string(), |d| d.trim().to_string());
    if decoded.parse::<Ipv6Addr>().is_ok() {
        return format!("[{decoded}]:443");
    }
    if decoded.starts_with('[') {
        return if decoded.ends_with(']') {
            format!("{decoded}:443")
        } else {
            decoded
        };
    }
    match port_split(&decoded) {
        Some((host, ports)) => format!("{}:{ports}", host.trim_end_matches('.')),
        None => format!("{}:443", decoded.trim_end_matches('.')),
    }
}

/// `host:ports` split at the last colon when a port list follows it.
fn port_split(input: &str) -> Option<(&str, &str)> {
    let (host, ports) = input.rsplit_once(':')?;
    is_ports(ports).then_some((host, ports))
}

fn is_ports(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ',')
}

/// The TLS inspector's order: percent-decode, then bracket, then `host:port`. A bracketed
/// input gives the address inside the brackets.
fn tls_host(input: &str) -> Option<String> {
    let decoded = percent_decode_str(input).decode_utf8().ok()?;
    let input = decoded.trim();
    if let Some(rest) = input.strip_prefix('[') {
        let (addr, after) = rest.split_once(']')?;
        let ports_ok = after.is_empty() || after.strip_prefix(':').is_some_and(is_ports);
        return ports_ok.then(|| addr.to_string());
    }
    if input.contains(':') {
        port_split(input).map(|(host, _)| host.to_string())
    } else {
        Some(input.to_string())
    }
}

/// The HTTP inspector's reading of the URL: `https://` assumed when it names no scheme.
fn http_url(input: &str) -> Option<Url> {
    let input = input.trim();
    if input.contains("://") {
        Url::parse(input)
    } else {
        Url::parse(&format!("https://{input}"))
    }
    .ok()
}

/// The HTTP inspector's host: a domain host only.
fn http_host(input: &str) -> Option<String> {
    match http_url(input)?.host()? {
        Host::Domain(domain) => Some(domain.to_string()),
        _ => None,
    }
}

/// A host the TLS and HTTP inspectors take: LDH labels, at least one dot.
fn is_ldh_host(host: &str) -> bool {
    host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// The registrable domain of a host name (Public Suffix List); `None` for a public suffix.
pub fn registrable_domain(host: &str) -> Option<String> {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let domain = psl::domain_str(&host)?;
    netray_common::domain::validate(domain).ok()
}
