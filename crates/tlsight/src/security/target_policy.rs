//! Target IP validation — blocks inspection of internal/reserved IP addresses.
//!
//! This is the first layer of defense-in-depth (SDD §8.1). Before any TCP
//! connection is made, every resolved IP is checked against the shared
//! blocklist in `netray_common::target_policy`. This prevents the service from
//! being used as an SSRF vector to probe internal infrastructure.

use std::net::IpAddr;

use netray_common::target_policy;

/// Check if an IP address is allowed as a TLS inspection target.
///
/// Returns `Ok(())` if the IP is routable and not in any reserved range,
/// or if `allow_blocked` is true (development mode).
/// Returns `Err` with a human-readable reason if the IP is blocked.
pub fn check_allowed_with_policy(ip: &IpAddr, allow_blocked: bool) -> Result<(), &'static str> {
    if allow_blocked {
        return Ok(());
    }
    match target_policy::refusal_reason(*ip) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // ---- IPv4 blocked ranges ----

    #[test]
    fn blocks_loopback_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_loopback_v4_non_standard() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 255, 255, 255));
        assert!(check_allowed_with_policy(&ip, false).is_err());
    }

    #[test]
    fn blocks_private_10() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("private"), "{err}");
    }

    #[test]
    fn blocks_private_172_16() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));
        assert!(check_allowed_with_policy(&ip, false).is_err());
    }

    #[test]
    fn blocks_private_172_31() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255));
        assert!(check_allowed_with_policy(&ip, false).is_err());
    }

    #[test]
    fn blocks_private_192_168() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert!(check_allowed_with_policy(&ip, false).is_err());
    }

    #[test]
    fn blocks_link_local_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_broadcast() {
        let ip = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("broadcast"), "{err}");
    }

    #[test]
    fn blocks_unspecified_v4() {
        let ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("unspecified"), "{err}");
    }

    #[test]
    fn blocks_cgnat_start() {
        let ip = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn blocks_cgnat_end() {
        let ip = IpAddr::V4(Ipv4Addr::new(100, 127, 255, 255));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn allows_non_cgnat_100() {
        // 100.128.0.0 is outside CGNAT range
        let ip = IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1));
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    #[test]
    fn blocks_doc_192_0_2() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_doc_198_51_100() {
        let ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 50));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_doc_203_0_113() {
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 200));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_multicast_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1));
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("multicast"), "{err}");
    }

    #[test]
    fn blocks_multicast_v4_high() {
        let ip = IpAddr::V4(Ipv4Addr::new(239, 255, 255, 255));
        assert!(check_allowed_with_policy(&ip, false).is_err());
    }

    // ---- IPv4 allowed ----

    #[test]
    fn allows_public_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    #[test]
    fn allows_cloudflare_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    #[test]
    fn allows_public_v4_high() {
        let ip = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    // ---- IPv6 blocked ranges ----

    #[test]
    fn blocks_loopback_v6() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_unspecified_v6() {
        let ip = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("unspecified"), "{err}");
    }

    #[test]
    fn blocks_multicast_v6() {
        let ip: IpAddr = "ff02::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("multicast"), "{err}");
    }

    #[test]
    fn blocks_link_local_v6() {
        let ip: IpAddr = "fe80::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_link_local_v6_high() {
        let ip: IpAddr = "febf::ffff".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_ula_fc00() {
        let ip: IpAddr = "fc00::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("unique local"), "{err}");
    }

    #[test]
    fn blocks_ula_fd00() {
        let ip: IpAddr = "fd12:3456::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("unique local"), "{err}");
    }

    #[test]
    fn blocks_doc_2001_db8() {
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_deprecated_site_local_v6() {
        let ip: IpAddr = "fec0::1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("deprecated site-local"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_loopback() {
        let ip: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_private() {
        let ip: IpAddr = "::ffff:192.168.1.1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("private"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_cgnat() {
        let ip: IpAddr = "::ffff:100.64.0.1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn blocks_6to4() {
        let ip: IpAddr = "2002:7f00:1::".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("6to4"), "{err}");
    }

    #[test]
    fn blocks_nat64() {
        let ip: IpAddr = "64:ff9b::7f00:1".parse().unwrap();
        let err = check_allowed_with_policy(&ip, false).unwrap_err();
        assert!(err.contains("NAT64"), "{err}");
    }

    // ---- IPv6 allowed ----

    #[test]
    fn allows_public_v6() {
        let ip: IpAddr = "2606:4700::1".parse().unwrap();
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    #[test]
    fn allows_google_dns_v6() {
        let ip: IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(check_allowed_with_policy(&ip, false).is_ok());
    }

    // ---- agreement with the shared policy ----

    #[test]
    fn agrees_with_shared_target_policy() {
        let addrs = [
            "10.0.0.1",
            "127.0.0.1",
            "::1",
            "fe80::1",
            "fc00::1",
            "100.64.0.1",
            "0.1.2.3",
            "240.0.0.1",
            "198.18.0.1",
            "192.0.0.8",
            "2002:808:808::1",
            "2002:a00:1::",
            "64:ff9b::a00:1",
            "::ffff:10.0.0.1",
            "8.8.8.8",
            "2606:4700::",
        ];
        for a in addrs {
            let ip: IpAddr = a.parse().unwrap();
            assert_eq!(
                check_allowed_with_policy(&ip, false).is_err(),
                !netray_common::target_policy::is_allowed_target(ip),
                "tlsight and shared policy disagree on {a}"
            );
        }
    }

    #[test]
    fn policy_flag_allows_private_target() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert!(check_allowed_with_policy(&ip, true).is_ok());
    }
}
