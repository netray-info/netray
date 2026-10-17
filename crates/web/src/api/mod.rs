//! API v1 (spec v2-api): the typed, documented read surface of the suite. `router` is mounted
//! at `/api/v1` by lens; `openapi()` is the document it serves.

mod config;
pub mod contract;
mod docs;
mod error;
mod limit;
mod runs;
mod state;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use netray_common::error::ErrorResponse;
use netray_engine::catalogue::{CheckSpec, catalogue};
use netray_engine::profile::Profile;
use netray_model::evidence::EVIDENCE_SCHEMA_VERSION;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub use config::{
    ApiConfig, ApiDnsConfig, ApiEmailConfig, ApiHttpConfig, ApiIpConfig, ApiTlsConfig,
};
pub use error::V1Error;
pub use runs::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunEvents, RunRefusal, RunStart, RunTrigger,
};
pub use state::{AdmissionLimits, ApiParts, ApiState};

const MOUNT: &str = "/api/v1";

#[derive(OpenApi)]
#[openapi(components(schemas(ErrorResponse)))]
struct ApiDoc;

fn read_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(catalogue_handler))
        .routes(routes!(profile_handler))
        .routes(routes!(meta_handler))
}

/// The generated v1 document; path keys carry the `/api/v1` mount.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, routed) = read_routes().split_for_parts();
    let mut doc = ApiDoc::openapi().nest(MOUNT, routed);
    doc.info.title = "netray API".into();
    doc.info.version = env!("CARGO_PKG_VERSION").into();
    doc.info.description = None;
    doc
}

/// Routes relative to the mount. With `[api] enabled = false` the router has no routes.
pub fn router(state: ApiState) -> Router {
    if !state.config().enabled {
        return Router::new();
    }
    let (limited, _) = read_routes().split_for_parts();
    limited
        .route_layer(middleware::from_fn_with_state(state.clone(), limit::per_ip))
        .route("/openapi.json", get(|| async { Json(openapi()) }))
        .route("/docs", get(docs::page))
        .route("/", any(not_found))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .with_state(state)
}

async fn not_found() -> Response {
    V1Error::NotFound("no such endpoint".into()).into_response()
}

async fn method_not_allowed() -> Response {
    V1Error::MethodNotAllowed.into_response()
}

#[derive(Serialize, ToSchema)]
struct CatalogueResponse {
    checks: Vec<CheckSpec>,
}

/// Every check of the catalogue, in file order.
#[utoipa::path(
    get,
    path = "/catalogue",
    tag = "reference",
    responses(
        (status = 200, description = "The catalogue", body = CatalogueResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
async fn catalogue_handler() -> Json<CatalogueResponse> {
    Json(CatalogueResponse {
        checks: catalogue().entries().to_vec(),
    })
}

/// One scoring profile.
#[utoipa::path(
    get,
    path = "/profiles/{n}",
    tag = "reference",
    params(("n" = String, Path, description = "Profile version, an integer")),
    responses(
        (status = 200, description = "The profile", body = Profile),
        (status = 400, description = "Not an integer", body = ErrorResponse),
        (status = 404, description = "Unknown profile version", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
async fn profile_handler(
    State(state): State<ApiState>,
    n: Result<Path<String>, PathRejection>,
) -> Result<Response, V1Error> {
    let Path(n) = n.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return Err(V1Error::InvalidInput(format!(
            "profile version `{n}` is not an integer"
        )));
    }
    n.parse::<u32>()
        .ok()
        .and_then(|version| state.profile(version))
        .map(|profile| Json(profile).into_response())
        .ok_or_else(|| V1Error::NotFound(format!("no profile version {n}")))
}

#[derive(Serialize, ToSchema)]
struct Meta {
    version: String,
    api: String,
    current_profile: u32,
    profiles: Vec<u32>,
    evidence_schema_version: u32,
    limits: Limits,
}

#[derive(Serialize, ToSchema)]
struct Limits {
    api: ApiLimits,
    admission: AdmissionMeta,
    rate_limit: RateLimitMeta,
}

#[derive(Serialize, ToSchema)]
struct ApiLimits {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
}

#[derive(Serialize, ToSchema)]
struct AdmissionMeta {
    client_budget_per_hour: u32,
    domain_run_slots: u32,
}

#[derive(Serialize, ToSchema)]
struct RateLimitMeta {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
    global_per_minute: u32,
    global_burst: u32,
}

/// Version, profiles and the limits this deployment runs with.
#[utoipa::path(
    get,
    path = "/meta",
    tag = "reference",
    responses(
        (status = 200, description = "Deployment metadata", body = Meta),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
    )
)]
async fn meta_handler(State(state): State<ApiState>) -> Json<Meta> {
    let admission = state.admission();
    let config = state.config();
    Json(Meta {
        version: env!("CARGO_PKG_VERSION").into(),
        api: "v1".into(),
        current_profile: state.current_profile(),
        profiles: state.profile_versions(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        limits: Limits {
            api: ApiLimits {
                per_ip_per_minute: config.per_ip_per_minute,
                per_ip_burst: config.per_ip_burst,
            },
            admission: AdmissionMeta {
                client_budget_per_hour: admission.client_budget_per_hour,
                domain_run_slots: admission.domain_run_slots,
            },
            rate_limit: RateLimitMeta {
                per_ip_per_minute: admission.rate_limit_per_ip_per_minute,
                per_ip_burst: admission.rate_limit_per_ip_burst,
                global_per_minute: admission.rate_limit_global_per_minute,
                global_burst: admission.rate_limit_global_burst,
            },
        },
    })
}
