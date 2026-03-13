use std::net::IpAddr;

use crate::config::Config;
use crate::error::ApiError;
use crate::parser::{ParsedQuery, ServerSpec};

/// Validates parsed queries against security restrictions.
///
/// Enforces the hardcoded caps from SDD SS8.1 and per-deployment config limits.
/// Validation runs **before** any DNS queries are issued (fail-fast).
pub struct QueryPolicy<'a> {
    config: &'a Config,
}

impl<'a> QueryPolicy<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    /// Validate a parsed query against all policy rules.
    ///
    /// Returns `Ok(())` if the query is allowed, or the first policy violation found.
    pub fn validate(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        self.check_domain_length(query)?;
        self.check_record_type_count(query)?;
        self.check_server_count(query)?;
        self.check_system_resolvers(query)?;
        self.check_arbitrary_servers(query)?;
        self.check_server_ips(query)?;
        Ok(())
    }

    /// Validate a check request against server policy rules.
    ///
    /// Skips record-type-count check (irrelevant for the dedicated check
    /// endpoint which uses a fixed set of 15+1 record types).
    pub fn validate_for_check(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        self.check_domain_length(query)?;
        self.check_server_count(query)?;
        self.check_system_resolvers(query)?;
        self.check_arbitrary_servers(query)?;
        self.check_server_ips(query)?;
        Ok(())
    }

    fn check_domain_length(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        // DNS protocol limit (RFC 1035): max 253 octets for the text representation.
        if query.domain.len() > 253 {
            return Err(ApiError::InvalidDomain(format!(
                "domain exceeds maximum length of 253 characters (got {})",
                query.domain.len()
            )));
        }
        Ok(())
    }

    fn check_record_type_count(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        let count = query.record_types.len();
        let max = self.config.limits.max_record_types;
        if count > max {
            return Err(ApiError::TooManyRecordTypes {
                requested: count,
                max,
            });
        }
        Ok(())
    }

    fn check_server_count(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        let count = query.servers.len();
        let max = self.config.limits.max_servers;
        if count > max {
            return Err(ApiError::TooManyServers {
                requested: count,
                max,
            });
        }
        Ok(())
    }

    fn check_system_resolvers(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        if !self.config.dns.allow_system_resolvers
            && query
                .servers
                .iter()
                .any(|s| matches!(s, ServerSpec::System))
        {
            return Err(ApiError::SystemResolversDisabled);
        }
        Ok(())
    }

    fn check_arbitrary_servers(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        if !self.config.dns.allow_arbitrary_servers
            && query
                .servers
                .iter()
                .any(|s| matches!(s, ServerSpec::Ip { .. }))
        {
            return Err(ApiError::ArbitraryServersDisabled);
        }
        Ok(())
    }

    fn check_server_ips(&self, query: &ParsedQuery) -> Result<(), ApiError> {
        for server in &query.servers {
            if let ServerSpec::Ip { addr, .. } = server {
                is_allowed_target(*addr)?;
            }
        }
        Ok(())
    }
}

/// Validate that a target IP address is safe to query.
///
/// Rejects loopback, unspecified, multicast, private (RFC 1918), link-local,
/// CGNAT (RFC 6598), documentation, and IPv6 unique-local (ULA) addresses.
/// This prevents the service from being used to probe internal networks.
pub(crate) fn is_allowed_target(ip: IpAddr) -> Result<(), ApiError> {
    if ip.is_loopback() {
        return Err(blocked_ip(ip, "loopback address"));
    }
    if ip.is_unspecified() {
        return Err(blocked_ip(ip, "unspecified address"));
    }
    if ip.is_multicast() {
        return Err(blocked_ip(ip, "multicast address"));
    }
    if is_rfc1918(ip) {
        return Err(blocked_ip(ip, "private network (RFC 1918)"));
    }
    if is_link_local(ip) {
        return Err(blocked_ip(ip, "link-local address"));
    }
    if is_cgnat(ip) {
        return Err(blocked_ip(ip, "CGNAT address (RFC 6598)"));
    }
    if is_documentation(ip) {
        return Err(blocked_ip(ip, "documentation address"));
    }
    if is_ipv6_ula(ip) {
        return Err(blocked_ip(ip, "IPv6 unique-local address (ULA, fc00::/7)"));
    }
    Ok(())
}

fn blocked_ip(ip: IpAddr, reason: &str) -> ApiError {
    ApiError::BlockedTargetIp {
        ip: ip.to_string(),
        reason: reason.to_string(),
    }
}

fn is_rfc1918(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 10.0.0.0/8
            octets[0] == 10
            // 172.16.0.0/12
            || (octets[0] == 172 && (octets[1] & 0xF0) == 16)
            // 192.168.0.0/16
            || (octets[0] == 192 && octets[1] == 168)
        }
        IpAddr::V6(_) => false,
    }
}

fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 169.254.0.0/16
            octets[0] == 169 && octets[1] == 254
        }
        IpAddr::V6(v6) => {
            // fe80::/10 — first 10 bits are 1111_1110_10
            let segments = v6.segments();
            (segments[0] & 0xFFC0) == 0xFE80
        }
    }
}

fn is_cgnat(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 100.64.0.0/10 — first octet 100, second octet 64..127
            octets[0] == 100 && (octets[1] & 0xC0) == 64
        }
        IpAddr::V6(_) => false,
    }
}

fn is_ipv6_ula(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V6(v6) => {
            // fc00::/7 — first 7 bits are 1111_110x
            (v6.segments()[0] & 0xfe00) == 0xfc00
        }
        IpAddr::V4(_) => false,
    }
}

fn is_documentation(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 192.0.2.0/24 (TEST-NET-1)
            (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
            // 198.51.100.0/24 (TEST-NET-2)
            || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
            // 203.0.113.0/24 (TEST-NET-3)
            || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        }
        IpAddr::V6(v6) => {
            // 2001:db8::/32
            v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    // ---- domain length ----

    fn test_config() -> crate::config::Config {
        use crate::config::{
            CircuitBreakerConfig, Config, DnsConfig, EcosystemConfig, LimitsConfig, ServerConfig,
            TelemetryConfig, TraceConfig,
        };
        Config {
            site_name: "prism".to_string(),
            server: ServerConfig {
                bind: ([127, 0, 0, 1], 8080).into(),
                metrics_bind: ([127, 0, 0, 1], 9090).into(),
                trusted_proxies: vec![],
            },
            limits: LimitsConfig {
                per_ip_per_minute: 120,
                per_ip_burst: 40,
                per_target_per_minute: 60,
                per_target_burst: 20,
                global_per_minute: 1000,
                global_burst: 50,
                max_concurrent_connections: 256,
                per_ip_max_streams: 10,
                max_timeout_secs: 10,
                max_record_types: 10,
                max_servers: 4,
            },
            circuit_breaker: CircuitBreakerConfig {
                window_secs: 60,
                cooldown_secs: 30,
                failure_threshold: 0.5,
                min_requests: 5,
            },
            dns: DnsConfig {
                default_servers: vec!["cloudflare".to_owned()],
                allow_system_resolvers: true,
                allow_arbitrary_servers: false,
            },
            trace: TraceConfig {
                max_hops: 10,
                query_timeout_secs: 3,
            },
            telemetry: TelemetryConfig::default(),
            ecosystem: EcosystemConfig::default(),
        }
    }

    fn make_query(domain: &str) -> ParsedQuery {
        ParsedQuery {
            domain: domain.to_owned(),
            record_types: vec![],
            servers: vec![],
            transport: None,
            dnssec: false,
            warnings: vec![],
        }
    }

    #[test]
    fn domain_length_exactly_253_is_allowed() {
        let config = test_config();
        let policy = QueryPolicy::new(&config);
        // 253 'a' chars is at the limit.
        let q = make_query(&"a".repeat(253));
        assert!(policy.validate(&q).is_ok());
    }

    #[test]
    fn domain_length_254_is_rejected() {
        let config = test_config();
        let policy = QueryPolicy::new(&config);
        let q = make_query(&"a".repeat(254));
        assert!(matches!(
            policy.validate(&q),
            Err(ApiError::InvalidDomain(_))
        ));
    }

    // ---- is_allowed_target: blocked ranges ----

    #[test]
    fn rejects_ipv4_loopback() {
        assert!(is_allowed_target(IpAddr::V4(Ipv4Addr::LOCALHOST)).is_err());
        assert!(is_allowed_target("127.0.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("127.0.0.2".parse().unwrap()).is_err());
        assert!(is_allowed_target("127.255.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_ipv6_loopback() {
        assert!(is_allowed_target(IpAddr::V6(Ipv6Addr::LOCALHOST)).is_err());
    }

    #[test]
    fn rejects_ipv4_unspecified() {
        assert!(is_allowed_target(IpAddr::V4(Ipv4Addr::UNSPECIFIED)).is_err());
    }

    #[test]
    fn rejects_ipv6_unspecified() {
        assert!(is_allowed_target(IpAddr::V6(Ipv6Addr::UNSPECIFIED)).is_err());
    }

    #[test]
    fn rejects_ipv4_multicast() {
        assert!(is_allowed_target("224.0.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("239.255.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_ipv6_multicast() {
        assert!(is_allowed_target("ff02::1".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_rfc1918_10_slash_8() {
        assert!(is_allowed_target("10.0.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("10.0.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("10.255.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_rfc1918_172_16_slash_12() {
        assert!(is_allowed_target("172.16.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("172.16.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("172.31.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn allows_172_outside_rfc1918() {
        // 172.15.x.x is below the /12 range
        assert!(is_allowed_target("172.15.255.255".parse().unwrap()).is_ok());
        // 172.32.x.x is above the /12 range
        assert!(is_allowed_target("172.32.0.0".parse().unwrap()).is_ok());
    }

    #[test]
    fn rejects_rfc1918_192_168_slash_16() {
        assert!(is_allowed_target("192.168.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("192.168.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("192.168.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_ipv4_link_local() {
        assert!(is_allowed_target("169.254.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("169.254.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("169.254.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_ipv6_link_local() {
        assert!(is_allowed_target("fe80::1".parse().unwrap()).is_err());
        assert!(is_allowed_target("fe80::ffff:ffff:ffff:ffff".parse().unwrap()).is_err());
        // febf:: is still within fe80::/10 (first 10 bits = 1111_1110_10)
        assert!(is_allowed_target("febf::1".parse().unwrap()).is_err());
    }

    #[test]
    fn allows_ipv6_outside_link_local() {
        // fec0:: has first 10 bits = 1111_1110_11, outside fe80::/10
        assert!(is_allowed_target("fec0::1".parse().unwrap()).is_ok());
    }

    #[test]
    fn rejects_cgnat() {
        assert!(is_allowed_target("100.64.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("100.64.0.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("100.127.255.255".parse().unwrap()).is_err());
    }

    #[test]
    fn allows_100_outside_cgnat() {
        // 100.63.x.x is below the /10 range
        assert!(is_allowed_target("100.63.255.255".parse().unwrap()).is_ok());
        // 100.128.x.x is above the /10 range
        assert!(is_allowed_target("100.128.0.0".parse().unwrap()).is_ok());
    }

    #[test]
    fn rejects_documentation_test_net_1() {
        assert!(is_allowed_target("192.0.2.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("192.0.2.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("192.0.2.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_documentation_test_net_2() {
        assert!(is_allowed_target("198.51.100.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("198.51.100.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("198.51.100.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_documentation_test_net_3() {
        assert!(is_allowed_target("203.0.113.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("203.0.113.1".parse().unwrap()).is_err());
        assert!(is_allowed_target("203.0.113.255".parse().unwrap()).is_err());
    }

    #[test]
    fn rejects_documentation_ipv6() {
        assert!(is_allowed_target("2001:db8::1".parse().unwrap()).is_err());
        assert!(
            is_allowed_target("2001:db8:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_err()
        );
    }

    #[test]
    fn rejects_ipv6_ula_fc00() {
        assert!(is_allowed_target("fc00::1".parse().unwrap()).is_err());
        assert!(
            is_allowed_target("fc00:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_err()
        );
    }

    #[test]
    fn rejects_ipv6_ula_fd00() {
        assert!(is_allowed_target("fd00::1".parse().unwrap()).is_err());
        assert!(
            is_allowed_target("fdff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_err()
        );
    }

    #[test]
    fn allows_ipv6_outside_ula() {
        // fe00:: has first 7 bits = 1111_111x, outside fc00::/7
        assert!(is_allowed_target("fe00::1".parse().unwrap()).is_ok());
        // fb00:: has first 7 bits = 1111_101x, outside fc00::/7
        assert!(is_allowed_target("fb00::1".parse().unwrap()).is_ok());
    }

    // ---- is_allowed_target: allowed public IPs ----

    #[test]
    fn allows_cloudflare_dns() {
        assert!(is_allowed_target("1.1.1.1".parse().unwrap()).is_ok());
        assert!(is_allowed_target("1.0.0.1".parse().unwrap()).is_ok());
    }

    #[test]
    fn allows_google_dns() {
        assert!(is_allowed_target("8.8.8.8".parse().unwrap()).is_ok());
        assert!(is_allowed_target("8.8.4.4".parse().unwrap()).is_ok());
    }

    #[test]
    fn allows_quad9() {
        assert!(is_allowed_target("9.9.9.9".parse().unwrap()).is_ok());
    }

    #[test]
    fn allows_public_ipv6() {
        assert!(is_allowed_target("2606:4700:4700::1111".parse().unwrap()).is_ok());
        assert!(is_allowed_target("2001:4860:4860::8888".parse().unwrap()).is_ok());
    }

    // ---- is_allowed_target: boundary edge cases ----

    #[test]
    fn boundary_rfc1918_172_range() {
        // Last address inside 172.16.0.0/12
        assert!(is_allowed_target("172.31.255.255".parse().unwrap()).is_err());
        // First address outside 172.16.0.0/12
        assert!(is_allowed_target("172.32.0.0".parse().unwrap()).is_ok());
    }

    #[test]
    fn boundary_cgnat_range() {
        // Last address inside 100.64.0.0/10
        assert!(is_allowed_target("100.127.255.255".parse().unwrap()).is_err());
        // First address outside 100.64.0.0/10
        assert!(is_allowed_target("100.128.0.0".parse().unwrap()).is_ok());
        // Address just before CGNAT range
        assert!(is_allowed_target("100.63.255.255".parse().unwrap()).is_ok());
    }

    #[test]
    fn boundary_documentation_ranges() {
        // Just before TEST-NET-1
        assert!(is_allowed_target("192.0.1.255".parse().unwrap()).is_ok());
        // Just after TEST-NET-1
        assert!(is_allowed_target("192.0.3.0".parse().unwrap()).is_ok());

        // Just before TEST-NET-2
        assert!(is_allowed_target("198.51.99.255".parse().unwrap()).is_ok());
        // Just after TEST-NET-2
        assert!(is_allowed_target("198.51.101.0".parse().unwrap()).is_ok());

        // Just before TEST-NET-3
        assert!(is_allowed_target("203.0.112.255".parse().unwrap()).is_ok());
        // Just after TEST-NET-3
        assert!(is_allowed_target("203.0.114.0".parse().unwrap()).is_ok());
    }

    #[test]
    fn boundary_ipv6_link_local() {
        // fe80::/10 ends at febf:ffff:...:ffff
        assert!(
            is_allowed_target("febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_err()
        );
        // fec0:: is outside fe80::/10
        assert!(is_allowed_target("fec0::".parse().unwrap()).is_ok());
        // fe7f:: is outside fe80::/10 (first 10 bits = 1111_1110_01)
        assert!(is_allowed_target("fe7f::1".parse().unwrap()).is_ok());
    }

    #[test]
    fn boundary_ipv6_documentation() {
        // 2001:db8::/32 — last address in range
        assert!(
            is_allowed_target("2001:0db8:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_err()
        );
        // First address outside the /32
        assert!(is_allowed_target("2001:0db9::1".parse().unwrap()).is_ok());
        // Just before the range
        assert!(
            is_allowed_target("2001:0db7:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap()).is_ok()
        );
    }

    #[test]
    fn boundary_loopback_v4() {
        // 127.0.0.0/8 — first address
        assert!(is_allowed_target("127.0.0.0".parse().unwrap()).is_err());
        // Just outside
        assert!(is_allowed_target("126.255.255.255".parse().unwrap()).is_ok());
        assert!(is_allowed_target("128.0.0.0".parse().unwrap()).is_ok());
    }

    #[test]
    fn boundary_multicast_v4() {
        // 224.0.0.0/4 (224.0.0.0 - 239.255.255.255)
        assert!(is_allowed_target("224.0.0.0".parse().unwrap()).is_err());
        assert!(is_allowed_target("223.255.255.255".parse().unwrap()).is_ok());
    }
}
