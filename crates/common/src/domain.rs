//! The domain-name input rule shared by lens and the evidence pages.

use std::net::IpAddr;

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
/// Does NOT perform DNS resolution.
pub fn validate(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err("domain must not be empty".to_string());
    }

    if trimmed.parse::<IpAddr>().is_ok() {
        return Err(format!(
            "IP addresses are not accepted as domain input: {trimmed}"
        ));
    }

    let lower = trimmed.to_lowercase();

    if lower.starts_with('*') {
        return Err("wildcard domains are not accepted".to_string());
    }

    let domain = lower.strip_suffix('.').unwrap_or(&lower);

    if domain.len() > 253 {
        return Err(format!(
            "domain exceeds 253 characters (got {})",
            domain.len()
        ));
    }

    for label in domain.split('.') {
        if label.len() > 63 {
            return Err(format!("label exceeds 63 characters: {label}"));
        }
        if label.is_empty() {
            return Err("domain contains an empty label".to_string());
        }
    }

    Ok(domain.to_string())
}
