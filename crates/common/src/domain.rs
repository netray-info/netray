//! The domain-name input rule shared by lens and the evidence pages.

use std::net::IpAddr;

/// Validate and normalize a domain name input string.
///
/// - Trims surrounding whitespace.
/// - Rejects empty strings.
/// - Rejects IP addresses (both v4 and v6), with one trailing dot or in square brackets too.
/// - Rejects a name whose last label is a number (WHATWG URL "ends in a number": all digits, or
///   `0x` and hex digits): URL parsers and `inet_aton` read `1.2.3` or `3221225985` as IPv4.
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

    let undotted = trimmed.strip_suffix('.').unwrap_or(trimmed);
    let unbracketed = undotted
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(undotted);
    if [trimmed, undotted, unbracketed]
        .iter()
        .any(|s| s.parse::<IpAddr>().is_ok())
    {
        return Err(format!(
            "IP addresses are not accepted as domain input: {trimmed}"
        ));
    }
    if undotted.rsplit('.').next().is_some_and(is_number) {
        return Err(format!(
            "the last label is a number, so the name reads as an IP address: {trimmed}"
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

/// A label URL parsers read as a number: all ASCII digits, or `0x`/`0X` and only hex digits.
fn is_number(label: &str) -> bool {
    if let Some(hex) = label
        .strip_prefix("0x")
        .or_else(|| label.strip_prefix("0X"))
    {
        return hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    !label.is_empty() && label.bytes().all(|b| b.is_ascii_digit())
}
