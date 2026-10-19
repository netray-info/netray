//! OG cards carry the layout plate (spec v2-result-page, Phase 7, requirement 29).
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use lens::config::{
    BackendsConfig, BadgesConfig, CacheConfig, Config, EcosystemConfig, OgCardsConfig,
    RateLimitConfig, ScoringConfig, ServerConfig, SiteConfig, SnapshotsConfig,
};
use lens::og::layout::DEFAULT_LABEL;
use lens::og::render::{format_utc_timestamp, svg_for_entry, svg_for_grade, svg_to_png};
use lens::snapshot::SnapshotStore;
use lens::state::AppState;
use netray_model::diff::diff;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, Grade, Protocol, Status};
use netray_store::{HistoryStore, Recorded};
use netray_web::plate::{card_layout, card_plate};

const DOMAIN: &str = "shop.example.com";

fn config(dir: &std::path::Path) -> Config {
    Config {
        modules: Default::default(),
        server: ServerConfig {
            bind: ([127, 0, 0, 1], 0).into(),
            metrics_bind: ([127, 0, 0, 1], 0).into(),
            trusted_proxies: Vec::new(),
        },
        backends: BackendsConfig {
            resolve_timeout_ms: 2000,
            dns: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            tls: lens::config::BackendConfig {
                url: Some("http://127.0.0.1:19998".to_string()),
                timeout_ms: 100,
                ..Default::default()
            },
            ip: lens::config::BackendConfig {
                timeout_ms: 100,
                ..Default::default()
            },
            http: None,
            email: None,
        },
        ecosystem: EcosystemConfig::default(),
        cache: CacheConfig {
            enabled: false,
            ttl_seconds: 300,
        },
        telemetry: Default::default(),
        rate_limit: RateLimitConfig {
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            global_per_minute: 1000,
            global_burst: 100,
        },
        scoring: ScoringConfig::default(),
        site: SiteConfig::default(),
        badges: BadgesConfig::default(),
        og_cards: OgCardsConfig { enabled: true },
        snapshots: SnapshotsConfig {
            enabled: true,
            db_path: dir.join("snapshots.db"),
            ..Default::default()
        },
        history: lens::config::HistoryConfig {
            enabled: true,
            db_path: dir.join("history.db"),
            ..Default::default()
        },
        admission: lens::config::AdmissionConfig::default(),
        inspect: Default::default(),
        api: Default::default(),
    }
}

fn a_run(profile_version: u32) -> Run {
    let check = |id: &str, status| RunCheck {
        id: CheckId::parse(id).unwrap(),
        status,
        findings: vec![],
        evidence: vec![],
    };
    Run {
        profile_version,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Dns,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks: vec![
            check("dns.dnssec_valid", Status::Pass),
            check("tls.caa_compliant", Status::Fail),
        ],
        blocks: vec![],
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

struct Seeded {
    state: AppState,
    id: String,
    run: Run,
    seen: i64,
    _dir: tempfile::TempDir,
}

/// A state whose history holds one stored entry for `DOMAIN`, last seen 10 s ago.
async fn seeded() -> Seeded {
    let dir = tempfile::tempdir().unwrap();
    let cfg = config(dir.path());
    let history = HistoryStore::open(&cfg.history.db_path).await.unwrap();
    history.migrate().await.unwrap();
    let history = Arc::new(history);
    let snapshots = SnapshotStore::new(&cfg.snapshots.db_path).await.unwrap();
    snapshots.migrate().await.unwrap();

    let mut state = AppState::new(cfg).unwrap();
    let run = a_run(state.scoring_profile.version);
    let seen = now_unix() - 10;
    let id = match history
        .record(DOMAIN, &run, &[], seen, |p, c| diff(p, c, &[], None))
        .await
        .unwrap()
    {
        Recorded::First { id } => id,
        other => panic!("expected First, got {other:?}"),
    };
    state.history_store = Some(history);
    state.snapshot_store = Some(Arc::new(snapshots));
    Seeded {
        state,
        id,
        run,
        seen,
        _dir: dir,
    }
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    (status, bytes.to_vec())
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[tokio::test]
async fn c2_entry_card_is_a_1200x630_png_and_its_svg_carries_the_plate() {
    let s = seeded().await;
    let (status, body) = get(lens::app(s.state.clone()), &format!("/r/{}/og.png", s.id)).await;
    assert_eq!(status, StatusCode::OK);
    let img = image::load_from_memory(&body).expect("body decodes as an image");
    assert_eq!((img.width(), img.height()), (1200, 630));

    let plate = card_plate(&card_layout(&s.run, None));
    let t = UNIX_EPOCH + Duration::from_secs(s.seen as u64);
    let svg = svg_for_entry(DOMAIN, "B", DEFAULT_LABEL, t, &plate);
    assert_eq!(count(&svg, "class=\"plate__line\""), 54);
    assert!(svg.contains(DOMAIN));
    assert!(svg.contains(">B<"));
    assert!(svg.contains(&format_utc_timestamp(t)));
}

#[tokio::test]
async fn c5_domain_card_with_a_fresh_head_carries_the_plate_and_without_an_entry_none() {
    let s = seeded().await;
    let pct =
        netray_engine::history::score_run(&s.run, &s.state.scoring_profile).overall_percentage;
    let plate = card_plate(&card_layout(&s.run, None));
    let svg = svg_for_grade(
        DOMAIN,
        "B",
        DEFAULT_LABEL,
        pct,
        UNIX_EPOCH + Duration::from_secs(s.seen as u64),
        Some(&plate),
    );
    assert_eq!(count(&svg, "class=\"plate__line\""), 54);
    let expected = svg_to_png(&svg, s.state.font_db.clone()).unwrap();

    let (status, body) = get(
        lens::routes::og_router().with_state(s.state.clone()),
        &format!("/og/{DOMAIN}.png"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body == expected, "PNG differs from the head's plated card");

    // No entry: the card is the unscored v1 card, no plate.
    let dir = tempfile::tempdir().unwrap();
    let empty = AppState::new(config(dir.path())).unwrap();
    let before = now_unix();
    let (status, body) = get(
        lens::routes::og_router().with_state(empty.clone()),
        "/og/empty.example.com.png",
    )
    .await;
    let after = now_unix();
    assert_eq!(status, StatusCode::OK);
    let unplated = |at: i64| {
        let svg = svg_for_grade(
            "empty.example.com",
            "?",
            DEFAULT_LABEL,
            0.0,
            UNIX_EPOCH + Duration::from_secs(at as u64),
            None,
        );
        assert_eq!(count(&svg, "plate__line"), 0);
        svg_to_png(&svg, empty.font_db.clone()).unwrap()
    };
    assert!(
        body == unplated(before) || body == unplated(after),
        "unscored PNG differs from the plain v1 card"
    );
}

#[test]
fn c6_font_db_holds_two_inter_faces_and_one_jetbrains_mono() {
    let db = lens::og::fonts::init_font_db();
    assert_eq!(db.len(), 3);
    let family = |f: &fontdb::FaceInfo| f.families[0].0.clone();
    let mono = db.faces().filter(|f| family(f) == "JetBrains Mono").count();
    let inter = db.faces().filter(|f| family(f) == "Inter").count();
    assert_eq!((inter, mono), (2, 1));
}
