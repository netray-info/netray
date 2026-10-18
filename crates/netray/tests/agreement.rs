//! One run, read through every view main has, says the same thing (spec v2-cutover, Phase 5,
//! requirements 20 and 21; C1/C4 and the half of C2/C5 that main can answer).
//!
//! The views are composed as `serve` composes them: netray-web's host router over lens's routes,
//! wrapped in lens's layers, asked with `Host: netray.info`. The modules are stubs that answer
//! a scripted section; the store is a temporary file. A run is started through lens's admission
//! and read back once it is recorded.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode};
use lens::admission::{Admit, RunEvent, THROTTLE, Trigger, admit};
use lens::config::Config;
use lens::state::AppState;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, CheckResult, Grade, Protocol, Status};
use netray_store::{Entry, HistoryStore, Recorded};
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
    state: AppState,
    store: Arc<HistoryStore>,
    dns: Arc<Mutex<Plan>>,
    email: Arc<Mutex<Plan>>,
    _dir: tempfile::TempDir,
}

/// `serve`'s composition over stub DNS and email modules (email's window: 300 ms) and a store
/// on a temp file.
async fn world(dns: Plan, email: Plan) -> World {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../lens/tests/fixtures/lens.production.toml");
    let mut config = Config::load(fixture.to_str()).expect("production config loads");
    config.cache.enabled = false;
    config.rate_limit.per_ip_per_minute = 1000;
    config.rate_limit.per_ip_burst = 1000;
    config.backends.email.as_mut().unwrap().timeout_ms = 300;
    let dir = tempfile::tempdir().unwrap();
    config.history.enabled = true;
    config.history.db_path = dir.path().join("history.db");

    let store = HistoryStore::open(&config.history.db_path).await.unwrap();
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
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = Some(store.clone());

    let app = lens::layers(netray_web::host::router(HostRouters {
        apex: "netray.info".to_string(),
        probes: lens::probe_router(),
        apex_router: lens::routes(state.clone()),
        dns: None,
        tls: None,
        http: None,
        ip: None,
    }))
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

impl World {
    /// Starts a visit run for the domain and waits for its record.
    async fn run(&self) -> Option<Recorded> {
        // Past the per-domain throttle; `pause` needs this test's current_thread runtime.
        tokio::time::pause();
        tokio::time::advance(THROTTLE).await;
        tokio::time::resume();
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
        let resp = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("host", "netray.info")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
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
}

async fn read_views(w: &World, entry: &Entry) -> Views {
    Views {
        badge: w.get(&format!("/badge/{DOMAIN}.svg")).await,
        og_card: w.get(&format!("/og/{DOMAIN}.png")).await,
        entry_page: w.get(&format!("/r/{}", entry.id)).await,
        entry_card: w.get(&format!("/r/{}/og.png", entry.id)).await,
    }
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
        // TODO(v2-api, v2-inspectors, v2-result-page): add the result page, the open issues and
        // evidence views and `GET /api/v1/domains/{domain}` (and `/diff`) as rows; each reads
        // the same entry and must name this grade and these statuses.
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
        // seam lens's own og tests use (the SVG renderer).
        let recorded = UNIX_EPOCH + Duration::from_secs(entry.created_at as u64);
        let seen = UNIX_EPOCH + Duration::from_secs(entry.last_seen_at as u64);
        let pct = netray_engine::history::score_run(&entry.run, &w.state.scoring_profile)
            .overall_percentage;
        let png =
            |svg: String| lens::og::render::svg_to_png(&svg, w.state.font_db.clone()).unwrap();
        let label = lens::og::layout::DEFAULT_LABEL;
        assert_eq!(v.og_card.status, StatusCode::OK, "{name}: og card");
        assert!(
            v.og_card.body
                == png(lens::og::render::svg_for_grade(
                    DOMAIN,
                    want_letter,
                    label,
                    pct,
                    seen
                )),
            "{name}: the OG card does not draw the stored grade {want_letter}"
        );
        assert_eq!(v.entry_card.status, StatusCode::OK, "{name}: entry card");
        assert!(
            v.entry_card.body
                == png(lens::og::render::svg_for_entry(
                    DOMAIN,
                    want_letter,
                    label,
                    recorded
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

    let after = read_views(&w, &entry).await;
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
