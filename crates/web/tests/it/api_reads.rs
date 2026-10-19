//! API v1 read endpoints (spec v2-api, requirements 16, 17, 18): `/catalogue`, `/profiles/{n}`,
//! `/meta`, driven through `netray_web::api::router`, paths relative to the mount.
use std::net::IpAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::future::BoxFuture;
use http_body_util::BodyExt;
use netray_engine::catalogue;
use netray_engine::profile::Profiles;
use netray_web::api::{
    AdmissionLimits, ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger,
};
use serde_json::{Value, json};
use tower::ServiceExt;

struct StubRuns;

impl DomainRuns for StubRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

const LIMITS: AdmissionLimits = AdmissionLimits {
    client_budget_per_hour: 11,
    domain_run_slots: 3,
    rate_limit_per_ip_per_minute: 21,
    rate_limit_per_ip_burst: 22,
    rate_limit_global_per_minute: 23,
    rate_limit_global_burst: 24,
};

fn app() -> Router {
    netray_web::api::router(
        ApiState::new(ApiConfig::default(), Arc::new(StubRuns)).with_admission_limits(LIMITS),
    )
}

async fn get(path: &str) -> (StatusCode, Value) {
    let resp = app()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

const KEYS: [&str; 11] = [
    "id",
    "protocol",
    "title",
    "finding",
    "impact",
    "fix_hint",
    "fix_owner",
    "severity",
    "source",
    "guide",
    "short",
];

#[tokio::test]
async fn catalogue_lists_every_check_in_file_order_with_eleven_fields() {
    let file: toml::Table =
        toml::from_str(include_str!("../../../engine/catalogue/checks.toml")).unwrap();
    let expected = file["check"].as_array().unwrap();

    let (status, body) = get("/catalogue").await;
    assert_eq!(status, StatusCode::OK);
    let checks = body["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), expected.len());
    assert_eq!(checks.len(), catalogue().entries().len());
    for (got, want) in checks.iter().zip(expected) {
        let obj = got.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort();
        let mut want_keys = KEYS.to_vec();
        want_keys.sort();
        assert_eq!(keys, want_keys, "keys of {}", want["id"]);
        for k in KEYS {
            assert_eq!(got[k].as_str(), want[k].as_str(), "{k} of {}", want["id"]);
        }
    }
    for (got, spec) in checks.iter().zip(catalogue().entries()) {
        assert_eq!(got["id"].as_str().unwrap(), spec.id.to_string());
    }
}

#[tokio::test]
async fn profile_3_answers_the_file_values_and_thresholds_highest_first() {
    let file: toml::Table =
        toml::from_str(include_str!("../../../engine/profiles/v3.toml")).unwrap();
    let (status, body) = get("/profiles/3").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "v3");
    assert_eq!(body["version"], 3);

    let want_sections = file["sections"].as_table().unwrap();
    let got_sections = body["sections"].as_object().unwrap();
    assert_eq!(got_sections.len(), want_sections.len());
    for (proto, want) in want_sections {
        let got = &got_sections[proto];
        assert_eq!(
            got["weight"].as_i64(),
            want["weight"].as_integer(),
            "{proto} weight"
        );
        let want_checks = want["checks"].as_table().unwrap();
        let got_checks = got["checks"].as_object().unwrap();
        assert_eq!(got_checks.len(), want_checks.len(), "{proto} checks");
        for (name, w) in want_checks {
            assert_eq!(
                got_checks[&format!("{proto}.{name}")].as_i64(),
                w.as_integer(),
                "{proto}.{name}"
            );
        }
        let want_hf: Vec<String> = want["hard_fail"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| format!("{proto}.{}", n.as_str().unwrap()))
            .collect();
        let got_hf: Vec<String> = got["hard_fail"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect();
        assert_eq!(got_hf, want_hf, "{proto} hard_fail");
    }

    assert_eq!(
        body["thresholds"],
        json!([
            {"grade": "A+", "min": 97},
            {"grade": "A", "min": 90},
            {"grade": "B", "min": 75},
            {"grade": "C", "min": 60},
            {"grade": "D", "min": 40},
            {"grade": "F", "min": 0},
        ])
    );
    // The loaded profile agrees on the shape it carries.
    let p = Profiles::get(3).unwrap();
    assert_eq!(got_sections.len(), p.sections.len());
}

#[tokio::test]
async fn profile_errors_use_the_error_shape() {
    let (status, body) = get("/profiles/99").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert!(body["error"]["message"].is_string());

    let (status, body) = get("/profiles/x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "INVALID_INPUT");
    assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn meta_reports_version_profiles_and_limits() {
    let (status, body) = get("/meta").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["api"], "v1");
    assert_eq!(body["current_profile"], 3);
    assert!(body["profiles"].as_array().unwrap().contains(&json!(3)));
    assert_eq!(
        body["evidence_schema_version"],
        netray_model::evidence::EVIDENCE_SCHEMA_VERSION
    );
    assert_eq!(
        body["limits"]["api"],
        json!({"per_ip_per_minute": 60, "per_ip_burst": 10})
    );
    assert_eq!(
        body["limits"]["admission"],
        json!({"client_budget_per_hour": 11, "domain_run_slots": 3})
    );
    assert_eq!(
        body["limits"]["rate_limit"],
        json!({"per_ip_per_minute": 21, "per_ip_burst": 22, "global_per_minute": 23, "global_burst": 24})
    );
}
