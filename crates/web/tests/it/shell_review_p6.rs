//! Review findings on v2-inspector-shell Phase 6 (requirements 21, 22): links follow the row's
//! record type (NS glue addresses go to the IP inspector, CNAME rows in any block to the DNS
//! inspector), and the network-type chip comes from the infrastructure, not the threat
//! classification.
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use futures::future::BoxFuture;
use netray_ip::backend::Ifconfig;
use netray_ip::testing::StubIpLookup;
use netray_ip::{IpLookup, IpRefusal};
use netray_model::evidence::{BlockId, Value};
use netray_web::api::{
    ApiConfig, ApiParts, ApiState, DomainRefusal, DomainRuns, InspectJobs, RunStart, RunTrigger,
};
use netray_web::evidence::{Badge, BadgeTone};
use netray_web::links::value_link;
use netray_web::pages::InspectorJobs;

struct NoRuns;

impl DomainRuns for NoRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        Ok(domain.to_string())
    }
    fn start<'a>(
        &'a self,
        _: &'a str,
        _: IpAddr,
        _: RunTrigger,
        _: bool,
    ) -> BoxFuture<'a, RunStart> {
        unreachable!("no run is started by the inspector")
    }
}

/// Answers every address as an Example Org Tor exit in a hosting network.
struct Hosted(StubIpLookup);

impl IpLookup for Hosted {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Result<Ifconfig, IpRefusal>> {
        Box::pin(async move {
            let mut i = self.0.lookup(ip).await?;
            i.network.org = Some("Example Org".into());
            // The module's single classification names the threat; the infrastructure is apart.
            i.network.network_type = "tor".into();
            i.network.infra_type = "datacenter".into();
            i.network.is_tor = true;
            i.network.is_vpn = false;
            i.network.is_spamhaus = false;
            i.network.is_c2 = false;
            Ok(i)
        })
    }
    fn lookup_caller<'a>(
        &'a self,
        remote: SocketAddr,
        ua: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Ifconfig, IpRefusal>> {
        self.0.lookup_caller(remote, ua)
    }
    fn range<'a>(
        &'a self,
        cidr: &'a str,
    ) -> BoxFuture<'a, Result<netray_ip::lookup::RangeResponse, IpRefusal>> {
        self.0.range(cidr)
    }
    fn asn(
        &self,
        n: u32,
    ) -> BoxFuture<'_, Result<netray_ip::lookup::AsnLookupResponse, IpRefusal>> {
        self.0.asn(n)
    }
}

fn state(ip: bool) -> ApiState {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
    ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        ip: ip.then(|| Arc::new(Hosted(StubIpLookup::new())) as Arc<dyn IpLookup>),
        ..Default::default()
    })
}

#[test]
fn links_follow_the_rows_record_type() {
    let ns = BlockId::parse("dns.ns").unwrap();
    let a = BlockId::parse("dns.a").unwrap();
    let text = |s: &str| Value::Text(s.into());
    assert_eq!(
        value_link(&ns, "data:A", &text("192.0.2.10")).as_deref(),
        Some("/inspect/ip?ip=192.0.2.10")
    );
    assert_eq!(
        value_link(&ns, "data:AAAA", &text("2001:db8::a")).as_deref(),
        Some("/inspect/ip?ip=2001%3Adb8%3A%3Aa")
    );
    assert_eq!(
        value_link(&ns, "data:NS", &text("ns1.example.com.")).as_deref(),
        Some("/inspect/dns?q=ns1.example.com")
    );
    assert_eq!(
        value_link(&a, "data:CNAME", &text("example.com.")).as_deref(),
        Some("/inspect/dns?q=example.com")
    );
    assert_eq!(
        value_link(&a, "data:MX", &text("10 mx.example.com.")).as_deref(),
        Some("/inspect/dns?q=mx.example.com")
    );
}

#[tokio::test]
async fn the_network_chip_names_the_infrastructure_beside_the_threat() {
    let config: ApiConfig = toml::from_str("[ip]\ninternal_mode = true\n").unwrap();
    let state = ApiState::new(config, Arc::new(NoRuns)).with_parts(ApiParts {
        ip: Some(Arc::new(Hosted(StubIpLookup::new())) as Arc<dyn IpLookup>),
        ..Default::default()
    });
    let jobs = InspectJobs::new(state);
    let addr: IpAddr = "192.0.2.24".parse().unwrap();
    let got = jobs.enrich(vec![addr]).await;
    let labels: Vec<(String, BadgeTone)> = got
        .iter()
        .find(|(a, _)| *a == addr)
        .map(|(_, b)| {
            b.iter()
                .map(|b: &Badge| (b.label.clone(), b.tone.clone()))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        labels,
        vec![
            ("Example Org".to_string(), BadgeTone::Muted),
            ("hosting".to_string(), BadgeTone::Muted),
            ("Tor".to_string(), BadgeTone::Fail),
        ]
    );
}
