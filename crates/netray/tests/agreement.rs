//! One run, read through every view main has, says the same thing (spec v2-cutover, Phase 5,
//! requirements 20 and 21; C1/C4 and the half of C2/C5 that main can answer).
//!
//! The views are composed as `serve` composes them: netray-web's host router over the apex's
//! routes (`netray_web::host::apex`, what `host::build` composes), wrapped in `host::layers`,
//! asked with `Host: netray.info`, configured from a `netray.toml` of production shape. The
//! modules are stubs that answer a scripted section; the store is a temporary file. A run is
//! started through netray-web's admission and read back once it is recorded.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
use netray_web::admission::{Admit, ApexState, RunEvent, THROTTLE, Trigger, admit};
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use netray_web::host::HostRouters;
use tower::ServiceExt;

const DOMAIN: &str = "example.com";

/// What a stub module does on its next run.
#[derive(Clone)]
enum Plan {
    Answer(SectionOutcome),
    /// Never answers: the section's window closes on it.
    Hang,
}

struct Script {
    protocol: Protocol,
    plan: Arc<Mutex<Plan>>,
}

impl Module for Script {
    fn protocol(&self) -> Protocol {
        self.protocol
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        let plan = self.plan.lock().unwrap().clone();
        Box::pin(async move {
            match plan {
                Plan::Answer(outcome) => outcome,
                Plan::Hang => std::future::pending().await,
            }
        })
    }
}

fn measured(checks: &[(&str, Status)]) -> Plan {
    Plan::Answer(SectionOutcome::Measured {
        checks: checks
            .iter()
            .map(|(id, status)| CheckResult {
                id: CheckId::parse(id).unwrap(),
                status: *status,
                findings: vec![],
                evidence: vec![],
            })
            .collect(),
        evidence: vec![],
    })
}

fn not_applicable(reason: &str) -> Plan {
    Plan::Answer(SectionOutcome::NotApplicable {
        reason: reason.to_string(),
    })
}

struct World {
    app: Router,
    state: ApexState,
    store: Arc<HistoryStore>,
    dns: Arc<Mutex<Plan>>,
    email: Arc<Mutex<Plan>>,
    _dir: tempfile::TempDir,
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/netray.production.toml")
}

/// The production fixture as `netray serve` reads it: the apex's config, the section windows
/// (`[modules.<m>] deadline_ms`) and the resolve budget (`[engine] resolve_timeout_ms`).
fn production() -> (ApexConfig, impl Fn(Protocol) -> Duration, Duration) {
    let path = fixture();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_netray"))
        .args(["serve", "--check-config", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "the production fixture must load: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let raw: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let int = |table: &str, key: &str| -> u64 {
        raw[table][key]
            .as_integer()
            .unwrap_or_else(|| panic!("[{table}] {key}")) as u64
    };
    let admission = |key: &str| int("admission", key) as u32;
    let config = ApexConfig {
        trusted_proxies: raw["server"]["trusted_proxies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect(),
        limits: AdmissionLimits {
            client_budget_per_hour: admission("client_budget_per_hour"),
            domain_run_slots: admission("domain_run_slots"),
            rate_limit_per_ip_per_minute: admission("per_ip_per_minute"),
            rate_limit_per_ip_burst: admission("per_ip_burst"),
            rate_limit_global_per_minute: admission("global_per_minute"),
            rate_limit_global_burst: admission("global_burst"),
        },
        cache_enabled: raw["admission"]["cache_enabled"].as_bool().unwrap(),
        cache_window_seconds: int("admission", "cache_window_seconds"),
        ..ApexConfig::default()
    };
    let modules = raw["modules"].as_table().unwrap().clone();
    let window = move |p: Protocol| {
        Duration::from_millis(modules[p.as_str()]["deadline_ms"].as_integer().unwrap() as u64)
    };
    (
        config,
        window,
        Duration::from_millis(int("engine", "resolve_timeout_ms")),
    )
}

/// `serve`'s composition over stub DNS and email modules (email's window: 300 ms) and a store
/// on a temp file.
async fn world(dns: Plan, email: Plan) -> World {
    let (mut config, window, resolve_budget) = production();
    config.cache_enabled = false;
    config.limits.rate_limit_per_ip_per_minute = 1000;
    config.limits.rate_limit_per_ip_burst = 1000;
    config.api.per_ip_per_minute = 1000;
    config.api.per_ip_burst = 1000;
    let dir = tempfile::tempdir().unwrap();

    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let dns = Arc::new(Mutex::new(dns));
    let email = Arc::new(Mutex::new(email));
    let registry = Registry::new()
        .with(Box::new(Script {
            protocol: Protocol::Dns,
            plan: dns.clone(),
        }))
        .with(Box::new(Script {
            protocol: Protocol::Email,
            plan: email.clone(),
        }));
    let plan = netray_engine::section_plan(&registry, |p| {
        Some(match p {
            Protocol::Email => Duration::from_millis(300),
            p => window(p),
        })
    });
    let profile = netray_web::admission::load_scoring_profile(None).unwrap();
    let api_enabled = config.api.enabled;
    let mut state = ApexState::new(config, registry, plan, resolve_budget, profile);
    state.history_store = Some(store.clone());

    let app = netray_web::host::layers(
        netray_web::host::router(HostRouters {
            apex: "netray.info".to_string(),
            probes: netray_web::host::probes(),
            apex_router: netray_web::host::apex::routes(state.clone()),
            dns: None,
            tls: None,
            http: None,
            ip: None,
        }),
        api_enabled,
    )
    .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    World {
        app,
        state,
        store,
        dns,
        email,
        _dir: dir,
    }
}

/// Past the per-domain throttle; `pause` needs this test's current_thread runtime.
async fn pass_throttle() {
    tokio::time::pause();
    tokio::time::advance(THROTTLE).await;
    tokio::time::resume();
}

impl World {
    /// Starts a visit run for the domain and waits for its record.
    async fn run(&self) -> Option<Recorded> {
        pass_throttle().await;
        let client = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let Admit::Admitted(mut stream) = admit(&self.state, DOMAIN, client, Trigger::Visit, false)
        else {
            panic!("the run was not admitted");
        };
        while let Some(event) = stream.next().await {
            if let RunEvent::Recorded(recorded) = event {
                return recorded;
            }
        }
        panic!("the run ended without a record");
    }

    async fn get(&self, uri: &str) -> Answer {
        self.send(
            Request::builder()
                .uri(uri)
                .header("host", "netray.info")
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// POSTs `body` to `uri` with `content_type`, asking for `accept`.
    async fn post(&self, uri: &str, content_type: &str, accept: &str, body: &str) -> Answer {
        self.send(
            Request::post(uri)
                .header("host", "netray.info")
                .header("content-type", content_type)
                .header("accept", accept)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    async fn send(&self, request: Request<Body>) -> Answer {
        let resp = self.app.clone().oneshot(request).await.unwrap();
        let status = resp.status();
        let body = to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        Answer { status, body }
    }

    async fn head_entry(&self) -> Entry {
        let head = self
            .store
            .head(DOMAIN)
            .await
            .unwrap()
            .expect("a stored entry");
        self.store.get(&head.id).await.unwrap().expect("the entry")
    }
}

#[derive(PartialEq, Eq)]
struct Answer {
    status: StatusCode,
    body: Vec<u8>,
}

impl Answer {
    fn text(&self) -> String {
        String::from_utf8(self.body.clone()).expect("a text body")
    }
}

/// The grade as every view spells it, written out here.
fn letter(grade: Grade) -> &'static str {
    match grade {
        Grade::APlus => "A+",
        Grade::A => "A",
        Grade::B => "B",
        Grade::C => "C",
        Grade::D => "D",
        Grade::F => "F",
        Grade::Incomplete => "incomplete",
    }
}

fn status_word(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::NotApplicable => "not_applicable",
        Status::NotTested => "not_tested",
        Status::Unmeasured => "unmeasured",
    }
}

/// The badge's value text: the last `<text>` of the SVG.
fn badge_letter(svg: &str) -> String {
    let end = svg.rfind("</text>").expect("a text element");
    let start = svg[..end].rfind('>').expect("an opening tag") + 1;
    svg[start..end].to_string()
}

/// The grade the entry page names in its `og:title` ("example.com · B").
fn page_letter(html: &str) -> String {
    let marker = r#"property="og:title" content=""#;
    let from = html.find(marker).expect("og:title") + marker.len();
    let content = &html[from..from + html[from..].find('"').unwrap()];
    content
        .rsplit_once(" \u{00b7} ")
        .expect("domain and grade")
        .1
        .to_string()
}

/// The `(code, tone)` of every issue row of the entry page, sorted.
fn page_issues(html: &str) -> Vec<(String, String)> {
    let code = r#"<span class="check-row__code">"#;
    let tone = r#"<span class="check-row__tone">"#;
    let mut rows = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(code) {
        rest = &rest[i + code.len()..];
        let id = rest[..rest.find("</span>").unwrap()].to_string();
        let j = rest.find(tone).expect("a tone after the code") + tone.len();
        let t = rest[j..j + rest[j..].find("</span>").unwrap()].to_string();
        rows.push((id, t));
    }
    rows.sort();
    rows
}

/// The `N pass · N warn · …` line of the entry page.
fn page_counts(html: &str) -> String {
    let end = html.find(" not measured</p>").expect("the counts line");
    let start = html[..end].rfind("<p>").unwrap() + 3;
    format!("{} not measured", &html[start..end])
}

fn expected_counts(entry: &Entry) -> String {
    let n = |s: Status| entry.run.checks.iter().filter(|c| c.status == s).count();
    format!(
        "{} pass \u{00b7} {} warn \u{00b7} {} fail \u{00b7} {} not applicable \u{00b7} {} not tested \u{00b7} {} not measured",
        n(Status::Pass),
        n(Status::Warn),
        n(Status::Fail),
        n(Status::NotApplicable),
        n(Status::NotTested),
        n(Status::Unmeasured),
    )
}

/// What each view says about one stored entry, read through the composed router.
struct Views {
    badge: Answer,
    og_card: Answer,
    entry_page: Answer,
    entry_card: Answer,
    /// `GET /api/v1/domains/{domain}`.
    api: Answer,
}

async fn read_views(w: &World, entry: &Entry) -> Views {
    Views {
        badge: w.get(&format!("/badge/{DOMAIN}.svg")).await,
        og_card: w.get(&format!("/og/{DOMAIN}.png")).await,
        entry_page: w.get(&format!("/r/{}", entry.id)).await,
        entry_card: w.get(&format!("/r/{}/og.png", entry.id)).await,
        api: w.get(&format!("/api/v1/domains/{DOMAIN}")).await,
    }
}

/// The tone a page draws a status in, written out: warn and fail keep their word, pass too,
/// every other status is untested.
fn tone(status: Status) -> &'static str {
    match status {
        Status::Pass => "pass",
        Status::Warn => "warn",
        Status::Fail => "fail",
        Status::NotApplicable | Status::NotTested | Status::Unmeasured => "untested",
    }
}

/// `(title, tone)` of every check row of an evidence view, which names a check by its label and
/// title.
fn page_titles(html: &str) -> Vec<(String, String)> {
    let tone = r#"<span class="check-row__tone">"#;
    let title = r#"<h3 class="check-row__title">"#;
    let mut rows = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(tone) {
        rest = &rest[i + tone.len()..];
        let t = rest[..rest.find("</span>").unwrap()].to_string();
        let j = rest.find(title).expect("a title after the tone") + title.len();
        let name = rest[j..j + rest[j..].find("</h3>").unwrap()].to_string();
        rows.push((name, t));
    }
    rows
}

/// The part of `html` from the element with `id` to the next of `until`.
fn part<'a>(html: &'a str, id: &str, until: &str) -> &'a str {
    let from = html
        .find(&format!(r#"id="{id}""#))
        .unwrap_or_else(|| panic!("no element {id}"));
    let to = html[from..].find(until).map_or(html.len(), |n| from + n);
    &html[from..to]
}

/// The letter of the first `grade__letter` of `html`.
fn grade_view_letter(html: &str) -> String {
    let marker = r#"<span class="grade__letter">"#;
    let from = html.find(marker).expect("a grade letter") + marker.len();
    html[from..from + html[from..].find('<').unwrap()].to_string()
}

fn warn_fail(rows: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = rows
        .iter()
        .filter(|(_, t)| t == "warn" || t == "fail")
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `check id -> tone` of the run's checks.
fn run_tones(checks: impl IntoIterator<Item = (String, Status)>) -> BTreeMap<String, String> {
    checks
        .into_iter()
        .map(|(id, s)| (id, tone(s).to_string()))
        .collect()
}

/// `check id -> tone` of check rows `(id, tone)`; a check drawn twice must agree with itself.
fn row_tones(rows: &[(String, String)], view: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (id, t) in rows {
        if let Some(old) = map.insert(id.clone(), t.clone()) {
            assert_eq!(&old, t, "{view}: {id} drawn as {old} and {t}");
        }
    }
    map
}

/// `(check id, status word)` of an API run (`run.checks`).
fn api_checks(run: &serde_json::Value) -> Vec<(String, String)> {
    run["checks"]
        .as_array()
        .expect("run.checks")
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap().to_string(),
                c["status"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn status_of(word: &str) -> Status {
    [
        Status::Pass,
        Status::Warn,
        Status::Fail,
        Status::NotApplicable,
        Status::NotTested,
        Status::Unmeasured,
    ]
    .into_iter()
    .find(|s| status_word(*s) == word)
    .unwrap_or_else(|| panic!("unknown status {word}"))
}

/// C10: the result page, its open issues, every evidence view of the run's sections (latest and
/// by entry) and `GET /api/v1/domains/{domain}` (and `/diff` for the fixed state) against the
/// stored entry.
async fn result_page_rows(w: &World, entry: &Entry, state: &State, api: &Answer) {
    let name = state.name;
    let want_letter = letter(entry.grade);
    let wanted: Vec<(String, String)> = state
        .issues
        .iter()
        .map(|(id, s)| (id.to_string(), s.to_string()))
        .collect();
    let stored = run_tones(
        entry
            .run
            .checks
            .iter()
            .map(|c| (c.id.to_string(), c.status)),
    );

    // The result page: grade, every check row, the open issues.
    let result = w.get(&format!("/{DOMAIN}")).await;
    assert_eq!(result.status, StatusCode::OK, "{name}: result page");
    let page = result.text();
    let header = part(&page, "result-grade", "</div>\n</div>");
    assert!(
        header.contains(&format!(r#"data-grade="{want_letter}""#)),
        "{name}: result page data-grade: {header}"
    );
    assert_eq!(
        grade_view_letter(header),
        want_letter,
        "{name}: grade on the result page"
    );
    let rows = page_issues(&page);
    assert_eq!(
        row_tones(&rows, "result page"),
        stored,
        "{name}: status per check on the result page"
    );
    assert_eq!(
        warn_fail(&rows),
        wanted,
        "{name}: warn/fail set on the result page"
    );
    let issues = page_issues(part(&page, "issues", r#"<section id="evidence">"#));
    assert_eq!(
        warn_fail(&issues),
        wanted,
        "{name}: warn/fail set of the open issues"
    );
    assert_eq!(
        issues.len(),
        wanted.len(),
        "{name}: the open issues hold only the warn and fail checks: {issues:?}"
    );

    // Every evidence view of the run's sections, latest and by entry.
    let catalogue = netray_engine::catalogue();
    let mut evidence_issues = Vec::new();
    for section in &entry.run.sections {
        let p = section.protocol.as_str();
        for uri in [format!("/{DOMAIN}/{p}"), format!("/r/{}/{p}", entry.id)] {
            let view = w.get(&uri).await;
            assert_eq!(view.status, StatusCode::OK, "{name}: {uri}");
            let rows: Vec<(String, String)> = page_titles(&view.text())
                .into_iter()
                .map(|(title, t)| {
                    let id = catalogue
                        .entries()
                        .iter()
                        .find(|e| e.protocol == section.protocol && e.title == title)
                        .unwrap_or_else(|| panic!("{name}: {uri}: no check titled {title}"))
                        .id
                        .to_string();
                    (id, t)
                })
                .collect();
            let drawn = row_tones(&rows, &uri);
            for (id, t) in &drawn {
                let want = stored.get(id).map_or("untested", String::as_str);
                assert_eq!(t, want, "{name}: {id} on {uri}");
            }
            for id in stored.keys().filter(|id| id.starts_with(&format!("{p}."))) {
                assert!(drawn.contains_key(id), "{name}: {uri} lacks {id}");
            }
            evidence_issues.extend(warn_fail(&rows));
        }
    }
    evidence_issues.sort();
    evidence_issues.dedup();
    assert_eq!(
        evidence_issues, wanted,
        "{name}: warn/fail set on the evidence views"
    );

    // `GET /api/v1/domains/{domain}`.
    assert_eq!(api.status, StatusCode::OK, "{name}: api domain read");
    let json: serde_json::Value = serde_json::from_slice(&api.body).unwrap();
    assert_eq!(json["id"], entry.id.as_str(), "{name}: api entry id");
    assert_eq!(json["grade"], want_letter, "{name}: grade on the api");
    assert_eq!(
        json["run"]["grade"], want_letter,
        "{name}: run grade on the api"
    );
    let checks = api_checks(&json["run"]);
    assert_eq!(
        run_tones(checks.iter().map(|(id, s)| (id.clone(), status_of(s)))),
        stored,
        "{name}: status per check on the api"
    );
    let mut stored_words: Vec<(String, String)> = entry
        .run
        .checks
        .iter()
        .map(|c| (c.id.to_string(), status_word(c.status).to_string()))
        .collect();
    stored_words.sort();
    let mut api_words = checks.clone();
    api_words.sort();
    assert_eq!(api_words, stored_words, "{name}: statuses on the api");
    assert_eq!(
        warn_fail(&checks),
        wanted,
        "{name}: warn/fail set on the api"
    );

    // `/diff` names the check the page's diff block names as fixed.
    if state.fixed.is_some() {
        let diff = w.get(&format!("/api/v1/domains/{DOMAIN}/diff")).await;
        assert_eq!(diff.status, StatusCode::OK, "{name}: api diff");
        let json: serde_json::Value = serde_json::from_slice(&diff.body).unwrap();
        let fixed: Vec<&str> = json["diff"]["checks"]
            .as_array()
            .expect("diff.checks")
            .iter()
            .filter(|c| c["class"] == "fixed")
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            fixed,
            ["dns.dnssec"],
            "{name}: fixed checks on the api diff"
        );
        assert_eq!(
            json["diff"]["grade"]["to"], want_letter,
            "{name}: diff grade"
        );
    }
}

/// Server-sent events of `body` as `(event, id, html)`.
fn sse_events(body: &str) -> Vec<(String, String, String)> {
    let text = body.replace("\r\n", "\n");
    let mut events = Vec::new();
    for raw in text.split("\n\n").filter(|e| !e.trim().is_empty()) {
        let mut name = String::new();
        let mut data = String::new();
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push_str(v.trim_start());
            }
        }
        let j = serde_json::from_str::<serde_json::Value>(&data).unwrap_or_default();
        events.push((
            name,
            j["id"].as_str().unwrap_or_default().to_owned(),
            j["html"].as_str().unwrap_or_default().to_owned(),
        ));
    }
    events
}

fn last_fragment<'a>(events: &'a [(String, String, String)], id: &str) -> &'a str {
    events
        .iter()
        .rev()
        .find(|e| e.0 == "fragment" && e.1 == id)
        .unwrap_or_else(|| panic!("fragment {id} missing in {events:?}"))
        .2
        .as_str()
}

struct State {
    name: &'static str,
    dns: Plan,
    email: Plan,
    /// A plan swapped in for the second run: the first run is the stored predecessor.
    then_dns: Option<Plan>,
    /// Every warn and fail of the stored run, as `(check, status)`, written out.
    issues: &'static [(&'static str, &'static str)],
    /// The check the page's diff block names as fixed, by catalogue title.
    fixed: Option<&'static str>,
}

fn states() -> Vec<State> {
    let dns_ok = [
        ("dns.dnssec", Status::Pass),
        ("dns.ns", Status::Pass),
        ("dns.ttl", Status::Warn),
    ];
    vec![
        State {
            name: "complete run with warn and fail",
            dns: measured(&[
                ("dns.dnssec", Status::Pass),
                ("dns.caa", Status::Fail),
                ("dns.ttl", Status::Warn),
                ("dns.ns", Status::Pass),
            ]),
            email: measured(&[
                ("email.spf", Status::Pass),
                ("email.dmarc", Status::Warn),
                ("email.mx", Status::Pass),
            ]),
            then_dns: None,
            issues: &[
                ("dns.caa", "fail"),
                ("dns.ttl", "warn"),
                ("email.dmarc", "warn"),
            ],
            fixed: None,
        },
        State {
            name: "check fixed against its predecessor",
            dns: measured(&[
                ("dns.dnssec", Status::Fail),
                ("dns.ns", Status::Pass),
                ("dns.ttl", Status::Warn),
            ]),
            email: measured(&[("email.spf", Status::Pass), ("email.mx", Status::Pass)]),
            then_dns: Some(measured(&dns_ok)),
            issues: &[("dns.ttl", "warn")],
            fixed: Some("fixed"),
        },
        State {
            name: "email not applicable by design",
            dns: measured(&dns_ok),
            email: not_applicable("no MX record"),
            then_dns: None,
            issues: &[("dns.ttl", "warn")],
            fixed: None,
        },
        State {
            name: "dkim not tested from here",
            dns: measured(&dns_ok),
            email: measured(&[
                ("email.spf", Status::Pass),
                ("email.dmarc", Status::Pass),
                ("email.dkim", Status::NotTested),
            ]),
            then_dns: None,
            issues: &[("dns.ttl", "warn")],
            fixed: None,
        },
    ]
}

#[tokio::test]
async fn every_view_of_a_recorded_run_agrees_on_grade_and_statuses() {
    for state in states() {
        let w = world(state.dns.clone(), state.email.clone()).await;
        w.run().await.expect("the first run is recorded");
        if let Some(next) = state.then_dns.clone() {
            *w.dns.lock().unwrap() = next;
            w.run().await.expect("the second run is recorded");
        }

        let entry = w.head_entry().await;
        let v = read_views(&w, &entry).await;
        let name = state.name;

        // Stored run: the warn and fail set, written out above.
        let mut stored: Vec<(String, String)> = entry
            .run
            .checks
            .iter()
            .filter(|c| matches!(c.status, Status::Warn | Status::Fail))
            .map(|c| (c.id.to_string(), status_word(c.status).to_string()))
            .collect();
        stored.sort();
        let wanted: Vec<(String, String)> = state
            .issues
            .iter()
            .map(|(id, s)| (id.to_string(), s.to_string()))
            .collect();
        assert_eq!(stored, wanted, "{name}: stored warn/fail set");

        // Rows: (view, grade letter, (check, status) of every warn and fail, counts line).
        // The result page, its open issues, every evidence view and `GET
        // /api/v1/domains/{domain}` (and `/diff`) read the same entry: same grade, same status
        // per check, same warn/fail set.
        result_page_rows(&w, &entry, &state, &v.api).await;
        let want_letter = letter(entry.grade);
        let page = v.entry_page.text();
        assert_eq!(v.entry_page.status, StatusCode::OK, "{name}: entry page");
        let issues = page_issues(&page);
        let page_set: Vec<(String, String)> = issues
            .iter()
            .map(|(id, tone)| (id.clone(), tone.clone()))
            .collect();
        let grades = [
            ("stored run", want_letter.to_string()),
            ("entry page", page_letter(&page)),
            ("badge", {
                assert_eq!(v.badge.status, StatusCode::OK, "{name}: badge");
                badge_letter(&v.badge.text())
            }),
        ];
        for (view, got) in &grades {
            assert_eq!(got, want_letter, "{name}: grade on the {view}");
        }
        assert_eq!(page_set, wanted, "{name}: warn/fail set on the entry page");
        assert_eq!(
            page_counts(&page),
            expected_counts(&entry),
            "{name}: status counts on the entry page"
        );

        // OG cards compare as PNG bytes against the card the stored entry draws, through the
        // seam the og tests use (the SVG renderer).
        let recorded = UNIX_EPOCH + Duration::from_secs(entry.created_at as u64);
        let seen = UNIX_EPOCH + Duration::from_secs(entry.last_seen_at as u64);
        let pct = netray_engine::history::score_run(&entry.run, &w.state.scoring_profile)
            .overall_percentage;
        let png = |svg: String| {
            netray_web::og::render::svg_to_png(&svg, w.state.font_db.clone()).unwrap()
        };
        let label = netray_web::og::layout::DEFAULT_LABEL;
        let predecessor = match &entry.predecessor_id {
            Some(id) => w.store.get(id).await.unwrap(),
            None => None,
        };
        let plate = netray_web::plate::card_plate(&netray_web::plate::card_layout(
            &entry.run,
            predecessor.as_ref().map(|p| &p.run),
        ));
        assert_eq!(v.og_card.status, StatusCode::OK, "{name}: og card");
        assert!(
            v.og_card.body
                == png(netray_web::og::render::svg_for_grade(
                    DOMAIN,
                    want_letter,
                    label,
                    pct,
                    seen,
                    Some(&plate)
                )),
            "{name}: the OG card does not draw the stored grade {want_letter}"
        );
        assert_eq!(v.entry_card.status, StatusCode::OK, "{name}: entry card");
        assert!(
            v.entry_card.body
                == png(netray_web::og::render::svg_for_entry(
                    DOMAIN,
                    want_letter,
                    label,
                    recorded,
                    &plate
                )),
            "{name}: the entry card does not draw the stored grade {want_letter}"
        );

        // The page's diff block names the fixed check.
        if state.fixed.is_some() {
            let title = netray_engine::catalogue()
                .get(&CheckId::parse("dns.dnssec").unwrap())
                .unwrap()
                .title
                .clone();
            assert!(
                page.contains("Changes since the previous entry")
                    && page.contains("fixed")
                    && page.contains(&title),
                "{name}: the page's diff block must name dns.dnssec ({title}) as fixed"
            );
        }
    }
}

#[tokio::test]
async fn an_incomplete_run_leaves_badge_og_card_and_entry_page_as_they_were() {
    let w = world(
        measured(&[("dns.dnssec", Status::Pass), ("dns.ns", Status::Pass)]),
        measured(&[("email.spf", Status::Pass), ("email.mx", Status::Pass)]),
    )
    .await;
    w.run().await.expect("the first run is recorded");
    let entry = w.head_entry().await;
    let before = read_views(&w, &entry).await;
    assert_eq!(before.badge.status, StatusCode::OK);
    assert_eq!(before.og_card.status, StatusCode::OK);
    assert_eq!(before.entry_page.status, StatusCode::OK);

    *w.email.lock().unwrap() = Plan::Hang;
    let recorded = w.run().await;
    assert!(
        !matches!(
            recorded,
            Some(Recorded::First { .. } | Recorded::Changed { .. })
        ),
        "an incomplete run must not become an entry, got {recorded:?}"
    );

    // C10: the run stream of the result page and the `/api/v1` run answer both say
    // incomplete, with equal statuses.
    pass_throttle().await;
    let streamed = w
        .post(
            &format!("/{DOMAIN}/run"),
            "application/x-www-form-urlencoded",
            "text/event-stream",
            "trigger=recheck",
        )
        .await;
    assert_eq!(streamed.status, StatusCode::OK, "run stream");
    let events = sse_events(&streamed.text());
    assert_eq!(
        events.last().map(|e| e.0.as_str()),
        Some("end"),
        "{events:?}"
    );
    assert_eq!(
        grade_view_letter(last_fragment(&events, "result-grade")),
        "incomplete",
        "the run stream's grade"
    );
    assert!(
        last_fragment(&events, "run-line").contains("incomplete, not recorded"),
        "the run stream's run line"
    );
    let mut stream_rows = page_issues(last_fragment(&events, "issues"));
    stream_rows.extend(page_issues(last_fragment(&events, "passed")));
    let stream_tones = row_tones(&stream_rows, "run stream");

    pass_throttle().await;
    let answer = w
        .post(
            &format!("/api/v1/domains/{DOMAIN}/runs"),
            "application/json",
            "application/json",
            "",
        )
        .await;
    assert_eq!(answer.status, StatusCode::OK, "api run: {}", answer.text());
    let json: serde_json::Value = serde_json::from_slice(&answer.body).unwrap();
    assert_eq!(json["run"]["grade"], "incomplete", "the api run's grade");
    assert_eq!(json["run"]["complete"], false, "the api run is incomplete");
    assert_eq!(
        json["recorded"], "not_stored",
        "the api run is not recorded"
    );
    let api_tones = run_tones(
        api_checks(&json["run"])
            .iter()
            .map(|(id, s)| (id.clone(), status_of(s))),
    );
    assert_eq!(
        stream_tones, api_tones,
        "the run stream and the api run disagree on the statuses"
    );

    let after = read_views(&w, &entry).await;
    assert!(
        after.api == before.api,
        "GET /api/v1/domains/{{domain}} changed after the incomplete run"
    );
    assert!(
        after.badge == before.badge,
        "badge changed after the incomplete run"
    );
    assert!(
        after.og_card == before.og_card,
        "OG card changed after the incomplete run"
    );
    assert!(
        after.entry_page == before.entry_page,
        "entry page changed after the incomplete run"
    );
    assert!(
        after.entry_card == before.entry_card,
        "entry card changed after the incomplete run"
    );
}
