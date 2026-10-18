// v2-cutover requirement 28: `EnrichmentClient::in_process` answers from an `EnrichmentSource`
// with no HTTP request. Assumed API (netray_common::enrichment, feature `enrichment`):
//   pub trait EnrichmentSource: Send + Sync {
//       fn lookup(&self, ip: IpAddr) -> futures::future::BoxFuture<'_, Option<IpInfo>>;
//   }
//   EnrichmentClient::in_process(source: Arc<dyn EnrichmentSource>) -> EnrichmentClient
// Needs `[[test]] name = "enrichment_in_process"` with `required-features = ["enrichment"]`.

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::BoxFuture;
use netray_common::enrichment::{EnrichmentClient, EnrichmentSource, IpInfo};

struct Counting {
    calls: AtomicUsize,
}

impl EnrichmentSource for Counting {
    fn lookup(&self, ip: IpAddr) -> BoxFuture<'_, Option<IpInfo>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if ip == "8.8.8.8".parse::<IpAddr>().unwrap() {
                Some(IpInfo {
                    asn: Some(64496),
                    org: Some("Example Cloud GmbH".into()),
                    ip_type: Some("cloud".into()),
                    cloud: None,
                    is_tor: false,
                    is_vpn: false,
                    is_datacenter: true,
                    is_spamhaus: false,
                    is_c2: false,
                    network_role: None,
                })
            } else {
                None
            }
        })
    }
}

#[tokio::test]
async fn in_process_client_answers_from_its_source_without_a_base_url() {
    let source = Arc::new(Counting {
        calls: AtomicUsize::new(0),
    });
    let client = EnrichmentClient::in_process(source.clone());

    assert_eq!(
        client.base_url(),
        "",
        "an in-process client has no base URL"
    );

    let info = client
        .lookup("8.8.8.8".parse().unwrap(), None)
        .await
        .expect("the source's answer is returned");
    assert_eq!(info.org.as_deref(), Some("Example Cloud GmbH"));
    assert_eq!(info.ip_type.as_deref(), Some("cloud"));
    assert_eq!(info.asn, Some(64496));
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);

    // An address the source does not know stays absent.
    assert!(
        client
            .lookup("1.1.1.1".parse().unwrap(), None)
            .await
            .is_none()
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);

    // A private address is never handed to the source.
    assert!(
        client
            .lookup("10.0.0.1".parse().unwrap(), None)
            .await
            .is_none()
    );
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);

    // The batch path goes through the same source.
    let batch = client
        .lookup_batch(&["8.8.8.8".parse().unwrap()], None)
        .await;
    assert_eq!(batch.len(), 1);
    assert_eq!(source.calls.load(Ordering::SeqCst), 3);
}
