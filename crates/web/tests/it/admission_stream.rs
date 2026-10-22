//! The run stream of `lens::admission::admit` (specs/features/v2-admission/spec.md, requirement
//! 21; criteria C5 and C16 to C20).
//!
//! The modules answer the committed goldens (or are held open, or incomplete); the state is the
//! one lens built from `tests/fixtures/lens.production.toml`. `admit` is driven directly, no HTTP.
//!
//! Copied from crates/lens/tests/admission_stream.rs (spec v2-cutover, Phase 6); nothing dropped.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::common;

use common::{
    dns_golden_raw, email_golden, email_incomplete, facts_golden_raw, gated, http_module,
    ip_golden, registry_with, tls_golden,
};
use netray_model::Protocol;
use netray_store::{HistoryStore, Recorded};
use netray_web::admission::{Admit, ApexState, RunEvent, RunStream, Trigger, admit};
use sqlx::Connection;
use tempfile::NamedTempFile;
use tokio::sync::Notify;
use tokio::time::timeout;

const DOMAIN: &str = "example.com";
const WAIT: Duration = Duration::from_secs(10);

fn client() -> IpAddr {
    "127.0.0.1".parse().unwrap()
}

fn admitted(a: Admit) -> RunStream {
    match a {
        Admit::Admitted(s) => s,
        _ => panic!("expected Admitted"),
    }
}

/// Which held-open modules a state gets: HTTP, email, IP.
struct Gates {
    http: (Arc<Notify>, Arc<Notify>),
    email: (Arc<Notify>, Arc<Notify>),
    ip: (Arc<Notify>, Arc<Notify>),
}

impl Gates {
    fn new() -> Self {
        let pair = || (Arc::new(Notify::new()), Arc::new(Notify::new()));
        Gates {
            http: pair(),
            email: pair(),
            ip: pair(),
        }
    }
    fn open(&self) {
        self.http.1.notify_one();
        self.email.1.notify_one();
        self.ip.1.notify_one();
    }
}

fn state_with(email_incomplete_run: bool, gates: Option<&Gates>) -> ApexState {
    let hold = |m, g: Option<&(Arc<Notify>, Arc<Notify>)>| match g {
        Some((entered, release)) => gated(m, entered.clone(), release.clone()),
        None => m,
    };
    let email = if email_incomplete_run {
        email_incomplete()
    } else {
        email_golden("beacon.sse")
    };
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts_golden_raw("prism.sse"),
        hold(
            http_module(Some("spectra-inspect.json")),
            gates.map(|g| &g.http),
        ),
        hold(email, gates.map(|g| &g.email)),
        hold(ip_golden("ifconfig-json.json"), gates.map(|g| &g.ip)),
        tls_golden("tlsight-inspect.json"),
    );
    common::production_state(registry)
}

async fn store_on(db: &NamedTempFile) -> Arc<HistoryStore> {
    let store = HistoryStore::open(db.path()).await.unwrap();
    store.migrate().await.unwrap();
    Arc::new(store)
}

async fn next(s: &mut RunStream) -> Option<RunEvent> {
    timeout(WAIT, s.next()).await.expect("stream stalled")
}

/// Reads to the end; the events in order.
async fn read_all(s: &mut RunStream) -> Vec<RunEvent> {
    let mut events = Vec::new();
    while let Some(e) = next(s).await {
        events.push(e);
    }
    events
}

fn section_protocol(e: &RunEvent) -> Protocol {
    match e {
        RunEvent::Section(ev) => ev.protocol,
        _ => panic!("expected a Section event"),
    }
}

#[tokio::test]
async fn c5_recorded_follows_done_stream_ends_and_drop_keeps_the_run() {
    let gates = Gates::new();
    let state = state_with(false, Some(&gates));
    let first = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    // Drop mid-run: the run is held at the gated sections.
    drop(first);

    let mut second = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    assert!(second.joined(), "the second caller joins the run in flight");
    gates.open();

    let events = read_all(&mut second).await;
    let done = events
        .iter()
        .position(|e| matches!(e, RunEvent::Done(_)))
        .expect("the dropped run still completes");
    let recorded = events
        .iter()
        .position(|e| matches!(e, RunEvent::Recorded(_)))
        .expect("Recorded is yielded");
    assert!(done < recorded, "Recorded never precedes Done");
    assert_eq!(recorded, events.len() - 1, "Recorded is the last event");
    assert!(second.next().await.is_none(), "None after Recorded");
}

#[tokio::test]
async fn c16_a_joiner_receives_finished_sections_first() {
    let gates = Gates::new();
    let state = state_with(false, Some(&gates));
    let mut first = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    assert!(!first.joined());

    let mut early = vec![
        section_protocol(&next(&mut first).await.unwrap()),
        section_protocol(&next(&mut first).await.unwrap()),
    ];
    early.sort_by_key(|p| p.as_str());
    let mut want = vec![Protocol::Dns, Protocol::Tls];
    want.sort_by_key(|p| p.as_str());
    assert_eq!(early, want, "DNS and TLS finish while the others are held");

    let mut second = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    assert!(second.joined());

    let mut head = vec![
        section_protocol(&next(&mut second).await.unwrap()),
        section_protocol(&next(&mut second).await.unwrap()),
    ];
    head.sort_by_key(|p| p.as_str());
    assert_eq!(head, want, "the joiner first gets the finished sections");

    gates.open();
    let rest = read_all(&mut second).await;
    assert_eq!(rest.len(), 5, "three sections, Done, Recorded");
    let mut late: Vec<Protocol> = rest[..3].iter().map(section_protocol).collect();
    late.sort_by_key(|p| p.as_str());
    let mut want_late = vec![Protocol::Http, Protocol::Email, Protocol::Ip];
    want_late.sort_by_key(|p| p.as_str());
    assert_eq!(late, want_late);
    assert!(matches!(rest[3], RunEvent::Done(_)));
    assert!(matches!(rest[4], RunEvent::Recorded(_)));
}

#[tokio::test]
async fn c17_one_section_per_configured_section_then_done_then_recorded() {
    let state = state_with(false, None);
    let expected = state.plan.len();
    assert_eq!(
        expected, 5,
        "the production config configures five sections"
    );
    let mut stream = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    let events = read_all(&mut stream).await;

    assert_eq!(events.len(), expected + 2);
    for e in &events[..expected] {
        assert!(matches!(e, RunEvent::Section(_)));
    }
    assert!(matches!(events[expected], RunEvent::Done(_)));
    assert!(matches!(events[expected + 1], RunEvent::Recorded(_)));
}

#[tokio::test]
async fn c18_done_does_not_wait_for_the_history_write() {
    let db = NamedTempFile::new().unwrap();
    let store = store_on(&db).await;
    let mut state = state_with(false, None);
    state.history_store = Some(store.clone());

    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(db.path());
    let mut hold = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut hold)
        .await
        .unwrap();

    let mut stream = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    loop {
        match timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("Done arrives while the write path is held")
            .expect("stream ended before Done")
        {
            RunEvent::Section(_) => {}
            RunEvent::Done(_) => break,
            RunEvent::Recorded(_) => panic!("Recorded before Done"),
        }
    }

    sqlx::query("ROLLBACK").execute(&mut hold).await.unwrap();
    match next(&mut stream).await.expect("Recorded follows") {
        RunEvent::Recorded(Some(Recorded::First { .. })) => {}
        RunEvent::Recorded(other) => panic!("expected First, got {other:?}"),
        _ => panic!("expected Recorded"),
    }
}

fn recorded_of(events: Vec<RunEvent>) -> Option<Recorded> {
    match events.into_iter().last() {
        Some(RunEvent::Recorded(r)) => r,
        _ => panic!("the stream does not end in Recorded"),
    }
}

#[tokio::test]
async fn c19_recorded_carries_what_the_store_returned() {
    // Incomplete run, store: NotStored.
    let db = NamedTempFile::new().unwrap();
    let mut state = state_with(true, None);
    state.history_store = Some(store_on(&db).await);
    let mut s = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    match recorded_of(read_all(&mut s).await) {
        Some(Recorded::NotStored { .. }) => {}
        other => panic!("expected NotStored, got {other:?}"),
    }

    // No store: none.
    let state = state_with(false, None);
    assert!(state.history_store.is_none());
    let mut s = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    assert!(recorded_of(read_all(&mut s).await).is_none());

    // A store whose writes fail: none.
    let db = NamedTempFile::new().unwrap();
    let mut state = state_with(false, None);
    state.history_store = Some(store_on(&db).await);
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(db.path());
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let mut s = admitted(admit(&state, DOMAIN, client(), Trigger::Visit, false));
    assert!(recorded_of(read_all(&mut s).await).is_none());
}

#[tokio::test]
async fn c20_done_run_equals_the_stored_run() {
    let db = NamedTempFile::new().unwrap();
    let store = store_on(&db).await;
    let mut state = state_with(false, None);
    state.history_store = Some(store.clone());

    let mut stream = admitted(admit(&state, DOMAIN, client(), Trigger::Recheck, false));
    let events = read_all(&mut stream).await;
    let published = events
        .iter()
        .find_map(|e| match e {
            RunEvent::Done(p) => Some(p.clone()),
            _ => None,
        })
        .expect("Done is yielded");
    let stored = store
        .latest(DOMAIN)
        .await
        .unwrap()
        .expect("the run was recorded");
    assert_eq!(published.run, stored.run);
}
