use std::net::IpAddr;
use std::str::FromStr;

use mhost::RecordType;
use mhost::nameserver::predefined::PredefinedProvider;

/// Maximum number of record types allowed in a single query (SDD section 8).
const MAX_RECORD_TYPES: usize = 10;

/// Default record types when none are specified and the domain is not an IP address.
const DEFAULT_RECORD_TYPES: [RecordType; 4] = [
    RecordType::A,
    RecordType::AAAA,
    RecordType::CNAME,
    RecordType::MX,
];

/// Record types that are unconditionally blocked for security reasons (SDD section 4.5).
const BLOCKED_TYPES: [RecordType; 3] = [RecordType::ANY, RecordType::AXFR, RecordType::IXFR];

/// Blocked type names that may not have `FromStr` mappings in mhost's `RecordType`.
/// We check these explicitly so they are caught even when the upstream crate
/// doesn't recognize the string.
const BLOCKED_TYPE_NAMES: [(&str, RecordType); 3] = [
    ("ANY", RecordType::ANY),
    ("AXFR", RecordType::AXFR),
    ("IXFR", RecordType::IXFR),
];

/// Record types excluded from ALL expansion (internal/meta types, blocked types).
const EXCLUDED_FROM_ALL: [RecordType; 7] = [
    RecordType::ANY,
    RecordType::AXFR,
    RecordType::IXFR,
    RecordType::OPT,
    RecordType::ZERO,
    RecordType::NULL,
    RecordType::ANAME,
];

/// Specifies which DNS server to query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSpec {
    /// A well-known public DNS provider (e.g., Cloudflare, Google).
    Predefined(PredefinedProvider),
    /// The host system's configured resolvers (/etc/resolv.conf).
    System,
    /// A specific IP address and port.
    Ip { addr: IpAddr, port: u16 },
}

/// DNS transport protocol to use for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
    Https,
}

/// The result of parsing a query string.
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// The target domain name, lowercased.
    pub domain: String,
    /// Record types to query. Populated with defaults if none specified.
    pub record_types: Vec<RecordType>,
    /// DNS servers to query. Empty means "use configured defaults".
    pub servers: Vec<ServerSpec>,
    /// Explicit transport override, if any.
    pub transport: Option<Transport>,
    /// Whether DNSSEC validation was requested.
    pub dnssec: bool,
    /// Non-fatal warnings accumulated during parsing.
    pub warnings: Vec<String>,
}

/// Errors that prevent query execution.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("empty query: no domain specified")]
    EmptyQuery,
    #[error("blocked query type: {0}")]
    BlockedQueryType(String),
    #[error("ALL expands to {0} record types, exceeding the limit of {1}")]
    AllExceedsLimit(usize, usize),
}

/// Parse a dig-inspired query string into structured query parameters.
///
/// The first whitespace-delimited token is always the domain. Remaining tokens
/// are record types, server specifications (`@...`), or flags (`+...`).
/// Unknown tokens produce warnings rather than errors.
///
/// # Errors
///
/// Returns `ParseError::EmptyQuery` if the input is empty or whitespace-only.
/// Returns `ParseError::BlockedQueryType` if ANY, AXFR, or IXFR is requested.
/// Returns `ParseError::AllExceedsLimit` if ALL expansion exceeds the limit.
pub fn parse(input: &str) -> Result<ParsedQuery, ParseError> {
    let mut tokens = input.split_whitespace();

    let domain_token = tokens.next().ok_or(ParseError::EmptyQuery)?;
    let domain = domain_token.to_ascii_lowercase();

    let is_ip_domain = domain.parse::<IpAddr>().is_ok();

    let mut record_types = Vec::new();
    let mut servers = Vec::new();
    let mut transport = None;
    let mut dnssec = false;
    let mut warnings = Vec::new();
    let mut has_all = false;

    for token in tokens {
        if let Some(server_name) = token.strip_prefix('@') {
            parse_server(server_name, &mut servers, &mut warnings);
        } else if let Some(flag_name) = token.strip_prefix('+') {
            parse_flag(flag_name, &mut transport, &mut dnssec, &mut warnings);
        } else {
            parse_record_type(token, &mut record_types, &mut has_all, &mut warnings);
        }
    }

    // Check for blocked types before ALL expansion, since the user may have
    // typed them explicitly.
    for rt in &record_types {
        if BLOCKED_TYPES.contains(rt) {
            return Err(ParseError::BlockedQueryType(rt.to_string()));
        }
    }

    // Handle ALL expansion.
    if has_all {
        let expanded = expand_all();
        if expanded.len() > MAX_RECORD_TYPES {
            return Err(ParseError::AllExceedsLimit(
                expanded.len(),
                MAX_RECORD_TYPES,
            ));
        }
        // ALL replaces any individually specified types -- the expansion is
        // the complete set (minus blocked/internal).
        record_types = expanded;
    }

    // Apply defaults when no record types were specified.
    if record_types.is_empty() {
        if is_ip_domain {
            record_types.push(RecordType::PTR);
        } else {
            record_types.extend_from_slice(&DEFAULT_RECORD_TYPES);
        }
    }

    Ok(ParsedQuery {
        domain,
        record_types,
        servers,
        transport,
        dnssec,
        warnings,
    })
}

/// Try to parse a token as a record type name (case-insensitive).
///
/// "ALL" is handled as a special expansion marker. Blocked types (ANY, AXFR, IXFR)
/// are allowed through here -- they are rejected in the caller after all tokens
/// are collected, so the error message is clear.
fn parse_record_type(
    token: &str,
    record_types: &mut Vec<RecordType>,
    has_all: &mut bool,
    warnings: &mut Vec<String>,
) {
    let upper = token.to_ascii_uppercase();

    if upper == "ALL" {
        *has_all = true;
        return;
    }

    // Check blocked type names explicitly, since mhost's FromStr may not
    // recognize all of them (e.g., "IXFR" has no FromStr mapping).
    for &(name, rt) in &BLOCKED_TYPE_NAMES {
        if upper == name {
            if !record_types.contains(&rt) {
                record_types.push(rt);
            }
            return;
        }
    }

    match RecordType::from_str(&upper) {
        Ok(rt) => {
            if !record_types.contains(&rt) {
                record_types.push(rt);
            }
        }
        Err(_) => {
            warnings.push(format!("unrecognized token: {token}"));
        }
    }
}

/// Parse a server specification after stripping the `@` prefix.
///
/// Tries, in order: predefined provider name, "system", IP:port, bare IP.
fn parse_server(name: &str, servers: &mut Vec<ServerSpec>, warnings: &mut Vec<String>) {
    if name.is_empty() {
        warnings.push("empty server specification after @".to_string());
        return;
    }

    // "system" (case-insensitive)
    if name.eq_ignore_ascii_case("system") {
        servers.push(ServerSpec::System);
        return;
    }

    // Predefined provider (case-insensitive via PredefinedProvider::from_str)
    if let Ok(provider) = PredefinedProvider::from_str(name) {
        servers.push(ServerSpec::Predefined(provider));
        return;
    }

    // IPv6 with port uses bracket notation: [::1]:5353
    // IPv6 without port: ::1 or [::1]
    if let Some(spec) = parse_bracketed_ipv6_port(name) {
        servers.push(spec);
        return;
    }

    // Try as IP:port (only valid for IPv4 -- "1.2.3.4:53")
    if let Some(spec) = parse_ipv4_with_port(name) {
        servers.push(spec);
        return;
    }

    // Try as bare IP address (IPv4 or IPv6), default port 53
    if let Ok(addr) = name.parse::<IpAddr>() {
        servers.push(ServerSpec::Ip { addr, port: 53 });
        return;
    }

    warnings.push(format!("unrecognized server: @{name}"));
}

/// Parse `[ipv6]:port` or `[ipv6]` bracket notation.
fn parse_bracketed_ipv6_port(s: &str) -> Option<ServerSpec> {
    let s = s.strip_prefix('[')?;
    if let Some((addr_str, rest)) = s.split_once(']') {
        let addr: IpAddr = addr_str.parse().ok()?;
        let port = if let Some(port_str) = rest.strip_prefix(':') {
            port_str.parse::<u16>().ok()?
        } else {
            53
        };
        Some(ServerSpec::Ip { addr, port })
    } else {
        None
    }
}

/// Parse `ipv4:port` notation. Only matches if the part before `:` is a valid IPv4.
fn parse_ipv4_with_port(s: &str) -> Option<ServerSpec> {
    let (addr_str, port_str) = s.rsplit_once(':')?;
    // Reject if addr_str contains another colon (would be IPv6, not IPv4:port)
    if addr_str.contains(':') {
        return None;
    }
    let addr: IpAddr = addr_str.parse().ok()?;
    let port = port_str.parse::<u16>().ok()?;
    Some(ServerSpec::Ip { addr, port })
}

/// Parse a flag name after stripping the `+` prefix.
fn parse_flag(
    name: &str,
    transport: &mut Option<Transport>,
    dnssec: &mut bool,
    warnings: &mut Vec<String>,
) {
    match name.to_ascii_lowercase().as_str() {
        "udp" => *transport = Some(Transport::Udp),
        "tcp" => *transport = Some(Transport::Tcp),
        "tls" => *transport = Some(Transport::Tls),
        "https" => *transport = Some(Transport::Https),
        "dnssec" => *dnssec = true,
        // +check and +trace are routing hints for the frontend; the backend
        // routes them via dedicated endpoints, not via query-string flags.
        "check" | "trace" => {}
        _ => {
            warnings.push(format!("unrecognized flag: +{name}"));
        }
    }
}

/// Expand ALL to all standard record types minus blocked and internal types.
fn expand_all() -> Vec<RecordType> {
    RecordType::all()
        .into_iter()
        .filter(|rt| !EXCLUDED_FROM_ALL.contains(rt))
        .filter(|rt| !matches!(rt, RecordType::Unknown(_)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Basic domain parsing
    // -----------------------------------------------------------------------

    #[test]
    fn bare_domain_uses_defaults() {
        let q = parse("example.com").unwrap();
        assert_eq!(q.domain, "example.com");
        assert_eq!(q.record_types, DEFAULT_RECORD_TYPES.to_vec());
        assert!(q.servers.is_empty());
        assert_eq!(q.transport, None);
        assert!(!q.dnssec);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn domain_is_lowercased() {
        let q = parse("Example.COM").unwrap();
        assert_eq!(q.domain, "example.com");
    }

    #[test]
    fn empty_input_is_error() {
        assert!(matches!(parse(""), Err(ParseError::EmptyQuery)));
    }

    #[test]
    fn whitespace_only_is_error() {
        assert!(matches!(parse("   "), Err(ParseError::EmptyQuery)));
    }

    // -----------------------------------------------------------------------
    // First token is always domain (rule 1)
    // -----------------------------------------------------------------------

    #[test]
    fn record_type_name_as_domain() {
        // "MX" in first position is a domain, not a record type.
        let q = parse("MX").unwrap();
        assert_eq!(q.domain, "mx");
        assert_eq!(q.record_types, DEFAULT_RECORD_TYPES.to_vec());
    }

    #[test]
    fn a_as_domain() {
        let q = parse("A").unwrap();
        assert_eq!(q.domain, "a");
        assert_eq!(q.record_types, DEFAULT_RECORD_TYPES.to_vec());
    }

    // -----------------------------------------------------------------------
    // Record type parsing (rule 2, 3)
    // -----------------------------------------------------------------------

    #[test]
    fn single_record_type() {
        let q = parse("example.com MX").unwrap();
        assert_eq!(q.record_types, vec![RecordType::MX]);
    }

    #[test]
    fn multiple_record_types() {
        let q = parse("example.com A AAAA MX TXT").unwrap();
        assert_eq!(
            q.record_types,
            vec![
                RecordType::A,
                RecordType::AAAA,
                RecordType::MX,
                RecordType::TXT
            ]
        );
    }

    #[test]
    fn record_types_case_insensitive() {
        let q = parse("example.com mx Txt aaaa").unwrap();
        assert_eq!(
            q.record_types,
            vec![RecordType::MX, RecordType::TXT, RecordType::AAAA]
        );
    }

    #[test]
    fn duplicate_record_types_deduplicated() {
        let q = parse("example.com A A AAAA A").unwrap();
        assert_eq!(q.record_types, vec![RecordType::A, RecordType::AAAA]);
    }

    #[test]
    fn all_standard_record_types() {
        // Verify each individually recognized record type from the SDD grammar.
        let types = [
            "A",
            "AAAA",
            "MX",
            "TXT",
            "NS",
            "SOA",
            "CNAME",
            "CAA",
            "SRV",
            "PTR",
            "HTTPS",
            "SVCB",
            "SSHFP",
            "TLSA",
            "NAPTR",
            "HINFO",
            "OPENPGPKEY",
            "DNSKEY",
            "DS",
        ];
        for t in types {
            let input = format!("example.com {t}");
            let q = parse(&input).unwrap();
            assert_eq!(q.record_types.len(), 1, "expected 1 type for {t}");
            assert_eq!(
                q.record_types[0],
                RecordType::from_str(t).unwrap(),
                "mismatch for {t}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // IP address domain -> PTR default (rule 8)
    // -----------------------------------------------------------------------

    #[test]
    fn ipv4_domain_defaults_to_ptr() {
        let q = parse("192.0.2.1").unwrap();
        assert_eq!(q.domain, "192.0.2.1");
        assert_eq!(q.record_types, vec![RecordType::PTR]);
    }

    #[test]
    fn ipv6_domain_defaults_to_ptr() {
        let q = parse("2001:db8::1").unwrap();
        assert_eq!(q.domain, "2001:db8::1");
        assert_eq!(q.record_types, vec![RecordType::PTR]);
    }

    #[test]
    fn ip_domain_with_explicit_types_overrides_ptr() {
        let q = parse("192.0.2.1 A").unwrap();
        assert_eq!(q.domain, "192.0.2.1");
        assert_eq!(q.record_types, vec![RecordType::A]);
    }

    // -----------------------------------------------------------------------
    // Blocked types (rule 9)
    // -----------------------------------------------------------------------

    #[test]
    fn any_is_blocked() {
        let err = parse("example.com ANY").unwrap_err();
        assert!(matches!(err, ParseError::BlockedQueryType(ref s) if s == "ANY"));
    }

    #[test]
    fn axfr_is_blocked() {
        let err = parse("example.com AXFR").unwrap_err();
        assert!(matches!(err, ParseError::BlockedQueryType(ref s) if s == "AXFR"));
    }

    #[test]
    fn ixfr_is_blocked() {
        let err = parse("example.com IXFR").unwrap_err();
        assert!(matches!(err, ParseError::BlockedQueryType(ref s) if s == "IXFR"));
    }

    #[test]
    fn blocked_types_case_insensitive() {
        let err = parse("example.com any").unwrap_err();
        assert!(matches!(err, ParseError::BlockedQueryType(_)));
    }

    #[test]
    fn blocked_type_with_valid_types() {
        // Even if valid types are present, a blocked type causes an error.
        let err = parse("example.com A ANY MX").unwrap_err();
        assert!(matches!(err, ParseError::BlockedQueryType(ref s) if s == "ANY"));
    }

    // -----------------------------------------------------------------------
    // ALL expansion (rule 7)
    // -----------------------------------------------------------------------

    #[test]
    fn all_exceeds_limit() {
        let err = parse("example.com ALL").unwrap_err();
        assert!(matches!(err, ParseError::AllExceedsLimit(n, 10) if n > 10));
    }

    #[test]
    fn all_case_insensitive() {
        let err = parse("example.com all").unwrap_err();
        assert!(matches!(err, ParseError::AllExceedsLimit(_, _)));
    }

    #[test]
    fn all_expansion_excludes_blocked_and_internal() {
        let expanded = expand_all();
        assert!(!expanded.contains(&RecordType::ANY));
        assert!(!expanded.contains(&RecordType::AXFR));
        assert!(!expanded.contains(&RecordType::IXFR));
        assert!(!expanded.contains(&RecordType::OPT));
        assert!(!expanded.contains(&RecordType::ZERO));
        assert!(!expanded.contains(&RecordType::NULL));
        assert!(!expanded.contains(&RecordType::ANAME));
        // Should contain normal types.
        assert!(expanded.contains(&RecordType::A));
        assert!(expanded.contains(&RecordType::AAAA));
        assert!(expanded.contains(&RecordType::MX));
        assert!(expanded.contains(&RecordType::TXT));
    }

    // -----------------------------------------------------------------------
    // Server parsing
    // -----------------------------------------------------------------------

    #[test]
    fn predefined_provider() {
        let q = parse("example.com @cloudflare").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Predefined(PredefinedProvider::Cloudflare)]
        );
    }

    #[test]
    fn predefined_provider_case_insensitive() {
        let q = parse("example.com @Google").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Predefined(PredefinedProvider::Google)]
        );
    }

    #[test]
    fn all_predefined_providers() {
        for provider in PredefinedProvider::all() {
            let input = format!("example.com @{provider}");
            let q = parse(&input).unwrap();
            assert_eq!(q.servers, vec![ServerSpec::Predefined(*provider)]);
        }
    }

    #[test]
    fn system_server() {
        let q = parse("example.com @system").unwrap();
        assert_eq!(q.servers, vec![ServerSpec::System]);
    }

    #[test]
    fn system_case_insensitive() {
        let q = parse("example.com @SYSTEM").unwrap();
        assert_eq!(q.servers, vec![ServerSpec::System]);
    }

    #[test]
    fn ipv4_server() {
        let q = parse("example.com @198.51.100.1").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "198.51.100.1".parse().unwrap(),
                port: 53,
            }]
        );
    }

    #[test]
    fn ipv4_server_with_port() {
        let q = parse("example.com @198.51.100.1:5353").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "198.51.100.1".parse().unwrap(),
                port: 5353,
            }]
        );
    }

    #[test]
    fn ipv6_server() {
        let q = parse("example.com @2001:db8::1").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "2001:db8::1".parse().unwrap(),
                port: 53,
            }]
        );
    }

    #[test]
    fn ipv6_server_bracketed() {
        let q = parse("example.com @[2001:db8::1]").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "2001:db8::1".parse().unwrap(),
                port: 53,
            }]
        );
    }

    #[test]
    fn ipv6_server_bracketed_with_port() {
        let q = parse("example.com @[2001:db8::1]:5353").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "2001:db8::1".parse().unwrap(),
                port: 5353,
            }]
        );
    }

    #[test]
    fn multiple_servers() {
        let q = parse("example.com @google @quad9").unwrap();
        assert_eq!(
            q.servers,
            vec![
                ServerSpec::Predefined(PredefinedProvider::Google),
                ServerSpec::Predefined(PredefinedProvider::Quad9),
            ]
        );
    }

    #[test]
    fn mixed_server_types() {
        let q = parse("example.com @cloudflare @198.51.100.1:53 @system").unwrap();
        assert_eq!(
            q.servers,
            vec![
                ServerSpec::Predefined(PredefinedProvider::Cloudflare),
                ServerSpec::Ip {
                    addr: "198.51.100.1".parse().unwrap(),
                    port: 53,
                },
                ServerSpec::System,
            ]
        );
    }

    #[test]
    fn unknown_server_produces_warning() {
        let q = parse("example.com @notaprovider").unwrap();
        assert!(q.servers.is_empty());
        assert_eq!(q.warnings.len(), 1);
        assert!(q.warnings[0].contains("notaprovider"));
    }

    #[test]
    fn empty_server_after_at_produces_warning() {
        // This can only happen if the token is exactly "@" (whitespace-split
        // means "@" is one token, and strip_prefix('@') yields "").
        let q = parse("example.com @").unwrap();
        assert!(q.servers.is_empty());
        assert_eq!(q.warnings.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Flag parsing
    // -----------------------------------------------------------------------

    #[test]
    fn transport_udp() {
        let q = parse("example.com +udp").unwrap();
        assert_eq!(q.transport, Some(Transport::Udp));
    }

    #[test]
    fn transport_tcp() {
        let q = parse("example.com +tcp").unwrap();
        assert_eq!(q.transport, Some(Transport::Tcp));
    }

    #[test]
    fn transport_tls() {
        let q = parse("example.com +tls").unwrap();
        assert_eq!(q.transport, Some(Transport::Tls));
    }

    #[test]
    fn transport_https() {
        let q = parse("example.com +https").unwrap();
        assert_eq!(q.transport, Some(Transport::Https));
    }

    #[test]
    fn flag_dnssec() {
        let q = parse("example.com +dnssec").unwrap();
        assert!(q.dnssec);
    }

    #[test]
    fn flag_check_parses_without_error() {
        // +check is a routing hint; the parser accepts it silently.
        let q = parse("example.com +check").unwrap();
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn flag_trace_parses_without_error() {
        // +trace is a routing hint; the parser accepts it silently.
        let q = parse("example.com +trace").unwrap();
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn flags_case_insensitive() {
        let q = parse("example.com +TLS +DNSSEC").unwrap();
        assert_eq!(q.transport, Some(Transport::Tls));
        assert!(q.dnssec);
    }

    #[test]
    fn unknown_flag_produces_warning() {
        let q = parse("example.com +nosuchflag").unwrap();
        assert_eq!(q.warnings.len(), 1);
        assert!(q.warnings[0].contains("nosuchflag"));
    }

    #[test]
    fn last_transport_wins() {
        let q = parse("example.com +udp +tls").unwrap();
        assert_eq!(q.transport, Some(Transport::Tls));
    }

    // -----------------------------------------------------------------------
    // Unknown tokens (rule 5)
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_token_produces_warning() {
        let q = parse("example.com FOOBAR").unwrap();
        assert_eq!(q.warnings.len(), 1);
        assert!(q.warnings[0].contains("FOOBAR"));
        // Defaults should still apply since no valid types.
        assert_eq!(q.record_types, DEFAULT_RECORD_TYPES.to_vec());
    }

    #[test]
    fn multiple_unknown_tokens() {
        let q = parse("example.com foo bar baz").unwrap();
        assert_eq!(q.warnings.len(), 3);
    }

    #[test]
    fn unknown_tokens_do_not_prevent_valid_types() {
        let q = parse("example.com MX UNKNOWN TXT").unwrap();
        assert_eq!(q.record_types, vec![RecordType::MX, RecordType::TXT]);
        assert_eq!(q.warnings.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Complex / integration queries
    // -----------------------------------------------------------------------

    #[test]
    fn full_complex_query() {
        let q = parse("example.com A AAAA @cloudflare @google +tls +dnssec").unwrap();
        assert_eq!(q.domain, "example.com");
        assert_eq!(q.record_types, vec![RecordType::A, RecordType::AAAA]);
        assert_eq!(
            q.servers,
            vec![
                ServerSpec::Predefined(PredefinedProvider::Cloudflare),
                ServerSpec::Predefined(PredefinedProvider::Google),
            ]
        );
        assert_eq!(q.transport, Some(Transport::Tls));
        assert!(q.dnssec);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn tokens_position_independent() {
        // Record types, servers, and flags can appear in any order after the domain.
        let q1 = parse("example.com MX @cloudflare +tls").unwrap();
        let q2 = parse("example.com +tls @cloudflare MX").unwrap();
        let q3 = parse("example.com @cloudflare MX +tls").unwrap();

        assert_eq!(q1.record_types, q2.record_types);
        assert_eq!(q1.record_types, q3.record_types);
        assert_eq!(q1.servers, q2.servers);
        assert_eq!(q1.servers, q3.servers);
        assert_eq!(q1.transport, q2.transport);
        assert_eq!(q1.transport, q3.transport);
    }

    #[test]
    fn ip_domain_with_server_and_flags() {
        let q = parse("192.0.2.1 @google +tcp").unwrap();
        assert_eq!(q.domain, "192.0.2.1");
        assert_eq!(q.record_types, vec![RecordType::PTR]);
        assert_eq!(
            q.servers,
            vec![ServerSpec::Predefined(PredefinedProvider::Google)]
        );
        assert_eq!(q.transport, Some(Transport::Tcp));
    }

    #[test]
    fn check_flag_with_types_and_server() {
        let q = parse("example.com MX TXT @system +check").unwrap();
        assert_eq!(q.record_types, vec![RecordType::MX, RecordType::TXT]);
        assert_eq!(q.servers, vec![ServerSpec::System]);
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn extra_whitespace_handled() {
        let q = parse("  example.com   A   AAAA   ").unwrap();
        assert_eq!(q.domain, "example.com");
        assert_eq!(q.record_types, vec![RecordType::A, RecordType::AAAA]);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn single_domain_only() {
        let q = parse("example.com").unwrap();
        assert!(q.servers.is_empty());
        assert!(q.warnings.is_empty());
    }

    #[test]
    fn subdomain_with_many_labels() {
        let q = parse("deeply.nested.sub.example.com A").unwrap();
        assert_eq!(q.domain, "deeply.nested.sub.example.com");
        assert_eq!(q.record_types, vec![RecordType::A]);
    }

    #[test]
    fn trailing_dot_preserved() {
        // FQDN notation: "example.com." -- the parser preserves it.
        let q = parse("example.com.").unwrap();
        assert_eq!(q.domain, "example.com.");
    }

    #[test]
    fn dnssec_related_types() {
        let q = parse("example.com DNSKEY DS NSEC RRSIG").unwrap();
        assert_eq!(
            q.record_types,
            vec![
                RecordType::DNSKEY,
                RecordType::DS,
                RecordType::NSEC,
                RecordType::RRSIG,
            ]
        );
    }

    #[test]
    fn ipv4_server_port_zero_is_valid_u16() {
        // Port 0 is technically valid at the parsing layer.
        let q = parse("example.com @198.51.100.1:0").unwrap();
        assert_eq!(
            q.servers,
            vec![ServerSpec::Ip {
                addr: "198.51.100.1".parse().unwrap(),
                port: 0,
            }]
        );
    }

    #[test]
    fn invalid_port_produces_warning() {
        let q = parse("example.com @198.51.100.1:99999").unwrap();
        assert!(q.servers.is_empty());
        assert_eq!(q.warnings.len(), 1);
    }

    #[test]
    fn at_sign_with_invalid_ip_format() {
        let q = parse("example.com @999.999.999.999").unwrap();
        assert!(q.servers.is_empty());
        assert_eq!(q.warnings.len(), 1);
    }

    // -----------------------------------------------------------------------
    // Display / Error messages
    // -----------------------------------------------------------------------

    #[test]
    fn error_display_empty_query() {
        let err = parse("").unwrap_err();
        assert_eq!(err.to_string(), "empty query: no domain specified");
    }

    #[test]
    fn error_display_blocked_type() {
        let err = parse("example.com ANY").unwrap_err();
        assert_eq!(err.to_string(), "blocked query type: ANY");
    }

    #[test]
    fn error_display_all_exceeds_limit() {
        let err = parse("example.com ALL").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ALL expands to"));
        assert!(msg.contains("exceeding the limit of 10"));
    }
}
