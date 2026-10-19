//! Results table for `target_policy::is_allowed_target`.
//!
//! The table pins the answers, recorded literally. R3.7 of 0.23.0 moved the
//! range rows (0.0.0.0/8, 240.0.0.0/4, 198.18.0.0/15, 192.0.0.0/24 and the
//! 6to4 row) to refused; the public-address rows stay allowed.
//!
//! 192.0.0.8 is chosen inside 192.0.0.0/24 on purpose: 192.0.0.9 and
//! 192.0.0.10 are globally reachable anycast addresses, so they would not
//! represent the range.

use std::net::IpAddr;

use netray_common::target_policy::is_allowed_target;

#[test]
fn is_allowed_target_results_table() {
    // (address, expected answer of is_allowed_target)
    let table: &[(&str, bool)] = &[
        // 0.0.0.0/8 except 0.0.0.0
        ("0.1.2.3", false),
        // 240.0.0.0/4
        ("240.0.0.1", false),
        // 198.18.0.0/15
        ("198.18.0.1", false),
        // 192.0.0.0/24, a non-anycast member
        ("192.0.0.8", false),
        // 6to4 embedding 8.8.8.8
        ("2002:808:808::1", false),
        // public addresses
        ("8.8.8.8", true),
        ("1.1.1.1", true),
        ("2606:4700::", true),
    ];

    for (addr, expected) in table {
        let ip: IpAddr = addr.parse().unwrap();
        assert_eq!(
            is_allowed_target(ip),
            *expected,
            "is_allowed_target({addr}) should be {expected}"
        );
    }
}
