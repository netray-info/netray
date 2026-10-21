//! The DNS inspector over the offline DNS stub (`netray_dns::testing`), for screenshots and the
//! visual reading of the DNS views without network: `cargo run -p netray-web --example dns_stub
//! [port]`, then open `/inspect/dns?q=diverge.example.com%20A%20@google%20@cloudflare`.
use std::net::SocketAddr;
use std::sync::Arc;

use futures::future::BoxFuture;
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::pages::{InspectConfig, InspectDeps, inspect_router};

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: std::net::IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(18_533);
    let config: ApiConfig = toml::from_str(
        "[dns]\nper_ip_per_minute = 10000\nper_ip_burst = 1000\nper_target_per_minute = 10000\nper_target_burst = 1000\nglobal_burst = 1000",
    )
        .expect("[api.dns] parses");
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        dns: Some(netray_dns::testing::stub_jobs()),
        ..Default::default()
    });
    let deps = InspectDeps::new(
        Arc::new(InspectJobs::new(state.clone())),
        InspectConfig::default(),
    )
    .with_dns_results(state.dns_results().clone());
    let app = netray_web::pages::router()
        .merge(inspect_router(deps))
        .nest("/api/v1", netray_web::api::router(state));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    println!("listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("serve");
}
