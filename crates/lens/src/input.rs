use std::net::IpAddr;

use crate::error::AppError;

/// Validate and normalize a domain name input string.
///
/// - Trims surrounding whitespace.
/// - Rejects empty strings.
/// - Rejects bare IP addresses (both v4 and v6).
/// - Rejects wildcards (labels starting with `*`).
/// - Rejects labels longer than 63 characters.
/// - Rejects total length exceeding 253 characters.
/// - Returns the lowercased, trimmed domain on success.
///
/// Does NOT perform DNS resolution. Target IP policy checks happen after
/// resolution in the check orchestration layer.
pub fn validate_domain(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err(AppError::DomainInvalid("domain must not be empty".to_string()));
    }

    // Reject bare IP addresses.
    if trimmed.parse::<IpAddr>().is_ok() {
        return Err(AppError::DomainInvalid(format!(
            "IP addresses are not accepted as domain input: {trimmed}"
        )));
    }

    let lower = trimmed.to_lowercase();

    // Reject wildcards.
    if lower.starts_with('*') {
        return Err(AppError::DomainInvalid(
            "wildcard domains are not accepted".to_string(),
        ));
    }

    // Strip trailing dot before length checks (FQDN notation is fine, normalize it away).
    let domain = lower.strip_suffix('.').unwrap_or(&lower);

    if domain.len() > 253 {
        return Err(AppError::DomainInvalid(format!(
            "domain exceeds 253 characters (got {})",
            domain.len()
        )));
    }

    // Validate each label.
    for label in domain.split('.') {
        if label.len() > 63 {
            return Err(AppError::DomainInvalid(format!(
                "label exceeds 63 characters: {label}"
            )));
        }
        if label.is_empty() {
            return Err(AppError::DomainInvalid(
                "domain contains an empty label".to_string(),
            ));
        }
    }

    Ok(domain.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Valid domains ---

    #[test]
    fn simple_domain_accepted() {
        assert_eq!(validate_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn subdomain_accepted() {
        assert_eq!(
            validate_domain("sub.example.com").unwrap(),
            "sub.example.com"
        );
    }

    #[test]
    fn deep_subdomain_accepted() {
        assert_eq!(
            validate_domain("a.b.c.example.com").unwrap(),
            "a.b.c.example.com"
        );
    }

    #[test]
    fn uppercased_input_is_lowercased() {
        assert_eq!(validate_domain("EXAMPLE.COM").unwrap(), "example.com");
    }

    #[test]
    fn mixed_case_is_lowercased() {
        assert_eq!(validate_domain("Sub.Example.COM").unwrap(), "sub.example.com");
    }

    #[test]
    fn whitespace_is_trimmed() {
        assert_eq!(validate_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn trailing_dot_is_stripped() {
        assert_eq!(validate_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn punycode_accepted() {
        assert_eq!(
            validate_domain("xn--nxasmq6b.xn--jxalpdlp").unwrap(),
            "xn--nxasmq6b.xn--jxalpdlp"
        );
    }

    #[test]
    fn single_label_accepted() {
        // lens does not reject single-label domains at input validation time;
        // that is a DNS-level concern, not a syntax concern.
        assert_eq!(validate_domain("localhost").unwrap(), "localhost");
    }

    // --- IP addresses rejected ---

    #[test]
    fn ipv4_rejected() {
        let err = validate_domain("1.2.3.4").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn ipv4_loopback_rejected() {
        let err = validate_domain("127.0.0.1").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn ipv6_rejected() {
        let err = validate_domain("2001:db8::1").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn ipv6_loopback_rejected() {
        let err = validate_domain("::1").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    // --- Wildcards rejected ---

    #[test]
    fn wildcard_rejected() {
        let err = validate_domain("*.example.com").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn bare_wildcard_rejected() {
        let err = validate_domain("*").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    // --- Length limits ---

    #[test]
    fn domain_too_long_rejected() {
        // 4 labels of 63 chars + dots = 255 chars > 253
        let label = "a".repeat(63);
        let domain = format!("{label}.{label}.{label}.{label}");
        let err = validate_domain(&domain).unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn domain_exactly_253_chars_accepted() {
        // 3 labels of 63 chars (63+1+63+1+63 = 191) + a 62-char label = 254? Let's build exactly 253.
        // 63 + '.' + 63 + '.' + 63 + '.' + 61 = 253
        let l63 = "a".repeat(63);
        let l61 = "b".repeat(61);
        let domain = format!("{l63}.{l63}.{l63}.{l61}");
        assert_eq!(domain.len(), 253);
        assert!(validate_domain(&domain).is_ok());
    }

    #[test]
    fn label_too_long_rejected() {
        let long_label = "a".repeat(64);
        let domain = format!("{long_label}.example.com");
        let err = validate_domain(&domain).unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn label_exactly_63_chars_accepted() {
        let label = "a".repeat(63);
        let domain = format!("{label}.com");
        assert!(validate_domain(&domain).is_ok());
    }

    // --- Empty input ---

    #[test]
    fn empty_string_rejected() {
        let err = validate_domain("").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    #[test]
    fn whitespace_only_rejected() {
        let err = validate_domain("   ").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }

    // --- Empty labels ---

    #[test]
    fn double_dot_rejected() {
        let err = validate_domain("example..com").unwrap_err();
        assert!(matches!(err, AppError::DomainInvalid(_)));
    }
}
