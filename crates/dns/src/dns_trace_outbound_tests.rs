//! The trace walk under the outbound policy (requirement 2 of
//! `specs/features/raw-query-policy/spec.md`): every query goes through the injected
//! context, and referral glue that is not public is never queried. Offline: the recording
//! sender answers every query, root queries included.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use hickory_proto::rr::RecordType;

use super::walk_with;
use crate::dns_raw::outbound_tests::{
    RecordingSender, StubResolver, a_record, is_root, name, referral, reply, test_outbound, v4,
};

const AUTH: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(192, 0, 2, 53));

/// Roots refer "example.com." to `ns1.example.com.` with `glue`; 192.0.2.53 answers
/// authoritatively.
fn sender_with_glue(glue: IpAddr) -> Arc<RecordingSender> {
    RecordingSender::new(move |server, _name, _rtype| {
        if is_root(server.ip()) {
            Some(referral(
                "com.",
                &["ns1.example.com."],
                &[("ns1.example.com.", glue)],
            ))
        } else if server.ip() == AUTH {
            let mut m = reply(true);
            m.add_answer(a_record(
                "example.com.",
                std::net::Ipv4Addr::new(192, 0, 2, 80),
            ));
            Some(m)
        } else {
            None
        }
    })
}

async fn trace(sender: Arc<RecordingSender>) -> Vec<crate::dns_trace::TraceHop> {
    let raw = test_outbound(sender, StubResolver::new(&[]));
    walk_with(
        &raw,
        name("example.com."),
        RecordType::A,
        5,
        Duration::from_millis(50),
    )
    .await
}

#[tokio::test]
async fn trace_never_queries_loopback_referral_glue() {
    let sender = sender_with_glue(v4(127, 0, 0, 1));
    let hops = trace(sender.clone()).await;

    assert!(
        !sender.recorded_ips().contains(&v4(127, 0, 0, 1)),
        "loopback glue was queried: {:?}",
        sender.recorded_ips()
    );
    assert!(hops.iter().all(|h| !h.is_final));
}

#[tokio::test]
async fn trace_queries_public_referral_glue_and_ends_authoritative() {
    let sender = sender_with_glue(AUTH);
    let hops = trace(sender.clone()).await;

    assert!(
        sender.recorded_ips().contains(&AUTH),
        "public glue was not queried: {:?}",
        sender.recorded_ips()
    );
    assert!(hops.last().is_some_and(|h| h.is_final));
}

#[tokio::test]
async fn trace_sends_through_the_injected_context() {
    let sender = sender_with_glue(AUTH);
    let _ = trace(sender.clone()).await;

    let ips = sender.recorded_ips();
    assert!(
        ips.iter().any(|ip| is_root(*ip)),
        "no root server reached the recording sender: {ips:?}"
    );
}
