// V2 Phase 5, requirements 27, 28, 31 (C5 crate half, C12, C14): the TLS inspection type called
// directly with a stub backend and no axum type. Needs `required-features = ["testing"]` on this test target.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::http::StatusCode;
use http_body_util::BodyExt;
use netray_engine::SectionOutcome;
use netray_model::evidence::Block;
use netray_tls::security::RateLimitState;
use netray_tls::security::rate_limit::{Quotas, Scopes};
use netray_tls::testing::{StubTlsBackend, golden_module, stub_config, stub_state};
use netray_tls::{TlsInspectRequest, TlsRefusal, translate};
use serde_json::Value;

const GOLDEN: &str = include_str!("../../../tests/fixtures/contracts/tlsight-inspect.json");

fn caller() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))
}

/// tlsight's old default quotas: 30 per minute with a burst of 10 per IP, 60 and 20 per target.
fn limits() -> RateLimitState {
    RateLimitState::scoped(
        &Quotas {
            per_ip_per_minute: 30,
            per_ip_burst: 10,
            per_target_per_minute: 60,
            per_target_burst: 20,
        },
        Scopes {
            metrics_prefix: "tlsight",
            per_ip: "per_ip",
            per_target: "per_target",
        },
    )
}

fn req(host: &str, port: u16) -> TlsInspectRequest {
    TlsInspectRequest {
        host: host.into(),
        port: Some(port),
        ports: None,
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[tokio::test]
async fn example_com_inspects_port_443() {
    let stub = Arc::new(StubTlsBackend::new());
    let inspector = stub_state(stub.clone()).inspector();
    let resp = inspector
        .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
        .await
        .expect("inspection answers");
    assert_eq!(resp.hostname, "example.com");
    assert_eq!(resp.ports.len(), 1);
    assert_eq!(resp.ports[0].port, 443);
    assert_eq!(
        stub.calls(),
        vec![(ip("93.184.215.14"), 443, Some("example.com".to_string()))]
    );
}

#[tokio::test]
async fn mail_host_on_port_25_runs_smtp_starttls() {
    let stub = Arc::new(StubTlsBackend::new());
    let inspector = stub_state(stub.clone()).inspector();
    let resp = inspector
        .inspect(
            &req("mail.example.com", 25),
            caller(),
            &limits(),
            "rid".into(),
        )
        .await
        .expect("inspection answers");
    assert_eq!(
        stub.calls(),
        vec![(
            ip("93.184.215.14"),
            25,
            Some("mail.example.com".to_string())
        )]
    );
    let v = serde_json::to_value(&resp).unwrap();
    assert_eq!(v["ports"][0]["port"], 25);
    assert_eq!(v["ports"][0]["ips"][0]["tls"]["starttls"], "smtp");
}

#[tokio::test]
async fn result_translates_to_measured_tls_checks_with_evidence() {
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    let resp = inspector
        .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
        .await
        .unwrap();
    let (checks, evidence): (_, Vec<Block>) = match translate(&resp) {
        SectionOutcome::Measured { checks, evidence } => (checks, evidence),
        other => panic!("expected Measured, got {other:?}"),
    };
    let checks: Vec<netray_model::CheckResult> = checks;
    let got: Vec<String> = checks.iter().map(|c| c.id.to_string()).collect();
    let expected: Vec<String> = golden_module(GOLDEN)
        .checks()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert!(!expected.is_empty());
    assert!(expected.iter().all(|id| id.starts_with("tls.")));
    for id in &expected {
        assert!(got.contains(id), "missing check {id}; got {got:?}");
    }
    assert!(!evidence.is_empty(), "at least one evidence block");
}

#[tokio::test]
async fn localhost_is_invalid_input() {
    let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
    let err = inspector
        .inspect(&req("localhost", 443), caller(), &limits(), "rid".into())
        .await
        .unwrap_err();
    assert!(matches!(err, TlsRefusal::InvalidInput(_)), "{err:?}");
}

#[tokio::test]
async fn private_literal_and_private_resolution_are_blocked() {
    let stub =
        Arc::new(StubTlsBackend::new().with_host("internal.example.com", vec![ip("10.0.0.5")]));
    let inspector = stub_state(stub.clone()).inspector();
    for host in ["10.0.0.1", "internal.example.com"] {
        let err = inspector
            .inspect(&req(host, 443), caller(), &limits(), "rid".into())
            .await
            .unwrap_err();
        assert!(matches!(err, TlsRefusal::Blocked(_)), "{host}: {err:?}");
    }
    assert!(stub.calls().is_empty(), "no handshake to a blocked address");
}

// ---------------------------------------------------------------------------
// v2-input-edges Phase 2 (req. 8; C3, C11, C12, C13): a cost above the per-IP burst is an
// input error, never a rate limit
// ---------------------------------------------------------------------------

mod cost_limit {
    use super::*;
    use axum::response::IntoResponse;
    use netray_tls::error::AppError;

    fn limiter(per_ip_burst: u32, per_target_burst: u32) -> RateLimitState {
        RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: per_ip_burst,
                per_ip_burst,
                per_target_per_minute: 20,
                per_target_burst,
            },
            Scopes {
                metrics_prefix: "tlsight",
                per_ip: "per_ip",
                per_target: "per_target",
            },
        )
    }

    #[test]
    fn c11_cost_above_the_per_ip_burst_is_refused_and_charges_nothing() {
        let limits = limiter(2, 20);
        let err = limits.check_cost(caller(), "example.com", 3).unwrap_err();
        assert!(
            matches!(err, AppError::CostExceedsLimit { cost: 3, limit: 2 }),
            "{err:?}"
        );
        assert!(limits.check_cost(caller(), "example.com", 2).is_ok());
    }

    #[test]
    fn c12_remaining_budget_is_capped_by_the_per_target_burst() {
        assert_eq!(limiter(10, 4).remaining_budget(caller()), 4);
    }

    #[tokio::test]
    async fn c13_cost_exceeds_limit_answers_422_without_retry_after() {
        let resp = AppError::CostExceedsLimit { cost: 3, limit: 2 }.into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!resp.headers().contains_key("retry-after"));
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "COST_EXCEEDS_LIMIT");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains('3') && message.contains('2'), "{message}");

        let refusal = TlsRefusal::from(AppError::CostExceedsLimit { cost: 3, limit: 2 });
        assert!(
            matches!(refusal, TlsRefusal::InvalidInput(_)),
            "{refusal:?}"
        );
    }

    #[tokio::test]
    async fn c3_three_ports_over_a_burst_of_two_is_invalid_input_not_rate_limited() {
        let inspector = stub_state(Arc::new(StubTlsBackend::new())).inspector();
        let request = TlsInspectRequest {
            host: "example.com".into(),
            port: None,
            ports: Some(vec![443, 8443, 993]),
        };
        let err = inspector
            .inspect(&request, caller(), &limiter(2, 20), "rid".into())
            .await
            .unwrap_err();
        match err {
            TlsRefusal::InvalidInput(message) => {
                assert!(message.contains('3') && message.contains('2'), "{message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// specs/features/v2-tls-versions-coop Phase 2: probes in the inspection (C1/R4, C4, C8, C10)
// ---------------------------------------------------------------------------

mod probes_phase2 {
    use super::*;
    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};

    fn outcome(verdict: Verdict, detail: &str) -> ProbeOutcome {
        ProbeOutcome {
            verdict,
            version: None,
            cipher_suite: None,
            detail: detail.to_string(),
        }
    }

    fn canned() -> ProtocolProbes {
        ProtocolProbes {
            ssl3: outcome(Verdict::Refused, "handshake_failure alert"),
            tls1_0: outcome(Verdict::Accepted, "ServerHello TLSv1.0"),
            tls1_1: outcome(Verdict::Refused, "protocol_version alert"),
            tls1_2: outcome(Verdict::Accepted, "ServerHello TLSv1.2"),
            tls1_3: outcome(Verdict::Accepted, "ServerHello TLSv1.3"),
            suites_insecure: outcome(Verdict::Refused, "handshake_failure alert"),
            suites_cbc: outcome(Verdict::Refused, "handshake_failure alert"),
            suites_static_rsa: outcome(Verdict::Refused, "handshake_failure alert"),
        }
    }

    fn two_addresses() -> (IpAddr, IpAddr) {
        (ip("93.184.215.14"), ip("93.184.215.15"))
    }

    fn block_ids(resp: &netray_tls::inspection::InspectResponse) -> Vec<String> {
        match translate(resp) {
            SectionOutcome::Measured { evidence, .. } => {
                let evidence: Vec<Block> = evidence;
                evidence.iter().map(|b| b.id.to_string()).collect()
            }
            other => panic!("expected Measured, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn c1_c10_probe_runs_once_for_the_address_whose_handshake_succeeded() {
        let (good, bad) = two_addresses();
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![good, bad])
                .with_failed_handshake(bad)
                .with_probes(canned()),
        );
        let inspector = stub_state(stub.clone()).inspector();
        let resp = inspector
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        let ips = &resp.ports[0].ips;
        let good_res = ips.iter().find(|r| r.ip == good.to_string()).unwrap();
        let bad_res = ips.iter().find(|r| r.ip == bad.to_string()).unwrap();
        assert!(
            bad_res.error.is_some(),
            "the failed address carries its error"
        );
        let version = good_res
            .tls
            .as_ref()
            .expect("tls on success")
            .version
            .clone();
        assert_eq!(stub.probe_calls(), vec![(good, 443, version)]);
        assert_eq!(good_res.probes, Some(canned()));
        assert_eq!(bad_res.probes, None);

        let ids = block_ids(&resp);
        assert!(
            ids.iter().any(|id| id.starts_with("tls.probes@")),
            "no tls.probes block in {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "tls.protocols"),
            "no tls.protocols block in {ids:?}"
        );
    }

    #[tokio::test]
    async fn c8_without_canned_probes_no_probe_block_is_translated() {
        let (good, other) = two_addresses();
        let stub = Arc::new(StubTlsBackend::new().with_host("example.com", vec![good, other]));
        let inspector = stub_state(stub.clone()).inspector();
        let resp = inspector
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        assert_eq!(resp.ports[0].ips.len(), 2);
        assert!(resp.ports[0].ips.iter().all(|r| r.probes.is_none()));
        assert_eq!(stub.probe_calls().len(), 2, "the probe is still asked");
        let ids = block_ids(&resp);
        assert!(
            !ids.iter()
                .any(|id| id.starts_with("tls.probes") || id.starts_with("tls.protocols")),
            "probe blocks in {ids:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// specs/features/v2-tls-probe-close Phase 2 (requirements 4, 6, 9): probe connections are
// charged; probes are capped, never refused
// ---------------------------------------------------------------------------

mod probe_cost {
    use super::*;
    use netray_tls::backend::TlsBackend;
    use netray_tls::security::rate_limit::{Quotas, Scopes};
    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};

    fn refused() -> ProbeOutcome {
        ProbeOutcome {
            verdict: Verdict::Refused,
            version: None,
            cipher_suite: None,
            detail: "alert protocol_version".into(),
        }
    }

    fn canned() -> ProtocolProbes {
        ProtocolProbes {
            ssl3: refused(),
            tls1_0: refused(),
            tls1_1: refused(),
            tls1_2: refused(),
            tls1_3: refused(),
            suites_insecure: refused(),
            suites_cbc: refused(),
            suites_static_rsa: refused(),
        }
    }

    /// Per-IP burst `burst`, refilled one per minute; per-target out of the way.
    fn limiter(burst: u32) -> RateLimitState {
        RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: 1,
                per_ip_burst: burst,
                per_target_per_minute: 100,
                per_target_burst: 100,
            },
            Scopes {
                metrics_prefix: "tlsight",
                per_ip: "per_ip",
                per_target: "per_target",
            },
        )
    }

    #[test]
    fn the_stub_costs_probe_connections_only_with_canned_probes() {
        assert_eq!(StubTlsBackend::new().probe_connections(), 0);
        assert_eq!(
            StubTlsBackend::new()
                .with_probes(canned())
                .probe_connections(),
            7
        );
    }

    #[tokio::test]
    async fn one_probed_address_charges_eight() {
        let stub = Arc::new(StubTlsBackend::new().with_probes(canned()));
        let inspector = stub_state(stub).inspector();
        let limits = limiter(10);
        inspector
            .inspect(&req("example.com", 443), caller(), &limits, "rid".into())
            .await
            .expect("the first inspection fits the burst");
        let second = inspector
            .inspect(&req("example.com", 443), caller(), &limits, "rid".into())
            .await
            .unwrap_err();
        assert!(
            matches!(second, TlsRefusal::RateLimited { .. }),
            "the first charged 8 of 10: {second:?}"
        );
    }

    #[tokio::test]
    async fn two_addresses_probe_only_the_first_with_a_warning() {
        let (a, b) = (ip("93.184.215.14"), ip("93.184.215.15"));
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![a, b])
                .with_probes(canned()),
        );
        let inspector = stub_state(stub.clone()).inspector();
        let resp = inspector
            .inspect(
                &req("example.com", 443),
                caller(),
                &limiter(10),
                "rid".into(),
            )
            .await
            .expect("inspection answers");
        let handshaken: Vec<IpAddr> = stub.calls().iter().map(|(ip, _, _)| *ip).collect();
        assert_eq!(handshaken.len(), 2);
        assert!(handshaken.contains(&a) && handshaken.contains(&b));
        let probed: Vec<IpAddr> = stub.probe_calls().iter().map(|(ip, _, _)| *ip).collect();
        assert_eq!(probed, vec![a]);
        assert!(
            resp.warnings
                .iter()
                .any(|w| w == "rate limit: probing 1 of 2 addresses to stay within budget"),
            "{:?}",
            resp.warnings
        );
    }

    #[tokio::test]
    async fn ports_beyond_the_burst_are_refused_up_front_and_charge_nothing() {
        let stub = Arc::new(StubTlsBackend::new().with_probes(canned()));
        let inspector = stub_state(stub.clone()).inspector();
        let limits = limiter(3);
        let request = TlsInspectRequest {
            host: "example.com".into(),
            port: None,
            ports: Some(vec![443, 8443, 993, 995]),
        };
        let err = inspector
            .inspect(&request, caller(), &limits, "rid".into())
            .await
            .unwrap_err();
        match err {
            TlsRefusal::InvalidInput(message) => {
                assert!(message.contains('4') && message.contains('3'), "{message}");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
        assert!(stub.calls().is_empty());
        assert!(
            limits.check_cost(caller(), "example.com", 3).is_ok(),
            "nothing was charged"
        );
    }
}

// ---------------------------------------------------------------------------
// specs/features/v2-tls-probe-close Phase 3 (requirements 10, 11): the handshake permit is
// released after the handshake; probes have their own permits and a deadline
// ---------------------------------------------------------------------------

mod probe_permits {
    use super::*;
    use std::time::{Duration, Instant};

    use netray_model::Status;
    use netray_tls::state::AppState;
    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
    use tokio::sync::Semaphore;

    fn refused() -> ProbeOutcome {
        ProbeOutcome {
            verdict: Verdict::Refused,
            version: None,
            cipher_suite: None,
            detail: "alert protocol_version".into(),
        }
    }

    fn canned() -> ProtocolProbes {
        ProtocolProbes {
            ssl3: refused(),
            tls1_0: refused(),
            tls1_1: refused(),
            tls1_2: refused(),
            tls1_3: refused(),
            suites_insecure: refused(),
            suites_cbc: refused(),
            suites_static_rsa: refused(),
        }
    }

    /// One handshake and one probing address at a time, a 1 s handshake timeout.
    fn state(stub: Arc<StubTlsBackend>) -> AppState {
        let mut state = stub_state(stub);
        let mut config = stub_config();
        config.limits.handshake_timeout_secs = 1;
        config.limits.max_concurrent_handshakes = 1;
        state.config.store(Arc::new(config));
        state.handshake_semaphore = Arc::new(Semaphore::new(1));
        state.probe_semaphore = Arc::new(Semaphore::new(1));
        state
    }

    #[tokio::test]
    async fn a_hanging_probe_does_not_hold_the_handshake_permit() {
        let (a, b) = (ip("93.184.215.14"), ip("93.184.215.15"));
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![a, b])
                .with_probes(canned())
                .with_probe_delay(Duration::from_secs(10)),
        );
        let inspector = state(stub.clone()).inspector();
        let start = Instant::now();
        let resp = inspector
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("the inspection answers before the request timeout");
        let elapsed = start.elapsed();
        assert_eq!(stub.calls().len(), 2, "both handshakes ran");
        assert!(resp.ports[0].ips.iter().all(|r| r.error.is_none()));
        assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
    }

    #[tokio::test]
    async fn probes_past_the_deadline_are_unanswered_timed_out() {
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_probes(canned())
                .with_probe_delay(Duration::from_secs(10)),
        );
        let inspector = state(stub).inspector();
        let start = Instant::now();
        let resp = inspector
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        assert!(start.elapsed() < Duration::from_millis(1800));
        let probes = resp.ports[0].ips[0].probes.clone().expect("probes");
        for (name, o) in [
            ("ssl3", &probes.ssl3),
            ("tls1_0", &probes.tls1_0),
            ("tls1_1", &probes.tls1_1),
            ("tls1_2", &probes.tls1_2),
            ("suites_insecure", &probes.suites_insecure),
            ("suites_cbc", &probes.suites_cbc),
            ("suites_static_rsa", &probes.suites_static_rsa),
        ] {
            assert_eq!(o.verdict, Verdict::Unanswered, "{name}: {o:?}");
            assert_eq!(o.detail, "timed out", "{name}: {o:?}");
        }
        assert_eq!(probes.tls1_3.verdict, Verdict::Accepted);
        let SectionOutcome::Measured { checks, .. } = translate(&resp) else {
            panic!("expected Measured");
        };
        let legacy = checks
            .iter()
            .find(|c| c.id.to_string() == "tls.legacy_versions")
            .expect("legacy_versions");
        assert_eq!(legacy.status, Status::Unmeasured);
    }

    #[tokio::test]
    async fn waiting_for_a_probe_permit_counts_against_the_deadline() {
        let stub = Arc::new(StubTlsBackend::new().with_probes(canned()));
        let state = state(stub);
        let _held = Arc::clone(&state.probe_semaphore)
            .acquire_owned()
            .await
            .unwrap();
        let inspector = state.inspector();
        let start = Instant::now();
        let resp = inspector
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        assert!(start.elapsed() < Duration::from_millis(1800));
        let probes = resp.ports[0].ips[0].probes.clone().expect("probes");
        assert_eq!(probes.tls1_0.verdict, Verdict::Unanswered);
        assert_eq!(probes.tls1_0.detail, "timed out");
    }
}

// specs/features/v2-tls-probe-close Phase 3 review: a backend that honours the time left returns
// its answers at the deadline; they are kept, never replaced by "timed out".
mod probe_deadline_keeps_answers {
    use super::*;
    use std::time::{Duration, Instant};

    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
    use tokio::sync::Semaphore;

    fn accepted() -> ProbeOutcome {
        ProbeOutcome {
            verdict: Verdict::Accepted,
            version: Some("TLSv1.0".into()),
            cipher_suite: None,
            detail: "server_hello TLSv1.0".into(),
        }
    }

    #[tokio::test]
    async fn answers_returned_at_the_time_left_are_kept() {
        let canned = ProtocolProbes {
            ssl3: accepted(),
            tls1_0: accepted(),
            tls1_1: accepted(),
            tls1_2: accepted(),
            tls1_3: accepted(),
            suites_insecure: accepted(),
            suites_cbc: accepted(),
            suites_static_rsa: accepted(),
        };
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_probes(canned.clone())
                .with_probe_delay(Duration::from_secs(10))
                .honouring_probe_timeout(),
        );
        let mut state = stub_state(stub);
        let mut config = stub_config();
        config.limits.handshake_timeout_secs = 1;
        state.config.store(Arc::new(config));
        state.probe_semaphore = Arc::new(Semaphore::new(1));
        let start = Instant::now();
        let resp = state
            .inspector()
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        assert!(start.elapsed() < Duration::from_millis(1800));
        assert_eq!(resp.ports[0].ips[0].probes, Some(canned));
    }
}

// specs/features/v2-tls-probe-close final review: the probe slots go to the first addresses whose
// handshake succeeded, and two probed addresses behind one handshake permit run side by side.
mod probe_slots {
    use super::*;
    use std::time::{Duration, Instant};

    use netray_tls::security::rate_limit::{Quotas, Scopes};
    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};
    use tokio::sync::Semaphore;

    fn refused() -> ProbeOutcome {
        ProbeOutcome {
            verdict: Verdict::Refused,
            version: None,
            cipher_suite: None,
            detail: "alert protocol_version".into(),
        }
    }

    fn canned() -> ProtocolProbes {
        ProtocolProbes {
            ssl3: refused(),
            tls1_0: refused(),
            tls1_1: refused(),
            tls1_2: refused(),
            tls1_3: refused(),
            suites_insecure: refused(),
            suites_cbc: refused(),
            suites_static_rsa: refused(),
        }
    }

    fn limiter(burst: u32) -> RateLimitState {
        RateLimitState::scoped(
            &Quotas {
                per_ip_per_minute: burst,
                per_ip_burst: burst,
                per_target_per_minute: burst,
                per_target_burst: burst,
            },
            Scopes {
                metrics_prefix: "tlsight",
                per_ip: "per_ip",
                per_target: "per_target",
            },
        )
    }

    #[tokio::test]
    async fn a_failed_first_handshake_passes_its_probe_slot_on() {
        let (bad, good) = (ip("93.184.215.14"), ip("93.184.215.15"));
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![bad, good])
                .with_failed_handshake(bad)
                .with_probes(canned()),
        );
        let resp = stub_state(stub.clone())
            .inspector()
            .inspect(
                &req("example.com", 443),
                caller(),
                &limiter(10),
                "rid".into(),
            )
            .await
            .expect("inspection answers");
        let probed: Vec<IpAddr> = stub.probe_calls().iter().map(|(ip, _, _)| *ip).collect();
        assert_eq!(probed, vec![good]);
        let good_res = resp.ports[0]
            .ips
            .iter()
            .find(|r| r.ip == good.to_string())
            .unwrap();
        assert_eq!(good_res.probes, Some(canned()));
    }

    #[tokio::test]
    async fn two_probed_addresses_do_not_wait_on_each_others_probes_for_the_handshake() {
        let (a, b) = (ip("93.184.215.14"), ip("93.184.215.15"));
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![a, b])
                .with_probes(canned())
                .with_probe_delay(Duration::from_secs(10)),
        );
        let mut state = stub_state(stub.clone());
        let mut config = stub_config();
        config.limits.handshake_timeout_secs = 1;
        state.config.store(Arc::new(config));
        state.handshake_semaphore = Arc::new(Semaphore::new(1));
        state.probe_semaphore = Arc::new(Semaphore::new(2));
        let start = Instant::now();
        state
            .inspector()
            .inspect(
                &req("example.com", 443),
                caller(),
                &limiter(100),
                "rid".into(),
            )
            .await
            .expect("inspection answers");
        let elapsed = start.elapsed();
        assert_eq!(stub.probe_calls().len(), 2, "both addresses are probed");
        assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
    }
}

// specs/features/v2-tls-probe-close final review, second pass: a slow failing first address
// hands its slot on without holding the port: the next address's probe deadline runs from its
// own handshake.
mod probe_slot_no_barrier {
    use super::*;
    use std::time::{Duration, Instant};

    use netray_tls::tls::probe::{ProbeOutcome, ProtocolProbes, Verdict};

    fn refused() -> ProbeOutcome {
        ProbeOutcome {
            verdict: Verdict::Refused,
            version: None,
            cipher_suite: None,
            detail: "alert protocol_version".into(),
        }
    }

    #[tokio::test]
    async fn a_slow_failed_first_handshake_does_not_delay_the_port() {
        let canned = ProtocolProbes {
            ssl3: refused(),
            tls1_0: refused(),
            tls1_1: refused(),
            tls1_2: refused(),
            tls1_3: refused(),
            suites_insecure: refused(),
            suites_cbc: refused(),
            suites_static_rsa: refused(),
        };
        let (slow, good) = (ip("93.184.215.14"), ip("93.184.215.15"));
        let stub = Arc::new(
            StubTlsBackend::new()
                .with_host("example.com", vec![slow, good])
                .with_handshake_delay(slow, Duration::from_millis(1200))
                .with_failed_handshake(slow)
                .with_probes(canned)
                .with_probe_delay(Duration::from_secs(10))
                .honouring_probe_timeout(),
        );
        let mut state = stub_state(stub.clone());
        let mut config = stub_config();
        config.limits.handshake_timeout_secs = 1;
        state.config.store(Arc::new(config));
        let start = Instant::now();
        let resp = state
            .inspector()
            .inspect(&req("example.com", 443), caller(), &limits(), "rid".into())
            .await
            .expect("inspection answers");
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(1800), "took {elapsed:?}");
        let good_res = resp.ports[0]
            .ips
            .iter()
            .find(|r| r.ip == good.to_string())
            .unwrap();
        let probes = good_res
            .probes
            .clone()
            .expect("the slot passed to the good address");
        assert_eq!(probes.tls1_0.verdict, Verdict::Unanswered, "{probes:?}");
        assert_eq!(probes.tls1_0.detail, "timed out");
    }
}
