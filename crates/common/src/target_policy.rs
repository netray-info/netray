//! Unified target IP validation — SSRF blocklist covering all special ranges.
//!
//! This module is the single authoritative source for deciding whether an IP
//! address is safe to contact as an outbound target. It covers every reserved
//! or special-purpose range:
//!
//! - Loopback (127.0.0.0/8, ::1)
//! - Unspecified (0.0.0.0, ::)
//! - "This network" (0.0.0.0/8)
//! - RFC 1918 private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
//! - Link-local (169.254.0.0/16, fe80::/10)
//! - CGNAT (100.64.0.0/10, RFC 6598)
//! - Multicast (224.0.0.0/4, ff00::/8)
//! - Reserved (240.0.0.0/4, includes the broadcast address 255.255.255.255)
//! - Benchmarking (198.18.0.0/15, RFC 2544)
//! - IETF protocol assignments (192.0.0.0/24, RFC 6890)
//! - Documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24, 2001:db8::/32)
//! - IPv6 ULA (fc00::/7)
//! - IPv6 deprecated site-local (fec0::/10)
//! - IPv4-mapped IPv6 (::ffff:x.x.x.x — delegates to IPv4 check)
//! - 6to4 (2002::/16, the whole range)
//! - NAT64 well-known prefix (64:ff9b::/96)

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Returns `true` if the IP address is a safe, publicly-routable target.
///
/// Returns `false` for any reserved, private, or special-purpose address
/// as listed in the module documentation.
pub fn is_allowed_target(ip: IpAddr) -> bool {
    refusal_reason(ip).is_none()
}

/// Returns the name of the range that makes `ip` a refused target, or `None`
/// if the address is allowed.
///
/// IPv4-mapped IPv6 addresses (`::ffff:x.x.x.x`) report the reason of the
/// embedded IPv4 address.
pub fn refusal_reason(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(v4) => refusal_reason_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return refusal_reason_v4(v4);
            }
            refusal_reason_v6(v6)
        }
    }
}

fn refusal_reason_v4(v4: Ipv4Addr) -> Option<&'static str> {
    if v4.is_loopback() {
        return Some("loopback address");
    }
    if v4.is_unspecified() {
        return Some("unspecified address");
    }
    if v4.is_multicast() {
        return Some("multicast address");
    }
    if v4.is_broadcast() {
        return Some("broadcast address");
    }
    let o = v4.octets();
    // "This network": 0.0.0.0/8
    if o[0] == 0 {
        return Some("this-network address (0.0.0.0/8)");
    }
    // Reserved: 240.0.0.0/4 (255.255.255.255 is reported as broadcast above)
    if o[0] >= 240 {
        return Some("reserved address (240.0.0.0/4)");
    }
    // RFC 1918: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    if v4.is_private() {
        return Some("private address (RFC 1918)");
    }
    // Link-local: 169.254.0.0/16
    if v4.is_link_local() {
        return Some("link-local address");
    }
    // CGNAT: 100.64.0.0/10 (RFC 6598)
    if o[0] == 100 && (o[1] & 0xC0) == 64 {
        return Some("CGNAT address (100.64.0.0/10)");
    }
    // Benchmarking: 198.18.0.0/15 (RFC 2544)
    if o[0] == 198 && (o[1] & 0xFE) == 18 {
        return Some("benchmarking address (198.18.0.0/15)");
    }
    // IETF protocol assignments: 192.0.0.0/24 (RFC 6890)
    if o[0] == 192 && o[1] == 0 && o[2] == 0 {
        return Some("IETF protocol assignments address (192.0.0.0/24)");
    }
    // Documentation: 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24 (RFC 5737)
    if o[0] == 192 && o[1] == 0 && o[2] == 2 {
        return Some("documentation address (192.0.2.0/24)");
    }
    if o[0] == 198 && o[1] == 51 && o[2] == 100 {
        return Some("documentation address (198.51.100.0/24)");
    }
    if o[0] == 203 && o[1] == 0 && o[2] == 113 {
        return Some("documentation address (203.0.113.0/24)");
    }
    None
}

fn refusal_reason_v6(v6: Ipv6Addr) -> Option<&'static str> {
    if v6.is_loopback() {
        return Some("loopback address");
    }
    if v6.is_unspecified() {
        return Some("unspecified address");
    }
    if v6.is_multicast() {
        return Some("multicast address");
    }
    let segs = v6.segments();
    // Link-local: fe80::/10
    if (segs[0] & 0xFFC0) == 0xFE80 {
        return Some("link-local address");
    }
    // ULA: fc00::/7
    if (segs[0] & 0xFE00) == 0xFC00 {
        return Some("unique local address (fc00::/7)");
    }
    // Deprecated site-local: fec0::/10
    if (segs[0] & 0xFFC0) == 0xFEC0 {
        return Some("deprecated site-local address (fec0::/10)");
    }
    // Documentation: 2001:db8::/32
    if segs[0] == 0x2001 && segs[1] == 0x0DB8 {
        return Some("documentation address (2001:db8::/32)");
    }
    // 6to4: 2002::/16, the whole range
    if segs[0] == 0x2002 {
        return Some("6to4 address (2002::/16)");
    }
    // NAT64 well-known prefix: 64:ff9b::/96
    if segs[0] == 0x0064
        && segs[1] == 0xFF9B
        && segs[2] == 0
        && segs[3] == 0
        && segs[4] == 0
        && segs[5] == 0
    {
        return Some("NAT64 address (64:ff9b::/96)");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- loopback ----

    #[test]
    fn blocks_ipv4_loopback() {
        assert!(!is_allowed_target("127.0.0.1".parse().unwrap()));
        assert!(!is_allowed_target("127.255.255.255".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv6_loopback() {
        assert!(!is_allowed_target("::1".parse().unwrap()));
    }

    // ---- unspecified ----

    #[test]
    fn blocks_ipv4_unspecified() {
        assert!(!is_allowed_target("0.0.0.0".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv6_unspecified() {
        assert!(!is_allowed_target("::".parse().unwrap()));
    }

    // ---- RFC 1918 ----

    #[test]
    fn blocks_rfc1918() {
        assert!(!is_allowed_target("10.0.0.1".parse().unwrap()));
        assert!(!is_allowed_target("172.16.0.1".parse().unwrap()));
        assert!(!is_allowed_target("172.31.255.255".parse().unwrap()));
        assert!(!is_allowed_target("192.168.1.1".parse().unwrap()));
    }

    // ---- link-local ----

    #[test]
    fn blocks_link_local_ipv4() {
        assert!(!is_allowed_target("169.254.1.1".parse().unwrap()));
        assert!(!is_allowed_target("169.254.255.255".parse().unwrap()));
    }

    #[test]
    fn blocks_link_local_ipv6() {
        assert!(!is_allowed_target("fe80::1".parse().unwrap()));
        assert!(!is_allowed_target("febf::ffff".parse().unwrap()));
    }

    // ---- CGNAT ----

    #[test]
    fn blocks_cgnat() {
        assert!(!is_allowed_target("100.64.0.0".parse().unwrap()));
        assert!(!is_allowed_target("100.64.0.1".parse().unwrap()));
        assert!(!is_allowed_target("100.127.255.255".parse().unwrap()));
    }

    #[test]
    fn allows_100_outside_cgnat() {
        assert!(is_allowed_target("100.63.255.255".parse().unwrap()));
        assert!(is_allowed_target("100.128.0.0".parse().unwrap()));
    }

    // ---- multicast ----

    #[test]
    fn blocks_multicast_ipv4() {
        assert!(!is_allowed_target("224.0.0.1".parse().unwrap()));
        assert!(!is_allowed_target("239.255.255.255".parse().unwrap()));
    }

    #[test]
    fn blocks_multicast_ipv6() {
        assert!(!is_allowed_target("ff02::1".parse().unwrap()));
    }

    // ---- broadcast ----

    #[test]
    fn blocks_broadcast() {
        assert!(!is_allowed_target("255.255.255.255".parse().unwrap()));
    }

    // ---- documentation ----

    #[test]
    fn blocks_documentation_ipv4() {
        assert!(!is_allowed_target("192.0.2.1".parse().unwrap()));
        assert!(!is_allowed_target("198.51.100.1".parse().unwrap()));
        assert!(!is_allowed_target("203.0.113.1".parse().unwrap()));
    }

    #[test]
    fn blocks_documentation_ipv6() {
        assert!(!is_allowed_target("2001:db8::1".parse().unwrap()));
        assert!(!is_allowed_target(
            "2001:db8:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()
        ));
    }

    // ---- ULA ----

    #[test]
    fn blocks_ipv6_ula() {
        assert!(!is_allowed_target("fc00::1".parse().unwrap()));
        assert!(!is_allowed_target("fd00::1".parse().unwrap()));
    }

    // ---- deprecated site-local ----

    #[test]
    fn blocks_deprecated_site_local() {
        assert!(!is_allowed_target("fec0::1".parse().unwrap()));
        assert!(!is_allowed_target("feff::1".parse().unwrap()));
    }

    // ---- IPv4-mapped IPv6 ----

    #[test]
    fn blocks_ipv4_mapped_private() {
        assert!(!is_allowed_target("::ffff:10.0.0.1".parse().unwrap()));
        assert!(!is_allowed_target("::ffff:192.168.1.1".parse().unwrap()));
        assert!(!is_allowed_target("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_allowed_target("::ffff:100.64.0.1".parse().unwrap()));
    }

    #[test]
    fn allows_ipv4_mapped_public() {
        assert!(is_allowed_target("::ffff:1.1.1.1".parse().unwrap()));
    }

    // ---- 6to4 ----

    #[test]
    fn blocks_6to4_private() {
        // 2002:c0a8:0101:: embeds 192.168.1.1
        assert!(!is_allowed_target("2002:c0a8:0101::".parse().unwrap()));
    }

    #[test]
    fn refuses_6to4_public() {
        // 2002:0101:0101:: embeds 1.1.1.1; the whole 2002::/16 range is refused
        assert!(!is_allowed_target("2002:0101:0101::".parse().unwrap()));
    }

    // ---- NAT64 ----

    #[test]
    fn blocks_nat64() {
        assert!(!is_allowed_target("64:ff9b::".parse().unwrap()));
        assert!(!is_allowed_target("64:ff9b::1".parse().unwrap()));
    }

    // ---- public IPs allowed ----

    #[test]
    fn allows_public_ipv4() {
        assert!(is_allowed_target("1.1.1.1".parse().unwrap()));
        assert!(is_allowed_target("8.8.8.8".parse().unwrap()));
        assert!(is_allowed_target("9.9.9.9".parse().unwrap()));
    }

    #[test]
    fn allows_public_ipv6() {
        assert!(is_allowed_target("2001:4860:4860::8888".parse().unwrap()));
        assert!(is_allowed_target("2606:4700:4700::1111".parse().unwrap()));
    }
}
