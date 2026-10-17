//! Recording runs into the V2 history (spec v2-history, Phase 4: lens wiring).
//!
//! The modules answer the committed goldens (or are incomplete); lens runs with
//! `tests/fixtures/lens.production.toml`, the cache enabled and a `HistoryStore` on a temp file.
//! The routers are driven in-process; entries are read back with `store.entries`.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

mod common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use common::{
    dns_golden_raw, email_golden, email_incomplete, facts_golden_raw, facts_with_ips, http_module,
    ip_golden, registry_with, tls_module,
};
use lens::check::{CheckInput, SectionError, run_check_with_input};
use lens::config::Config;
use lens::history::build_run;
use lens::routes::{api_router, badge_router};
use lens::state::AppState;
use metrics_util::debugging::{DebugValue, DebuggingRecorder};
use netray_model::run::SectionKind;
use netray_model::{CheckId, Protocol, Status};
use netray_store::HistoryStore;
use serde_json::Value;
use tempfile::NamedTempFile;
use tower::ServiceExt;

const ERRORS: &str = "lens_history_record_errors_total";

#[derive(Clone, Copy)]
enum Mail {
    Golden,
    Incomplete,
}

struct Harness {
    app: Router,
    state: AppState,
    store: Option<Arc<HistoryStore>>,
    _db: NamedTempFile,
}

/// Production config, cache enabled. With `history`, a migrated store on a temp file is wired in.
/// `no_addresses` makes the resolve stage answer no address (lens's IP section: `NoDnsResults`).
async fn harness(history: bool, mail: Mail, no_addresses: bool) -> Harness {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lens.production.toml");
    let mut config = Config::load(path.to_str()).expect("production config loads");
    assert!(config.cache.enabled, "production config enables the cache");

    let db = NamedTempFile::new().unwrap();
    config.history.enabled = history;
    config.history.db_path = db.path().to_path_buf();

    let store = if history {
        let store = HistoryStore::open(db.path()).await.unwrap();
        store.migrate().await.unwrap();
        Some(Arc::new(store))
    } else {
        None
    };

    let email = match mail {
        Mail::Golden => email_golden("beacon.sse"),
        Mail::Incomplete => email_incomplete(),
    };
    let facts = if no_addresses {
        facts_with_ips(&[])
    } else {
        facts_golden_raw("prism.sse")
    };
    let registry = registry_with(
        dns_golden_raw("prism.sse"),
        facts,
        http_module(Some("spectra-inspect.json")),
        email,
        ip_golden("ifconfig-json.json"),
        tls_module(Some("tlsight-inspect.json")),
    );
    let mut state = AppState::with_registry(config, registry).unwrap();
    state.history_store = store.clone();
    assert!(state.badge_check_fn.is_none(), "use the real check");

    let (api, _) = api_router().split_for_parts();
    let (badge, _) = badge_router().split_for_parts();
    let app = Router::new()
        .merge(api.with_state(state.clone()))
        .merge(badge.with_state(state.clone()))
        .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));
    Harness {
        app,
        state,
        store,
        _db: db,
    }
}

/// Drops the fields that differ between two runs of the same check.
fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("duration_ms");
            map.remove("snapshot_id");
            map.values_mut().for_each(strip_volatile);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_volatile),
        _ => {}
    }
}

/// The sync (JSON) response of a check, minus its volatile fields.
async fn sync_check(app: &Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    let mut body: Value = serde_json::from_slice(&bytes).expect("sync check answers JSON");
    strip_volatile(&mut body);
    (status, body)
}

async fn get_status(app: &Router, uri: &str) -> StatusCode {
    let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    to_bytes(resp.into_body(), 4 * 1024 * 1024).await.unwrap();
    status
}

fn input() -> CheckInput {
    CheckInput {
        domain: "example.com".to_string(),
        dkim_selectors: None,
        client_ip: None,
        request_id: None,
    }
}

fn catalogue_ids(protocol: Protocol) -> Vec<CheckId> {
    netray_engine::catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == protocol)
        .map(|e| e.id.clone())
        .collect()
}

fn counter(snap: &metrics_util::debugging::Snapshotter, name: &str) -> u64 {
    snap.snapshot()
        .into_vec()
        .into_iter()
        .filter(|(k, _, _, _)| k.key().name() == name)
        .map(|(_, _, _, v)| match v {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum()
}

/// The domain's entries once at least `n` are stored: the run task records after the response
/// (v2-admission requirement 3), so a read right after it may come too early. Bounded at 5 s.
async fn entries_eventually(
    store: &HistoryStore,
    domain: &str,
    n: usize,
) -> Vec<netray_store::Entry> {
    for _ in 0..250 {
        let entries = store.entries(domain).await.unwrap();
        if entries.len() >= n {
            return entries;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    store.entries(domain).await.unwrap()
}

/// Gives a run task that records after the response the time to write before an absence is
/// asserted.
async fn settle() {
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

// ---------------------------------------------------------------------------

/// C1/C8: one entry per complete run, holding every catalogue check of each run section; the
/// response is the one a history-less lens gives.
#[tokio::test]
async fn history_c1_c8_complete_run_is_recorded_whole_and_response_unchanged() {
    let with = harness(true, Mail::Golden, false).await;
    let without = harness(false, Mail::Golden, false).await;

    let (status_with, body_with) = sync_check(&with.app, "/api/check/example.com").await;
    let (status_without, body_without) = sync_check(&without.app, "/api/check/example.com").await;
    assert_eq!(status_with, StatusCode::OK);
    assert_eq!(status_without, StatusCode::OK);
    assert_eq!(
        body_with, body_without,
        "recording must not change the response"
    );

    let entries = entries_eventually(with.store.as_ref().unwrap(), "example.com", 1).await;
    assert_eq!(entries.len(), 1, "exactly one entry for the run");
    let run = &entries[0].run;
    assert!(run.complete);
    assert!(!run.sections.is_empty(), "the run has sections");
    assert!(
        run.sections
            .iter()
            .any(|s| s.outcome == SectionKind::Measured),
        "the run measured a section"
    );
    // A complete run may hold not-applicable sections (the stub IP section has no public
    // address); every section still lists every catalogue check of its protocol.
    for section in &run.sections {
        let have: Vec<CheckId> = run
            .checks
            .iter()
            .filter(|c| c.id.protocol() == section.protocol)
            .map(|c| c.id.clone())
            .collect();
        for want in catalogue_ids(section.protocol) {
            assert!(
                have.contains(&want),
                "run lacks catalogue check {want} of {:?}",
                section.protocol
            );
        }
    }
}

/// C1: a not-applicable section lists all its catalogue checks as not_applicable; an errored one
/// is Incomplete with all its checks unmeasured.
#[tokio::test]
async fn history_c1_not_applicable_and_errored_sections_list_every_catalogue_check() {
    let h = harness(false, Mail::Golden, false).await;
    let mut output = run_check_with_input(&h.state, input()).await;
    output.sections.insert(
        "tls".to_string(),
        Err(SectionError::NotApplicable {
            reason: "no HTTPS".to_string(),
        }),
    );
    output.sections.insert(
        "http".to_string(),
        Err(SectionError::BackendError("boom".to_string())),
    );

    let run = build_run(&h.state, &output, &[]);

    let outcome = |p: Protocol| {
        run.sections
            .iter()
            .find(|s| s.protocol == p)
            .unwrap_or_else(|| panic!("no run section for {p:?}"))
            .outcome
            .clone()
    };
    assert_eq!(outcome(Protocol::Tls), SectionKind::NotApplicable);
    assert_eq!(outcome(Protocol::Http), SectionKind::Incomplete);

    for (protocol, status) in [
        (Protocol::Tls, Status::NotApplicable),
        (Protocol::Http, Status::Unmeasured),
    ] {
        let want = catalogue_ids(protocol);
        assert!(!want.is_empty());
        for id in want {
            let check = run
                .checks
                .iter()
                .find(|c| c.id == id)
                .unwrap_or_else(|| panic!("run lacks {id}"));
            assert_eq!(check.status, status, "{id}");
        }
    }
}

/// C3/C9: the badge route computing a complete run on a cache miss records it.
#[tokio::test]
async fn history_c3_c9_badge_cache_miss_records_an_entry() {
    let h = harness(true, Mail::Golden, false).await;
    let status = get_status(&h.app, "/badge/example.com.svg").await;
    assert_eq!(status, StatusCode::OK);
    let entries = entries_eventually(h.store.as_ref().unwrap(), "example.com", 1).await;
    assert_eq!(entries.len(), 1, "the badge's run is recorded");
}

/// C10: a check with custom DKIM selectors is not recorded.
#[tokio::test]
async fn history_c10_dkim_selectors_record_nothing() {
    let h = harness(true, Mail::Golden, false).await;
    let (status, _) = sync_check(&h.app, "/api/check/example.com?dkim_selectors=default").await;
    assert_eq!(status, StatusCode::OK);
    settle().await;
    let entries = h
        .store
        .as_ref()
        .unwrap()
        .entries("example.com")
        .await
        .unwrap();
    assert!(entries.is_empty(), "got {} entries", entries.len());
}

/// C11: an incomplete run (a section did not answer) is not stored.
#[tokio::test]
async fn history_c11_incomplete_run_records_nothing() {
    let h = harness(true, Mail::Incomplete, false).await;
    let (status, body) = sync_check(&h.app, "/api/check/example.com").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["complete"], false, "body: {body}");
    settle().await;
    let entries = h
        .store
        .as_ref()
        .unwrap()
        .entries("example.com")
        .await
        .unwrap();
    assert!(entries.is_empty(), "got {} entries", entries.len());
}

/// C12: with no DNS result the IP section is NoDnsResults: not stored, its checks unmeasured.
#[tokio::test]
async fn history_c12_no_dns_results_ip_section_is_unmeasured_and_unstored() {
    let h = harness(true, Mail::Golden, true).await;

    let output = run_check_with_input(&h.state, input()).await;
    assert!(
        matches!(
            output.sections.get("ip"),
            Some(Err(SectionError::NoDnsResults))
        ),
        "precondition: the IP section is NoDnsResults"
    );
    let run = build_run(&h.state, &output, &[]);
    assert!(!run.complete);
    let ip = run
        .sections
        .iter()
        .find(|s| s.protocol == Protocol::Ip)
        .expect("run has an IP section");
    assert_eq!(ip.outcome, SectionKind::Incomplete);
    let want = catalogue_ids(Protocol::Ip);
    assert!(!want.is_empty());
    for id in want {
        let check = run
            .checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("run lacks {id}"));
        assert_eq!(check.status, Status::Unmeasured, "{id}");
    }

    // The same run through the run task, which records into the store wired into the state:
    // the store answers NotStored and nothing is stored (v2-admission requirement 3).
    let mut stream = match lens::admission::admit(
        &h.state,
        "example.com",
        std::net::IpAddr::from([127, 0, 0, 1]),
        lens::admission::Trigger::Recheck,
        false,
    ) {
        lens::admission::Admit::Admitted(s) => s,
        _ => panic!("expected Admitted"),
    };
    let mut recorded = None;
    while let Some(event) = stream.next().await {
        if let lens::admission::RunEvent::Recorded(r) = event {
            recorded = Some(r);
        }
    }
    assert!(
        matches!(
            recorded,
            Some(Some(netray_store::Recorded::NotStored { .. }))
        ),
        "the incomplete run is not stored"
    );
    let entries = h
        .store
        .as_ref()
        .unwrap()
        .entries("example.com")
        .await
        .unwrap();
    assert!(entries.is_empty(), "got {} entries", entries.len());
}

/// C5/C17: a store whose writes fail leaves the response unchanged and counts the error.
#[tokio::test(flavor = "current_thread")]
async fn history_c5_failing_store_keeps_response_and_counts_the_error() {
    let recorder = DebuggingRecorder::new();
    let snap = recorder.snapshotter();
    let _guard = metrics::set_default_local_recorder(&recorder);

    let broken = harness(true, Mail::Golden, false).await;
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(broken._db.path());
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query("DROP TABLE entries")
        .execute(&pool)
        .await
        .unwrap();
    let reference = harness(false, Mail::Golden, false).await;

    let (status, body) = sync_check(&broken.app, "/api/check/example.com").await;
    let (ref_status, ref_body) = sync_check(&reference.app, "/api/check/example.com").await;

    assert_eq!(status, ref_status);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body, ref_body,
        "a failing store must not change the response"
    );
    // The run task records after the response: accumulate reads (each snapshot drains the
    // counter) until the error is counted, bounded at 5 s.
    let mut errors = 0;
    for _ in 0..250 {
        errors += counter(&snap, ERRORS);
        if errors >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(errors, 1);
}

/// C17: the error counter renders at zero before any failure.
#[test]
fn history_c17_error_counter_renders_at_zero() {
    let recorder = netray_common::server::metrics_recorder(lens::metrics::HISTOGRAM_BUCKETS);
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    lens::metrics::init_zero_series();
    let out = handle.render();

    assert!(
        out.lines()
            .any(|l| l == "lens_history_record_errors_total 0"),
        "missing zero series in:\n{out}"
    );
}
