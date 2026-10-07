use std::net::IpAddr;

use crate::error::AppError;

/// Parsed inspection target.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedInput {
    pub target: Target,
    pub ports: Vec<u16>,
}

/// Target is either a hostname (for SNI) or a raw IP address.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Hostname(String),
    Ip(IpAddr),
}

impl Target {
    /// Returns the hostname string for SNI, or None for IP targets.
    pub fn hostname(&self) -> Option<&str> {
        match self {
            Target::Hostname(h) => Some(h),
            Target::Ip(_) => None,
        }
    }
}

/// Parse raw input string into a structured inspection target.
///
/// Accepts: `hostname`, `hostname:port`, `hostname:port,port,...`,
/// `1.2.3.4`, `1.2.3.4:port`, `[::1]`, `[::1]:port`.
///
/// Percent-encoded colons (%3A) and commas (%2C) are decoded first.
pub fn parse_input(raw: &str, max_ports: usize) -> Result<ParsedInput, AppError> {
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|e| AppError::ParseError(format!("invalid UTF-8 in input: {e}")))?;
    let input = decoded.trim();

    if input.is_empty() {
        return Err(AppError::ParseError("empty input".to_string()));
    }

    let (target_str, port_str) = split_target_and_ports(input)?;
    let target = parse_target(target_str)?;
    let ports = parse_ports(port_str, max_ports)?;

    Ok(ParsedInput { target, ports })
}

/// Split input into target and optional port portion.
fn split_target_and_ports(input: &str) -> Result<(&str, Option<&str>), AppError> {
    // IPv6 in brackets: [::1] or [::1]:port
    if input.starts_with('[') {
        let Some(bracket_end) = input.find(']') else {
            return Err(AppError::ParseError(
                "unclosed bracket in IPv6 address".to_string(),
            ));
        };
        let target = &input[1..bracket_end];
        let rest = &input[bracket_end + 1..];
        if rest.is_empty() {
            Ok((target, None))
        } else if let Some(ports) = rest.strip_prefix(':') {
            Ok((target, Some(ports)))
        } else {
            Err(AppError::ParseError(format!(
                "unexpected characters after bracket: {rest}"
            )))
        }
    } else {
        // Check if it looks like an IPv4 address (contains dots but no alpha after last colon)
        // or a hostname. Split on last colon that's followed by digits/commas only.
        match input.rfind(':') {
            Some(colon_pos) => {
                let after = &input[colon_pos + 1..];
                // If everything after the last colon is digits and commas, treat as port spec
                if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit() || c == ',') {
                    let target = &input[..colon_pos];
                    Ok((target, Some(after)))
                } else {
                    // Could be IPv6 without brackets — reject
                    Err(AppError::ParseError(
                        "IPv6 addresses must be enclosed in brackets (e.g. [::1])".to_string(),
                    ))
                }
            }
            None => Ok((input, None)),
        }
    }
}

fn parse_target(target_str: &str) -> Result<Target, AppError> {
    if target_str.is_empty() {
        return Err(AppError::ParseError("empty target".to_string()));
    }

    // Try parsing as IP first
    if let Ok(ip) = target_str.parse::<IpAddr>() {
        return Ok(Target::Ip(ip));
    }

    // Strip trailing dot before validation and storage
    let hostname = target_str.strip_suffix('.').unwrap_or(target_str);

    // Validate as hostname
    validate_hostname(hostname)?;
    Ok(Target::Hostname(hostname.to_string()))
}

fn parse_ports(port_str: Option<&str>, max_ports: usize) -> Result<Vec<u16>, AppError> {
    let Some(ports_raw) = port_str else {
        return Ok(vec![443]);
    };

    if ports_raw.is_empty() {
        return Err(AppError::InvalidPort(
            "empty port specification".to_string(),
        ));
    }

    let ports: Vec<u16> = ports_raw
        .split(',')
        .map(|p| {
            let trimmed = p.trim();
            if trimmed.is_empty() {
                return Err(AppError::InvalidPort("empty port in list".to_string()));
            }
            trimmed
                .parse::<u16>()
                .map_err(|_| AppError::InvalidPort(format!("invalid port number: {trimmed}")))
                .and_then(|port| {
                    if port == 0 {
                        Err(AppError::InvalidPort("port 0 is not valid".to_string()))
                    } else {
                        Ok(port)
                    }
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    if ports.is_empty() {
        return Err(AppError::InvalidPort("no ports specified".to_string()));
    }

    if ports.len() > max_ports {
        return Err(AppError::TooManyPorts {
            requested: ports.len(),
            max: max_ports,
        });
    }

    Ok(ports)
}

/// Validate a hostname per RFC 952 / RFC 1123 (SDD §4.3).
fn validate_hostname(hostname: &str) -> Result<(), AppError> {
    // Strip trailing dot
    let hostname = hostname.strip_suffix('.').unwrap_or(hostname);

    if hostname.is_empty() {
        return Err(AppError::InvalidHostname("empty hostname".to_string()));
    }

    if hostname.len() > 253 {
        return Err(AppError::InvalidHostname(format!(
            "hostname exceeds 253 characters (got {})",
            hostname.len()
        )));
    }

    // Reject single-label hostnames (no dot) — these resolve to intranet/internal names
    if !hostname.contains('.') {
        return Err(AppError::InvalidHostname(
            "single-label hostname not allowed".to_string(),
        ));
    }

    // Reject wildcards
    if hostname.contains('*') {
        return Err(AppError::InvalidHostname(
            "wildcards are not allowed".to_string(),
        ));
    }

    // Reject underscores
    if hostname.contains('_') {
        return Err(AppError::InvalidHostname(
            "underscores are not allowed in hostnames".to_string(),
        ));
    }

    for label in hostname.split('.') {
        validate_label(label)?;
    }

    Ok(())
}

fn validate_label(label: &str) -> Result<(), AppError> {
    if label.is_empty() {
        return Err(AppError::InvalidHostname("empty label".to_string()));
    }

    if label.len() > 63 {
        return Err(AppError::InvalidHostname(format!(
            "label exceeds 63 characters: {label}"
        )));
    }

    // Must start and end with alphanumeric
    let first = label.chars().next().unwrap();
    let last = label.chars().next_back().unwrap();

    if !first.is_ascii_alphanumeric() {
        return Err(AppError::InvalidHostname(format!(
            "label must start with alphanumeric: {label}"
        )));
    }

    if !last.is_ascii_alphanumeric() {
        return Err(AppError::InvalidHostname(format!(
            "label must end with alphanumeric: {label}"
        )));
    }

    // Interior characters: alphanumeric or hyphen
    for ch in label.chars() {
        if !ch.is_ascii_alphanumeric() && ch != '-' {
            return Err(AppError::InvalidHostname(format!(
                "invalid character '{ch}' in label: {label}"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_PORTS: usize = 5;

    // --- Basic hostname parsing ---

    #[test]
    fn bare_hostname() {
        let result = parse_input("example.com", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
        assert_eq!(result.ports, vec![443]);
    }

    #[test]
    fn hostname_with_port() {
        let result = parse_input("example.com:8443", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
        assert_eq!(result.ports, vec![8443]);
    }

    #[test]
    fn hostname_with_multiple_ports() {
        let result = parse_input("example.com:443,465,993", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
        assert_eq!(result.ports, vec![443, 465, 993]);
    }

    #[test]
    fn trailing_dot_stripped() {
        let result = parse_input("example.com.", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
    }

    #[test]
    fn trailing_dot_with_port() {
        let result = parse_input("example.com.:443", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
    }

    // --- IPv4 parsing ---

    #[test]
    fn ipv4_bare() {
        let result = parse_input("93.184.216.34", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Ip("93.184.216.34".parse().unwrap()));
        assert_eq!(result.ports, vec![443]);
    }

    #[test]
    fn ipv4_with_port() {
        let result = parse_input("93.184.216.34:8443", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Ip("93.184.216.34".parse().unwrap()));
        assert_eq!(result.ports, vec![8443]);
    }

    // --- IPv6 parsing ---

    #[test]
    fn ipv6_bare_brackets() {
        let result = parse_input("[::1]", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Ip("::1".parse().unwrap()));
        assert_eq!(result.ports, vec![443]);
    }

    #[test]
    fn ipv6_with_port() {
        let result = parse_input("[2001:db8::1]:8443", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Ip("2001:db8::1".parse().unwrap()));
        assert_eq!(result.ports, vec![8443]);
    }

    #[test]
    fn ipv6_unclosed_bracket() {
        assert!(parse_input("[::1", MAX_PORTS).is_err());
    }

    // --- Punycode ---

    #[test]
    fn punycode_hostname_accepted() {
        let result = parse_input("xn--nxasmq6b.xn--jxalpdlp", MAX_PORTS).unwrap();
        assert_eq!(
            result.target,
            Target::Hostname("xn--nxasmq6b.xn--jxalpdlp".to_string())
        );
    }

    // --- Percent-encoding ---

    #[test]
    fn percent_encoded_colon() {
        let result = parse_input("example.com%3A8443", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
        assert_eq!(result.ports, vec![8443]);
    }

    #[test]
    fn percent_encoded_comma() {
        let result = parse_input("example.com:443%2C465", MAX_PORTS).unwrap();
        assert_eq!(result.ports, vec![443, 465]);
    }

    // --- Rejections ---

    #[test]
    fn empty_input() {
        assert!(parse_input("", MAX_PORTS).is_err());
    }

    #[test]
    fn whitespace_only() {
        assert!(parse_input("   ", MAX_PORTS).is_err());
    }

    #[test]
    fn wildcard_rejected() {
        let err = parse_input("*.example.com", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn underscore_rejected() {
        let err = parse_input("_dmarc.example.com", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn port_zero_rejected() {
        let err = parse_input("example.com:0", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidPort(_)));
    }

    #[test]
    fn port_too_large_rejected() {
        let err = parse_input("example.com:99999", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidPort(_)));
    }

    #[test]
    fn too_many_ports_rejected() {
        let err = parse_input("example.com:1,2,3,4,5,6", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::TooManyPorts { .. }));
    }

    #[test]
    fn hostname_too_long() {
        let long_label = "a".repeat(63);
        // 4 labels of 63 chars + dots = 255 chars > 253
        let hostname = format!("{long_label}.{long_label}.{long_label}.{long_label}");
        let err = parse_input(&hostname, MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn label_too_long() {
        let label = "a".repeat(64);
        let hostname = format!("{label}.example.com");
        let err = parse_input(&hostname, MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn label_starts_with_hyphen() {
        let err = parse_input("-example.com", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn label_ends_with_hyphen() {
        let err = parse_input("example-.com", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn empty_port_after_colon() {
        assert!(parse_input("example.com:", MAX_PORTS).is_err());
    }

    #[test]
    fn ipv6_without_brackets() {
        assert!(parse_input("2001:db8::1", MAX_PORTS).is_err());
    }

    // --- Target helper ---

    #[test]
    fn hostname_target_returns_hostname() {
        let t = Target::Hostname("example.com".to_string());
        assert_eq!(t.hostname(), Some("example.com"));
    }

    #[test]
    fn ip_target_returns_none() {
        let t = Target::Ip("1.2.3.4".parse().unwrap());
        assert_eq!(t.hostname(), None);
    }

    // --- Edge cases ---

    #[test]
    fn single_label_hostname_rejected() {
        let err = parse_input("localhost", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn numeric_hostname_not_ip_rejected() {
        // "12345" is not a valid IP, but also single-label — rejected
        let err = parse_input("12345", MAX_PORTS).unwrap_err();
        assert!(matches!(err, AppError::InvalidHostname(_)));
    }

    #[test]
    fn port_with_spaces_trimmed() {
        let result = parse_input("  example.com:443  ", MAX_PORTS).unwrap();
        assert_eq!(result.target, Target::Hostname("example.com".to_string()));
        assert_eq!(result.ports, vec![443]);
    }
}
