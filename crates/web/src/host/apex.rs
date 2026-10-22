//! The apex's routes and the inspector hosts' routers, composed the way lens composed its routes
//! (spec v2-cutover requirement 5) without the V1 parts: no SPA, no `/api/check` and `/api/meta`,
//! no V1 OpenAPI document or `/docs`, no snapshot routes and no V1 lookup on `/r/{id}`. An apex
//! path no route serves answers the pages' 404.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::response::Response;
use axum::routing::{any, get};
use netray_engine::Registry;
use netray_engine::profile::Profile;
use netray_model::Protocol;
use netray_store::HistoryStore;

use super::layers::{api_v1_not_found, security_headers_mw};
use crate::admission::{AdmittedRuns, ApexState, Runs};
use crate::api::{ApiParts, ApiState, InspectJobs};
use crate::config::ApexConfig;
use crate::metrics::ClientRunCounter;
use crate::pages::{InspectDeps, Tool};

/// What `netray serve` composes: the apex's routes for [`super::HostRouters::apex_router`], the
/// inspector hosts' routers, and what the serve loop flushes and drains.
pub struct Apex {
    pub apex_router: Router,
    pub dns: Router,
    pub tls: Router,
    pub http: Router,
    pub ip: Router,
    /// The API v1 state `ip.`'s command-line paths answer from ([`super::HostRouters::ip_api`]).
    pub api: ApiState,
    /// Flushed hourly into `netray_client_hourly_runs`.
    pub client_runs: Arc<ClientRunCounter>,
    /// Drained on shutdown (`Runs::drain`).
    pub runs: Arc<Runs>,
}

/// Builds the apex over `registry`: runs start the sections of `plan` with the resolve stage
/// under `resolve_budget`, scored with `profile`, recorded into `store` (`None`: no history, so
/// no result, entry and evidence pages); `/api/v1` calls `parts`. Binds nothing and spawns
/// nothing; the caller opens and migrates the store, runs its sweep and sets its gauges.
pub fn build(
    config: ApexConfig,
    registry: Registry,
    plan: Vec<(Protocol, Duration)>,
    resolve_budget: Duration,
    profile: Profile,
    store: Option<Arc<HistoryStore>>,
    parts: ApiParts,
) -> Apex {
    let mut state = ApexState::new(config, registry, plan, resolve_budget, profile);
    state.history_store = store;
    state.api_parts = parts;
    let api = api_state(&state);
    let inspect = inspect_deps(&state, api.clone());
    Apex {
        apex_router: routes_with(state.clone(), inspect.clone(), api.clone()),
        dns: crate::pages::inspect_host_router(Tool::Dns, inspect.clone()),
        tls: crate::pages::inspect_host_router(Tool::Tls, inspect.clone()),
        http: crate::pages::inspect_host_router(Tool::Http, inspect.clone()),
        ip: crate::pages::inspect_host_router(Tool::Ip, inspect),
        api,
        client_runs: Arc::clone(&state.client_runs),
        runs: Arc::clone(&state.runs),
    }
}

/// The API v1 state over `state`: its runs, its store, its extractor, limits, parts and profile.
pub fn api_state(state: &ApexState) -> ApiState {
    let config = &state.config;
    ApiState::new(
        config.api.clone(),
        Arc::new(AdmittedRuns::new(state.clone())),
    )
    .with_store(state.history_store.clone())
    .with_ip_extractor(state.ip_extractor.clone())
    .with_admission_limits(config.limits)
    .with_parts(state.api_parts.clone())
    .with_profile(state.scoring_profile.clone())
}

/// The inspector pages' dependencies over `state` and `api`, with the trusted-proxy extractor;
/// the apex and the inspector hosts share them, slots included. Pass the `ApiState` `/api/v1`
/// is built over: its clones share the per-tool limiters (v2-inspectors requirement 14).
pub fn inspect_deps(state: &ApexState, api: ApiState) -> InspectDeps {
    let dns_results = api.dns_results().clone();
    InspectDeps::new(
        Arc::new(InspectJobs::new(api)),
        state.config.inspect.clone(),
    )
    .with_ip_extractor(state.ip_extractor.clone())
    .with_history(state.history_store.clone())
    .with_dns_results(dns_results)
}

/// The apex's routes over `state` with the `http_metrics` layer, which needs the `MatchedPath`
/// only these routes set, and without the outer layer stack ([`super::layers::layers`]).
pub fn routes(state: ApexState) -> Router {
    let api = api_state(&state);
    let inspect = inspect_deps(&state, api.clone());
    routes_with(state, inspect, api)
}

/// [`routes`] inside the outer layer stack: the whole apex as one service.
pub fn app(state: ApexState) -> Router {
    let api_enabled = state.config.api.enabled;
    super::layers::layers(routes(state), api_enabled)
}

fn routes_with(state: ApexState, inspect: InspectDeps, api: ApiState) -> Router {
    let config = state.config.clone();

    let (entry_routes, result_routes) = match &state.history_store {
        Some(history) => {
            let entries = crate::entries::router(crate::entries::EntriesDeps {
                history: history.clone(),
                card: crate::pages::og::entry_card_renderer(&state),
            })
            .merge(crate::pages::evidence_router(crate::pages::EvidenceDeps {
                history: history.clone(),
            }));
            let result = crate::pages::result::router(
                crate::pages::result::ResultState::new(
                    history.clone(),
                    Arc::new(AdmittedRuns::new(state.clone())),
                    config.cache_window_seconds,
                )
                .with_ip_extractor(state.ip_extractor.clone()),
            )
            .layer(axum::middleware::map_response(
                |mut response: Response| async move {
                    response
                        .headers_mut()
                        .remove("cross-origin-resource-policy");
                    response
                },
            ));
            (entries, result)
        }
        None => (Router::new(), Router::new()),
    };

    let api_routes = if config.api.enabled {
        Router::new()
            .nest("/api/v1", crate::api::router(api))
            .route("/api/v1/", any(api_v1_not_found))
    } else {
        Router::new()
    };

    let pages = Router::new()
        .route("/robots.txt", get(crate::pages::robots))
        .fallback(crate::pages::not_found)
        .merge(crate::pages::router())
        .layer(axum::middleware::from_fn_with_state(
            false,
            security_headers_mw,
        ));

    Router::new()
        .merge(if config.badges.enabled {
            crate::pages::badge::router(state.clone())
        } else {
            Router::new()
        })
        .merge(if config.og_cards.enabled {
            crate::pages::og::router(state.clone())
        } else {
            Router::new()
        })
        .merge(crate::pages::inspect_router(inspect))
        .merge(pages)
        .merge(entry_routes)
        .merge(result_routes)
        .merge(crate::entries::privacy_router())
        .merge(api_routes)
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("netray", req, next)
        }))
}
