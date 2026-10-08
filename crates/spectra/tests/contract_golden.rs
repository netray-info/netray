// Golden test for the `GET /api/inspect?url=<url>` response lens consumes.
//
// The golden `tests/fixtures/contracts/spectra-inspect.json` is written here from spectra's
// real `InspectResponse` (built through its own `Deserialize`, so every field the type
// requires is present), serialized by axum's `Json` exactly as the route does.
// lens's `tests/contract_backends.rs` reads the same file.
//
// UPDATE_GOLDEN=1 cargo test -p spectra --test contract_golden   writes the golden.

use std::path::PathBuf;

use axum::Json;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use spectra::inspect::assembler::InspectResponse;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(name)
}

fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "golden {} is missing; run with UPDATE_GOLDEN=1 to write it",
            path.display()
        )
    });
    assert!(
        committed == actual,
        "golden {} differs from what spectra produces now; if the change is intended, run with UPDATE_GOLDEN=1 and commit the result",
        path.display()
    );
}

fn header_check(status: &str, value: Option<&str>, message: Option<&str>) -> serde_json::Value {
    let mut v = serde_json::json!({ "status": status });
    if let Some(value) = value {
        v["value"] = value.into();
    }
    if let Some(message) = message {
        v["message"] = message.into();
    }
    v
}

fn quality_check(
    name: &str,
    label: &str,
    status: &str,
    message: Option<&str>,
) -> serde_json::Value {
    let mut v = serde_json::json!({ "name": name, "label": label, "status": status });
    if let Some(message) = message {
        v["message"] = message.into();
    }
    v
}

fn response() -> InspectResponse {
    serde_json::from_value(serde_json::json!({
        "url": "https://example.com",
        "final_url": "https://example.com/",
        "timestamp": "2030-01-01T00:00:00Z",
        "duration_ms": 87,
        "http_version": "HTTP/2",
        "status": 200,
        "redirects": [],
        "http_upgrade": {
            "redirects_to_https": true,
            "status_code": 301,
            "same_host": true,
            "message": "http://example.com redirects to https",
            "redirects": []
        },
        "headers": {
            "content-type": "text/html",
            "strict-transport-security": "max-age=31536000"
        },
        "security": {
            "hsts": { "status": "pass", "max_age": 31536000, "include_sub_domains": false, "preload": false },
            "csp": { "status": "warn", "enforced": false, "report_only": false, "directives": {}, "issues": ["No Content-Security-Policy header"] },
            "x_frame_options": header_check("pass", Some("DENY"), None),
            "permissions_policy": header_check("skip", None, None),
            "x_content_type_options": header_check("pass", Some("nosniff"), None),
            "referrer_policy": header_check("pass", Some("no-referrer"), None),
            "coop": header_check("skip", None, None),
            "coep": header_check("skip", None, None),
            "corp": header_check("skip", None, None)
        },
        "cors": {
            "allows_any_origin": false,
            "reflects_origin": false,
            "allows_credentials": false,
            "status": "pass",
            "message": "No CORS headers"
        },
        "cookies": [],
        "caching": {
            "directives": {
                "public": false, "private": false, "no_store": false,
                "no_cache": false, "must_revalidate": false, "immutable": false
            },
            "vary": []
        },
        "cdn": { "indicators": [] },
        "fingerprint": {
            "info_leakage": { "status": "pass", "exposed_headers": [] }
        },
        "deprecated_headers": [],
        "reporting": { "report_to": false, "nel": false, "csp_reporting": false },
        "quality": {
            "verdict": "warn",
            "checks": [
                quality_check("hsts", "HSTS", "pass", None),
                quality_check("csp", "Content-Security-Policy", "warn", Some("No Content-Security-Policy header")),
                quality_check("x_frame_options", "X-Frame-Options", "pass", None),
                quality_check("cors", "CORS", "pass", None),
                quality_check("info_leakage", "Information leakage", "pass", None)
            ]
        },
        "enrichment": { "ip": "192.0.2.10", "org": "Example Hosting", "ip_type": "datacenter" }
    }))
    .expect("fixture must deserialize into spectra's InspectResponse")
}

#[tokio::test]
async fn spectra_inspect_response_matches_golden() {
    let resp = Json(response()).into_response();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut body = serde_json::to_string_pretty(&value).unwrap();
    body.push('\n');
    assert_golden("spectra-inspect.json", &body);
}
