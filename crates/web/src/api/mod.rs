//! API v1 (spec v2-api): the typed, documented read surface of the suite. `router` is mounted
//! at `/api/v1` by lens; `openapi()` is the document it serves.

mod compose;
mod config;
pub mod contract;
mod dns;
mod docs;
mod email;
mod error;
mod inspectors;
mod ip;
mod ip_host;
mod jobs;
mod limit;
mod reads;
mod runs;
mod state;

use axum::extract::rejection::PathRejection;
use axum::extract::{MatchedPath, Path, Request, State};
use axum::middleware::{self, Next};
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
use ip::{AddressField, CallerField, Format};
pub use ip_host::ip_host;
pub use jobs::InspectJobs;
pub(crate) use reads::entry_json;
pub(crate) use runs::wants_event_stream;
pub use runs::{
    DomainRefusal, DomainRuns, RefusalReason, RunEvent, RunEvents, RunRefusal, RunStart, RunTrigger,
};
use runs::{RunAnswer, RunSectionEvent};
pub use state::{AdmissionLimits, ApiParts, ApiState, IpLimiters};

const MOUNT: &str = "/api/v1";

#[derive(OpenApi)]
#[openapi(components(schemas(
    ErrorResponse,
    RunAnswer,
    RunSectionEvent,
    AddressField,
    CallerField,
    Format
)))]
struct ApiDoc;

fn read_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new()
        .routes(routes!(catalogue_handler))
        .routes(routes!(profile_handler))
        .routes(routes!(meta_handler))
        .routes(routes!(reads::domain_entry))
        .routes(routes!(reads::domain_changes))
        .routes(routes!(reads::domain_history))
        .routes(routes!(reads::entry_read))
        .routes(routes!(reads::entry_changes))
}

fn run_routes() -> OpenApiRouter<ApiState> {
    OpenApiRouter::new().routes(routes!(runs::start_run))
}

/// The generated v1 document; path keys carry the `/api/v1` mount.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (_, routed) = read_routes()
        .merge(run_routes())
        .merge(ip::ip_doc_routes())
        .split_for_parts();
    let (_, tls) = inspectors::tls_doc_routes().split_for_parts();
    let (_, http) = inspectors::http_doc_routes().split_for_parts();
    let mut base = ApiDoc::openapi();
    base.merge(routed);
    let composed = compose::compose(
        base,
        vec![("Tls", tls), ("Http", inspectors::typed_http_maps(http))],
    );
    let (_, email) = email::email_doc_routes().split_for_parts();
    let composed = compose::compose(composed, vec![("Email", email)]);
    let (_, dns) = dns::dns_doc_routes()
        .merge(dns::dns_read_routes())
        .split_for_parts();
    let composed = compose::compose(composed, vec![("Dns", dns)]);
    let mut doc = ApiDoc::openapi().nest(MOUNT, composed);
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
    let (limited, _) = read_routes()
        .merge(dns::dns_read_routes())
        .split_for_parts();
    let (unlimited, _) = run_routes().split_for_parts();
    limited
        .route_layer(middleware::from_fn_with_state(state.clone(), limit::per_ip))
        .merge(unlimited)
        .merge(ip::ip_routes(state.clone()))
        .merge(inspectors::tls_doc_routes().split_for_parts().0)
        .merge(inspectors::http_doc_routes().split_for_parts().0)
        .merge(email::email_doc_routes().split_for_parts().0)
        .merge(dns::dns_doc_routes().split_for_parts().0)
        .route("/openapi.json", get(|| async { Json(openapi()) }))
        .route("/docs", get(docs::page))
        .route("/", any(not_found))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .route_layer(middleware::from_fn(count_request))
        .with_state(state)
}

/// Counts every answer of a matched route by its template; unmatched paths reach no route and
/// count nothing, so no label ever carries a request value.
async fn count_request(req: Request, next: Next) -> Response {
    let template = req.extensions().get::<MatchedPath>().map(|m| {
        let path = m.as_str();
        if path.starts_with(MOUNT) {
            path.to_owned()
        } else {
            format!("{MOUNT}{}", path.trim_end_matches('/'))
        }
    });
    let response = next.run(req).await;
    if let Some(endpoint) = template {
        metrics::counter!("netray_api_requests_total", "endpoint" => endpoint).increment(1);
    }
    response
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
    /// The MaxMind GeoLite2 attribution text the IP data requires next to its use.
    attribution: String,
    limits: Limits,
}

#[derive(Serialize, ToSchema)]
struct Limits {
    api: ApiLimits,
    admission: AdmissionMeta,
    rate_limit: RateLimitMeta,
    ip: IpMeta,
    tls: InspectorMeta,
    http: InspectorMeta,
    email: EmailMeta,
    dns: DnsMeta,
}

#[derive(Serialize, ToSchema)]
struct DnsMeta {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
    per_target_per_minute: u32,
    per_target_burst: u32,
    global_per_minute: u32,
    global_burst: u32,
    per_ip_max_streams: u32,
}

#[derive(Serialize, ToSchema)]
struct EmailMeta {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
    max_concurrent_inspections: u32,
}

#[derive(Serialize, ToSchema)]
struct InspectorMeta {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
    per_target_per_minute: u32,
    per_target_burst: u32,
}

#[derive(Serialize, ToSchema)]
struct ApiLimits {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
}

#[derive(Serialize, ToSchema)]
struct IpMeta {
    per_ip_per_minute: u32,
    per_ip_burst: u32,
    per_target_per_minute: u32,
    per_target_burst: u32,
    batch_max_size: u32,
    batch_addresses_per_minute: u32,
    batch_addresses_burst: u32,
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
        attribution: netray_ip::GEOIP_ATTRIBUTION.into(),
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
            ip: IpMeta {
                per_ip_per_minute: config.ip.per_ip_per_minute,
                per_ip_burst: config.ip.per_ip_burst,
                per_target_per_minute: config.ip.per_target_per_minute,
                per_target_burst: config.ip.per_target_burst,
                batch_max_size: config.ip.batch_max_size,
                batch_addresses_per_minute: config.ip.batch_addresses_per_minute,
                batch_addresses_burst: config.ip.batch_addresses_burst,
            },
            tls: InspectorMeta {
                per_ip_per_minute: config.tls.per_ip_per_minute,
                per_ip_burst: config.tls.per_ip_burst,
                per_target_per_minute: config.tls.per_target_per_minute,
                per_target_burst: config.tls.per_target_burst,
            },
            http: InspectorMeta {
                per_ip_per_minute: config.http.per_ip_per_minute,
                per_ip_burst: config.http.per_ip_burst,
                per_target_per_minute: config.http.per_target_per_minute,
                per_target_burst: config.http.per_target_burst,
            },
            email: EmailMeta {
                per_ip_per_minute: config.email.per_ip_per_minute,
                per_ip_burst: config.email.per_ip_burst,
                max_concurrent_inspections: config.email.max_concurrent_inspections,
            },
            dns: DnsMeta {
                per_ip_per_minute: config.dns.per_ip_per_minute,
                per_ip_burst: config.dns.per_ip_burst,
                per_target_per_minute: config.dns.per_target_per_minute,
                per_target_burst: config.dns.per_target_burst,
                global_per_minute: config.dns.global_per_minute,
                global_burst: config.dns.global_burst,
                per_ip_max_streams: config.dns.per_ip_max_streams,
            },
        },
    })
}
