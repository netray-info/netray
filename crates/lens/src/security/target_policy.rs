//! Target IP policy — blocks lens from proxying to reserved/internal IP ranges.
//!
//! Called after DNS resolution in the check orchestration layer, not at input
//! validation time. This prevents SSRF via domain fronting to internal addresses.

use std::net::{IpAddr, Ipv4Addr};

use crate::error::AppError;

/// Check that all resolved IPs for a domain are publicly routable.
///
/// Rejects any IP that is:
/// - Loopback (127.0.0.0/8, ::1)
/// - RFC 1918 private (10/8, 172.16/12, 192.168/16)
/// - Link-local (169.254/16, fe80::/10)
/// - CGNAT (100.64/10)
/// - Multicast
/// - Unspecified (0.0.0.0, ::)
///
/// Returns `Ok(())` if every IP passes, or `Err(AppError::DomainBlocked)` with
/// the first rejected IP and its reason.
pub fn check_target(domain: &str, resolved_ips: &[IpAddr]) -> Result<(), AppError> {
    for ip in resolved_ips {
        if let Err(reason) = check_ip(ip) {
            return Err(AppError::DomainBlocked(format!(
                "domain '{domain}' resolves to blocked address {ip}: {reason}"
            )));
        }
    }
    Ok(())
}

fn check_ip(ip: &IpAddr) -> Result<(), &'static str> {
    match ip {
        IpAddr::V4(v4) => check_ipv4(v4),
        IpAddr::V6(v6) => {
            // Treat IPv4-mapped IPv6 addresses (::ffff:a.b.c.d) as IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return check_ipv4(&v4);
            }
            check_ipv6(v6)
        }
    }
}

fn check_ipv4(v4: &Ipv4Addr) -> Result<(), &'static str> {
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
    if v4.is_multicast() {
        return Err("multicast address");
    }
    // CGNAT (100.64.0.0/10)
    let octets = v4.octets();
    if octets[0] == 100 && (octets[1] & 0xC0) == 64 {
        return Err("CGNAT address (100.64.0.0/10)");
    }
    Ok(())
}

fn check_ipv6(v6: &std::net::Ipv6Addr) -> Result<(), &'static str> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn ok(ip: &str) {
        let ip: IpAddr = ip.parse().unwrap();
        assert!(
            check_ip(&ip).is_ok(),
            "expected {ip} to be allowed, but got: {:?}",
            check_ip(&ip)
        );
    }

    fn blocked(ip: &str) {
        let ip: IpAddr = ip.parse().unwrap();
        assert!(
            check_ip(&ip).is_err(),
            "expected {ip} to be blocked, but was allowed"
        );
    }

    // --- IPv4 blocked ---

    #[test]
    fn blocks_loopback_v4() {
        blocked("127.0.0.1");
    }

    #[test]
    fn blocks_private_10() {
        blocked("10.0.0.1");
    }

    #[test]
    fn blocks_private_172_16() {
        blocked("172.16.0.1");
    }

    #[test]
    fn blocks_private_192_168() {
        blocked("192.168.1.1");
    }

    #[test]
    fn blocks_link_local_v4() {
        blocked("169.254.1.1");
    }

    #[test]
    fn blocks_cgnat_start() {
        blocked("100.64.0.1");
    }

    #[test]
    fn blocks_cgnat_end() {
        blocked("100.127.255.255");
    }

    #[test]
    fn allows_non_cgnat_100() {
        ok("100.128.0.1");
    }

    #[test]
    fn blocks_multicast_v4() {
        blocked("224.0.0.1");
    }

    #[test]
    fn blocks_unspecified_v4() {
        let ip = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
        assert!(check_ip(&ip).is_err());
    }

    // --- IPv4 allowed ---

    #[test]
    fn allows_public_v4() {
        ok("8.8.8.8");
    }

    #[test]
    fn allows_cloudflare_v4() {
        ok("1.1.1.1");
    }

    // --- IPv6 blocked ---

    #[test]
    fn blocks_loopback_v6() {
        let ip = IpAddr::V6(Ipv6Addr::LOCALHOST);
        assert!(check_ip(&ip).is_err());
    }

    #[test]
    fn blocks_unspecified_v6() {
        let ip = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
        assert!(check_ip(&ip).is_err());
    }

    #[test]
    fn blocks_multicast_v6() {
        blocked("ff02::1");
    }

    #[test]
    fn blocks_link_local_v6() {
        blocked("fe80::1");
    }

    #[test]
    fn blocks_ula_fc00() {
        blocked("fc00::1");
    }

    #[test]
    fn blocks_ula_fd00() {
        blocked("fd12:3456::1");
    }

    #[test]
    fn blocks_ipv4_mapped_loopback() {
        blocked("::ffff:127.0.0.1");
    }

    #[test]
    fn blocks_ipv4_mapped_private() {
        blocked("::ffff:192.168.1.1");
    }

    #[test]
    fn blocks_ipv4_mapped_cgnat() {
        blocked("::ffff:100.64.0.1");
    }

    // --- IPv6 allowed ---

    #[test]
    fn allows_public_v6() {
        ok("2606:4700::1");
    }

    // --- check_target function ---

    #[test]
    fn check_target_empty_ips_passes() {
        assert!(check_target("example.com", &[]).is_ok());
    }

    #[test]
    fn check_target_all_public_passes() {
        let ips: Vec<IpAddr> = vec!["8.8.8.8".parse().unwrap(), "2606:4700::1".parse().unwrap()];
        assert!(check_target("example.com", &ips).is_ok());
    }

    #[test]
    fn check_target_private_ip_blocked() {
        let ips: Vec<IpAddr> = vec!["8.8.8.8".parse().unwrap(), "10.0.0.1".parse().unwrap()];
        let err = check_target("example.com", &ips).unwrap_err();
        assert!(matches!(err, AppError::DomainBlocked(_)));
        let AppError::DomainBlocked(msg) = err else {
            panic!("expected DomainBlocked");
        };
        assert!(msg.contains("example.com"));
        assert!(msg.contains("10.0.0.1"));
    }

    #[test]
    fn check_target_loopback_blocked() {
        let ips: Vec<IpAddr> = vec!["127.0.0.1".parse().unwrap()];
        let err = check_target("internal.example.com", &ips).unwrap_err();
        assert!(matches!(err, AppError::DomainBlocked(_)));
    }
}
