//! The site-wide image-run budget of badge and OG runs (specs/features/v2-release-cleanup,
//! Phase 1, requirements 1 to 3): a token bucket of `image_runs_per_minute`, taken only when a
//! badge or OG run starts.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use metrics_util::debugging::{DebugValue, DebuggingRecorder};
use netray_engine::Registry;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::admission::{Admit, ApexState, Reason, Trigger, admit};
use netray_web::config::ApexConfig;
use tower::ServiceExt;

use crate::common;

const BADGE_CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
const VISITOR: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));

fn state(image_runs_per_minute: u32, cache_window_seconds: u64) -> ApexState {
    let mut config = ApexConfig {
        cache_enabled: true,
        cache_window_seconds,
        ..ApexConfig::default()
    };
    config.limits.image_runs_per_minute = image_runs_per_minute;
    config.limits.rate_limit_per_ip_per_minute = 1000;
    config.limits.rate_limit_per_ip_burst = 1000;
    config.limits.rate_limit_global_per_minute = 1000;
    config.limits.rate_limit_global_burst = 1000;
    ApexState::new(
        config,
        Registry::new().with(common::dns_incomplete()),
        vec![(Protocol::Dns, Duration::from_millis(500))],
        Duration::from_millis(200),
        common::profile(),
    )
}

/// `Some(true)` a run started, `Some(false)` joined, `None` refused or not started; the refusal's
/// reason in the second slot.
fn badge(state: &ApexState, domain: &str) -> (Option<bool>, Option<Reason>) {
    outcome(admit(state, domain, BADGE_CLIENT, Trigger::Badge, false))
}

fn outcome(admitted: Admit) -> (Option<bool>, Option<Reason>) {
    match admitted {
        Admit::Admitted(stream) => (Some(!stream.joined()), None),
        Admit::Refused(r) => (None, Some(r.reason)),
        Admit::NotStarted => (None, None),
    }
}

#[tokio::test(start_paused = true)]
async fn c1_a_spent_budget_refuses_the_next_badge_run() {
    let s = state(2, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    assert_eq!(badge(&s, "example.net").0, Some(true));
    assert_eq!(
        badge(&s, "example.org"),
        (None, Some(Reason::ImageBudget)),
        "the third distinct domain must be refused by the image budget"
    );
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c1_og_runs_draw_from_the_same_budget() {
    let s = state(1, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    let og = outcome(admit(&s, "example.net", BADGE_CLIENT, Trigger::Og, false));
    assert_eq!(og, (None, Some(Reason::ImageBudget)));
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c1_the_budget_refills_continuously() {
    let s = state(2, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    assert_eq!(badge(&s, "example.net").0, Some(true));
    assert_eq!(badge(&s, "example.org").1, Some(Reason::ImageBudget));
    tokio::time::advance(Duration::from_secs(30)).await;
    // Half a minute at 2 per minute is one token; the budget refusal above spent no recompute
    // token of example.org (its TTL is an hour).
    assert_eq!(badge(&s, "example.org").0, Some(true));
    assert_eq!(badge(&s, "example.edu").1, Some(Reason::ImageBudget));
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c1_joining_a_run_in_flight_takes_no_token() {
    let s = state(2, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    assert_eq!(badge(&s, "example.com").0, Some(false), "must join");
    assert_eq!(badge(&s, "example.net").0, Some(true));
    assert_eq!(badge(&s, "example.org").1, Some(Reason::ImageBudget));
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c1_a_prefetch_takes_no_token() {
    let s = state(1, 300);
    let pre = outcome(admit(&s, "example.com", BADGE_CLIENT, Trigger::Badge, true));
    assert_eq!(pre, (None, None), "a prefetch starts nothing");
    assert_eq!(badge(&s, "example.net").0, Some(true));
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c3_a_throttle_refusal_spends_no_token() {
    let s = state(2, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    s.runs.drain().await;
    assert_eq!(badge(&s, "example.com").1, Some(Reason::Throttle));
    assert_eq!(badge(&s, "example.net").0, Some(true));
    assert_eq!(badge(&s, "example.org").1, Some(Reason::ImageBudget));
    s.runs.drain().await;
}

#[tokio::test(start_paused = true)]
async fn c3_visits_never_touch_the_budget() {
    let s = state(1, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    let visit = outcome(admit(&s, "example.net", VISITOR, Trigger::Visit, false));
    assert_eq!(
        visit.0,
        Some(true),
        "a visit must start whatever the image budget holds"
    );
    assert_eq!(badge(&s, "example.org").1, Some(Reason::ImageBudget));
    s.runs.drain().await;
}

#[test]
fn c2_a_budget_refusal_counts_image_budget() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    metrics::with_local_recorder(&recorder, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        rt.block_on(async {
            let s = state(1, 300);
            assert_eq!(badge(&s, "example.com").0, Some(true));
            assert_eq!(badge(&s, "example.net").1, Some(Reason::ImageBudget));
            s.runs.drain().await;
        });
    });
    let count: u64 = snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .filter(|(k, _, _, _)| {
            let key = k.key();
            key.name() == "netray_admission_rejections_total"
                && key
                    .labels()
                    .any(|l| l.key() == "reason" && l.value() == "image_budget")
        })
        .map(|(_, _, _, v)| match v {
            DebugValue::Counter(n) => n,
            _ => 0,
        })
        .sum();
    assert_eq!(count, 1);
}

/// A complete head of `domain` graded `grade`, last seen `age_s` ago, in a temp store on `state`.
async fn seed_head(
    state: &mut ApexState,
    domain: &str,
    grade: Grade,
    age_s: i64,
) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let run = Run {
        profile_version: state.scoring_profile.version,
        engine_version: "0.30.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![RunCheck {
            id: CheckId::parse("dns.dnssec_valid").unwrap(),
            status: Status::Pass,
            findings: vec![],
            evidence: vec![],
        }],
        blocks: vec![],
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    store
        .record(domain, &run, &[], now - age_s, |p, c| {
            netray_model::diff::diff(p, c, &[], None)
        })
        .await
        .unwrap();
    state.history_store = Some(Arc::new(store));
    dir
}

async fn get(state: &ApexState, uri: &str) -> (StatusCode, String, Vec<u8>) {
    let app = netray_web::pages::badge::router(state.clone())
        .merge(netray_web::pages::og::router(state.clone()));
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cc = resp
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = to_bytes(resp.into_body(), 1 << 22).await.unwrap().to_vec();
    (status, cc, body)
}

#[tokio::test]
async fn c2_a_refused_badge_without_head_renders_unscored_with_short_cache() {
    let s = state(1, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    let (status, cc, body) = get(&s, "/badge/example.net.svg").await;
    assert_eq!(status, StatusCode::OK);
    assert!(cc.contains("max-age=300"), "short Cache-Control, got {cc}");
    assert!(
        String::from_utf8(body).unwrap().contains(">?<"),
        "unscored badge"
    );
    s.runs.drain().await;
}

#[tokio::test]
async fn c2_a_refused_badge_with_a_stale_head_renders_its_grade() {
    // A one-second cache window makes the head (60 s old) stale, so the badge asks for a run.
    let mut s = state(1, 1);
    let _dir = seed_head(&mut s, "example.net", Grade::B, 60).await;
    assert_eq!(badge(&s, "example.com").0, Some(true));
    let (status, cc, body) = get(&s, "/badge/example.net.svg").await;
    assert_eq!(status, StatusCode::OK);
    assert!(cc.contains("max-age=300"), "short Cache-Control, got {cc}");
    assert!(
        String::from_utf8(body).unwrap().contains(">B<"),
        "the head's grade"
    );
    s.runs.drain().await;
}

#[tokio::test]
async fn c2_a_refused_og_card_without_head_is_an_image_not_an_error() {
    let s = state(1, 300);
    assert_eq!(badge(&s, "example.com").0, Some(true));
    let resp = netray_web::pages::og::router(s.clone())
        .oneshot(
            Request::builder()
                .uri("/og/example.net.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
    s.runs.drain().await;
}
