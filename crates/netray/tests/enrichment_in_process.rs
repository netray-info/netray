// v2-cutover requirement 28, scenario C15: the registry hands the modules an enrichment client
// that calls the registered ip module in-process. With the ip module present one run's server
// `org` and network type are filled and no base URL exists; with it absent they are absent and
// the run completes.
//
// Assumed API:
//   netray_ip::testing::stub_ip_module(ifconfig_json: &str) -> Arc<netray_ip::IpModule>
//       (feature `testing`): an IpModule that answers every address with the decoded ifconfig
//       response, no GeoIP data. `IpModule` implements `netray_common::enrichment::EnrichmentSource`.
//   netray_ip::enrichment_client(ip: Option<&Arc<IpModule>>) -> Option<Arc<EnrichmentClient>>
//       what `lens_registry` hands dns, tls, http and email: None without an ip module.
//   netray_http::HttpModule::new(config, enrichment: Option<Arc<EnrichmentClient>>)
//       (replaces building the client from `ip_url`; dns, tls, email constructors likewise).
// Needs netray-ip with feature `testing` as a dev-dependency of the netray crate.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use netray_common::enrichment::EnrichmentClient;
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, Module, RunContext, RunOptions, SectionOutcome,
};
use netray_model::evidence::{Block, BlockId, Shape, Value as EvValue};
use netray_model::{CheckId, Protocol};

const IFCONFIG_JSON: &str = include_str!("../../../tests/fixtures/contracts/ifconfig-json.json");

/// A section that reports one server address and enriches it through the client it was given,
/// as the http module's `http.server` block does.
struct ServerSection(Option<Arc<EnrichmentClient>>);

impl Module for ServerSection {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            let ip: std::net::IpAddr = Ipv4Addr::new(8, 8, 8, 8).into();
            let mut fields = vec![("address".to_string(), EvValue::Addr(ip))];
            if let Some(client) = &self.0
                && let Some(info) = client.lookup(ip, None).await
            {
                fields.extend(info.org.map(|o| ("org".to_string(), EvValue::Text(o))));
                fields.extend(
                    info.ip_type
                        .map(|t| ("network_type".to_string(), EvValue::Text(t))),
                );
            }
            SectionOutcome::Measured {
                checks: vec![],
                evidence: vec![Block {
                    id: BlockId::parse("http.server").unwrap(),
                    title: "Server".into(),
                    shape: Shape::Fields(fields),
                }],
            }
        })
    }
}

async fn server_fields(client: Option<Arc<EnrichmentClient>>) -> Vec<(String, EvValue)> {
    let ctx = RunContext {
        deadline: Instant::now() + Duration::from_secs(5),
        domain: Domain::new("example.com"),
        options: RunOptions::default(),
    };
    match ServerSection(client).run(&ctx, &Facts::default()).await {
        SectionOutcome::Measured { evidence, .. } => match &evidence[0].shape {
            Shape::Fields(f) => f.clone(),
            other => panic!("not a fields block: {other:?}"),
        },
        other => panic!("the run must complete, got {other:?}"),
    }
}

#[tokio::test]
async fn c15_enrichment_calls_the_registered_ip_module_in_process() {
    let ip = netray_ip::testing::stub_ip_module(IFCONFIG_JSON);
    let client = netray_ip::enrichment_client(Some(&ip)).expect("an ip module turns enrichment on");
    assert_eq!(client.base_url(), "", "no base URL is configured anywhere");
    // the http module accepts the client in place of an `ip_url`
    let http_config = toml::from_str("").unwrap();
    netray_http::HttpModule::new(http_config, Some(client.clone())).expect("http module builds");

    let fields = server_fields(Some(client)).await;
    let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(get("org"), Some(EvValue::Text("Example Cloud GmbH".into())));
    assert_eq!(get("network_type"), Some(EvValue::Text("cloud".into())));
}

#[tokio::test]
async fn c15_without_an_ip_module_enrichment_is_off_and_the_run_completes() {
    let client = netray_ip::enrichment_client(None);
    assert!(client.is_none(), "no ip module, no enrichment client");

    let fields = server_fields(client).await;
    assert!(fields.iter().any(|(n, _)| n == "address"));
    assert!(
        fields
            .iter()
            .all(|(n, _)| n != "org" && n != "network_type")
    );
}
