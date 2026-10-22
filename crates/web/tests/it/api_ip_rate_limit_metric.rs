//! v2-cutover requirement 17: a refused `/api/v1/ip*` caller counts on `netray_rate_limit_hits_total`
//! with scope `api_v1_ip`; no `lens_` series is exported (S11).

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use futures::future::BoxFuture;
use metrics_util::debugging::{DebuggingRecorder, Snapshotter};
use netray_web::api::{ApiConfig, ApiState, DomainRefusal, DomainRuns, RunStart, RunTrigger};

struct NoRuns;

impl DomainRuns for NoRuns {
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

fn names(snapshotter: &Snapshotter) -> Vec<(String, Vec<String>)> {
    snapshotter
        .snapshot()
        .into_vec()
        .into_iter()
        .map(|(key, _, _, _)| {
            let key = key.key();
            (
                key.name().to_string(),
                key.labels()
                    .map(|l| format!("{}={}", l.key(), l.value()))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn a_refused_ip_caller_counts_on_the_netray_series() {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let state = ApiState::new(ApiConfig::default(), Arc::new(NoRuns));
    let client = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 7));
    let seen = metrics::with_local_recorder(&recorder, || {
        let limiters = state.ip_limiters();
        let mut refused = false;
        for _ in 0..1000 {
            if limiters.charge_per_ip(client, 1).is_err() {
                refused = true;
                break;
            }
        }
        assert!(refused, "the per-IP limiter never refused");
        names(&snapshotter)
    });
    assert!(
        seen.iter().all(|(name, _)| !name.starts_with("lens_")),
        "a lens_ series was recorded: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(name, labels)| name == "netray_rate_limit_hits_total"
                && labels.iter().any(|l| l == "scope=api_v1_ip")),
        "no netray_rate_limit_hits_total{{scope=\"api_v1_ip\"}}: {seen:?}"
    );
}
