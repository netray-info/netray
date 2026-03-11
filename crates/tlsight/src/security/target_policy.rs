//! Target IP validation — blocks inspection of internal/reserved IP addresses.
//!
//! This is the first layer of defense-in-depth (SDD §8.1). Before any TCP
//! connection is made, every resolved IP is checked against a blocklist of
//! reserved ranges. This prevents the service from being used as an SSRF
//! vector to probe internal infrastructure.

use std::net::IpAddr;

fn check_ipv4(v4: &std::net::Ipv4Addr) -> Result<(), &'static str> {
    if v4.is_loopback() {
        return Err("loopback address");
    }
    if v4.is_private() {
        return Err("private address (RFC 1918)");
    }
    if v4.is_link_local() {
        return Err("link-local address");
    }
    if v4.is_broadcast() {
        return Err("broadcast address");
    }
    if v4.is_unspecified() {
        return Err("unspecified address");
    }
    // CGNAT (100.64.0.0/10)
    let octets = v4.octets();
    if octets[0] == 100 && (octets[1] & 0xC0) == 64 {
        return Err("CGNAT address (100.64.0.0/10)");
    }
    // Documentation ranges (RFC 5737)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
        return Err("documentation address (192.0.2.0/24)");
    }
    if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
        return Err("documentation address (198.51.100.0/24)");
    }
    if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
        return Err("documentation address (203.0.113.0/24)");
    }
    // Multicast
    if v4.is_multicast() {
        return Err("multicast address");
    }
    Ok(())
}

fn check_ipv6(v6: &std::net::Ipv6Addr) -> Result<(), &'static str> {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return check_ipv4(&v4);
    }
    if v6.is_loopback() {
        return Err("loopback address");
    }
    if v6.is_unspecified() {
        return Err("unspecified address");
    }
    if v6.is_multicast() {
        return Err("multicast address");
    }
    let segments = v6.segments();
    // Link-local fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return Err("link-local address");
    }
    // ULA fc00::/7
    if (segments[0] & 0xfe00) == 0xfc00 {
        return Err("unique local address (ULA)");
    }
    // Documentation 2001:db8::/32
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return Err("documentation address (2001:db8::/32)");
    }
    // 6to4 2002::/16
    if segments[0] == 0x2002 {
        return Err("6to4 address");
    }
    // NAT64 64:ff9b::/96
    if segments[0] == 0x0064
        && segments[1] == 0xff9b
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0
    {
        return Err("NAT64 address");
    }
    // Deprecated site-local fec0::/10
    if (segments[0] & 0xffc0) == 0xfec0 {
        return Err("deprecated site-local address (fec0::/10)");
    }
    Ok(())
}

/// Check if an IP address is allowed as a TLS inspection target.
///
/// Returns `Ok(())` if the IP is routable and not in any reserved range,
/// or if `allow_blocked` is true (development mode).
/// Returns `Err` with a human-readable reason if the IP is blocked.
#[cfg(test)]
pub fn check_allowed(ip: &IpAddr) -> Result<(), &'static str> {
    check_allowed_inner(ip, false)
}

/// Like [`check_allowed`] but with an explicit flag to bypass blocking.
pub fn check_allowed_with_policy(ip: &IpAddr, allow_blocked: bool) -> Result<(), &'static str> {
    check_allowed_inner(ip, allow_blocked)
}

fn check_allowed_inner(ip: &IpAddr, allow_blocked: bool) -> Result<(), &'static str> {
    if allow_blocked {
        return Ok(());
    }
    match ip {
        IpAddr::V4(v4) => check_ipv4(v4),
        IpAddr::V6(v6) => check_ipv6(v6),
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
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_loopback_v4_non_standard() {
        let ip = IpAddr::V4(Ipv4Addr::new(127, 255, 255, 255));
        assert!(check_allowed(&ip).is_err());
    }

    #[test]
    fn blocks_private_10() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("private"), "{err}");
    }

    #[test]
    fn blocks_private_172_16() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));
        assert!(check_allowed(&ip).is_err());
    }

    #[test]
    fn blocks_private_172_31() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 31, 255, 255));
        assert!(check_allowed(&ip).is_err());
    }

    #[test]
    fn blocks_private_192_168() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        assert!(check_allowed(&ip).is_err());
    }

    #[test]
    fn blocks_link_local_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_broadcast() {
        let ip = IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("broadcast"), "{err}");
    }

    #[test]
    fn blocks_unspecified_v4() {
        let ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("unspecified"), "{err}");
    }

    #[test]
    fn blocks_cgnat_start() {
        let ip = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 1));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn blocks_cgnat_end() {
        let ip = IpAddr::V4(Ipv4Addr::new(100, 127, 255, 255));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn allows_non_cgnat_100() {
        // 100.128.0.0 is outside CGNAT range
        let ip = IpAddr::V4(Ipv4Addr::new(100, 128, 0, 1));
        assert!(check_allowed(&ip).is_ok());
    }

    #[test]
    fn blocks_doc_192_0_2() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_doc_198_51_100() {
        let ip = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 50));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_doc_203_0_113() {
        let ip = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 200));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_multicast_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1));
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("multicast"), "{err}");
    }

    #[test]
    fn blocks_multicast_v4_high() {
        let ip = IpAddr::V4(Ipv4Addr::new(239, 255, 255, 255));
        assert!(check_allowed(&ip).is_err());
    }

    // ---- IPv4 allowed ----

    #[test]
    fn allows_public_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        assert!(check_allowed(&ip).is_ok());
    }

    #[test]
    fn allows_cloudflare_v4() {
        let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        assert!(check_allowed(&ip).is_ok());
    }

    #[test]
    fn allows_public_v4_high() {
        let ip = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
        assert!(check_allowed(&ip).is_ok());
    }

    // ---- IPv6 blocked ranges ----

    #[test]
    fn blocks_loopback_v6() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_unspecified_v6() {
        let ip = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("unspecified"), "{err}");
    }

    #[test]
    fn blocks_multicast_v6() {
        let ip: IpAddr = "ff02::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("multicast"), "{err}");
    }

    #[test]
    fn blocks_link_local_v6() {
        let ip: IpAddr = "fe80::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_link_local_v6_high() {
        let ip: IpAddr = "febf::ffff".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("link-local"), "{err}");
    }

    #[test]
    fn blocks_ula_fc00() {
        let ip: IpAddr = "fc00::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("unique local"), "{err}");
    }

    #[test]
    fn blocks_ula_fd00() {
        let ip: IpAddr = "fd12:3456::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("unique local"), "{err}");
    }

    #[test]
    fn blocks_doc_2001_db8() {
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("documentation"), "{err}");
    }

    #[test]
    fn blocks_deprecated_site_local_v6() {
        let ip: IpAddr = "fec0::1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("deprecated site-local"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_loopback() {
        let ip: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_private() {
        let ip: IpAddr = "::ffff:192.168.1.1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("private"), "{err}");
    }

    #[test]
    fn blocks_ipv4_mapped_cgnat() {
        let ip: IpAddr = "::ffff:100.64.0.1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("CGNAT"), "{err}");
    }

    #[test]
    fn blocks_6to4() {
        let ip: IpAddr = "2002:7f00:1::".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("6to4"), "{err}");
    }

    #[test]
    fn blocks_nat64() {
        let ip: IpAddr = "64:ff9b::7f00:1".parse().unwrap();
        let err = check_allowed(&ip).unwrap_err();
        assert!(err.contains("NAT64"), "{err}");
    }

    // ---- IPv6 allowed ----

    #[test]
    fn allows_public_v6() {
        let ip: IpAddr = "2606:4700::1".parse().unwrap();
        assert!(check_allowed(&ip).is_ok());
    }

    #[test]
    fn allows_google_dns_v6() {
        let ip: IpAddr = "2001:4860:4860::8888".parse().unwrap();
        assert!(check_allowed(&ip).is_ok());
    }
}
