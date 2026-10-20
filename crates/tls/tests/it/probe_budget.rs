// v2-tls-probe-close, Phase 2 (requirements 4 and 5): one plan decides, for a budget, the
// addresses inspected and how many of them are probed; a probed address costs the backend's
// probe connections on every port, beside its handshake.
//
// Spec: specs/features/v2-tls-probe-close/spec.md, Phase 2.

use std::net::IpAddr;

use netray_tls::backend::{LiveBackend, TlsBackend};
use netray_tls::routes::plan_budget;

fn public(n: u8) -> IpAddr {
    IpAddr::from([192, 0, 2, n])
}

fn ips(n: u8) -> Vec<IpAddr> {
    (1..=n).map(public).collect()
}

#[test]
fn one_address_one_port_is_inspected_and_probed_at_cost_8() {
    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(1), 1, 10, 7, &mut warnings);
    assert_eq!(plan.ips, ips(1));
    assert_eq!(plan.probed, 1);
    assert_eq!(plan.cost, 8);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn two_addresses_probe_the_first_at_cost_9() {
    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(2), 1, 10, 7, &mut warnings);
    assert_eq!(plan.ips, ips(2));
    assert!(plan.skipped.is_empty());
    assert_eq!(plan.probed, 1);
    assert_eq!(plan.cost, 9);
    assert_eq!(
        warnings,
        ["rate limit: probing 1 of 2 addresses to stay within budget"]
    );
}

#[test]
fn four_addresses_reserve_one_probed_address() {
    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(4), 1, 10, 7, &mut warnings);
    assert_eq!(plan.ips.len(), 3);
    assert_eq!(plan.skipped.len(), 1);
    assert_eq!(plan.probed, 1);
    assert_eq!(plan.cost, 10);
    assert_eq!(
        warnings,
        [
            "rate limit: inspecting 3 of 4 IPs to stay within budget",
            "rate limit: probing 1 of 3 addresses to stay within budget",
        ]
    );
}

#[test]
fn two_ports_without_room_for_probes_probe_nothing() {
    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(1), 2, 10, 7, &mut warnings);
    assert_eq!(plan.ips, ips(1));
    assert_eq!(plan.probed, 0);
    assert_eq!(plan.cost, 2);
    assert_eq!(
        warnings,
        ["rate limit: probing 0 of 1 addresses to stay within budget"]
    );
}

#[test]
fn without_probe_connections_the_plan_is_cap_and_warn() {
    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(25), 1, 10, 0, &mut warnings);
    assert_eq!(plan.ips.len(), 10);
    assert_eq!(plan.skipped.len(), 15);
    assert_eq!(plan.probed, 10);
    assert_eq!(plan.cost, 10);
    assert_eq!(
        warnings,
        ["rate limit: inspecting 10 of 25 IPs to stay within budget"]
    );

    let mut warnings = Vec::new();
    let plan = plan_budget(&ips(3), 2, 10, 0, &mut warnings);
    assert_eq!(plan.ips, ips(3));
    assert_eq!(plan.cost, 6);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn the_live_backend_costs_seven_connections_per_probed_address() {
    let backend = LiveBackend { resolver: None };
    assert_eq!(backend.probe_connections(), 7);
}
