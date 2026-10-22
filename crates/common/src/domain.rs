//! The domain-name input rule shared by lens and the evidence pages.

use std::net::IpAddr;

use idna::uts46::{AsciiDenyList, DnsLength, Hyphens, Uts46};

/// Validate and normalize a domain name input string.
///
/// - Trims surrounding whitespace.
/// - Rejects empty strings.
/// - Maps the name with UTS #46 (IDNA, the URL Standard's domain-to-ASCII): lowercases, maps
///   full-width forms and the ideographic full stop, punycodes Unicode labels; rejects what a URL
///   host cannot hold (whitespace, `/`, `%`, `@`, ...) and malformed `xn--` labels. The checks
///   below run on the mapped form, so `１.１.１.１` is the address `1.1.1.1`.
/// - Rejects IP addresses (both v4 and v6), with one trailing dot or in square brackets too.
/// - Rejects a name whose last label is a number (WHATWG URL "ends in a number": all digits, or
///   `0x` and hex digits): URL parsers and `inet_aton` read `1.2.3` or `3221225985` as IPv4.
/// - Rejects wildcards (labels starting with `*`).
/// - Rejects labels longer than 63 characters.
/// - Rejects total length exceeding 253 characters.
/// - Returns the mapped ASCII (A-label) form without a trailing dot on success: one domain,
///   whether typed as Unicode, full-width or `xn--`, has one form.
///
/// Does NOT perform DNS resolution.
pub fn validate(input: &str) -> Result<String, String> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return Err("domain must not be empty".to_string());
    }

    // The raw form first: UTS #46's URL deny list refuses `[`, so a bracketed address must be
    // named as an address before the mapping sees it.
    refuse_address(trimmed, trimmed)?;
    let mapped = Uts46::new()
        .to_ascii(
            trimmed.as_bytes(),
            AsciiDenyList::URL,
            Hyphens::Allow,
            DnsLength::Ignore,
        )
        .map_err(|_| format!("not a valid domain name: {trimmed}"))?;
    // Again on what the name means: `１.１.１.１` maps to `1.1.1.1`.
    refuse_address(&mapped, trimmed)?;

    if mapped.starts_with('*') {
        return Err("wildcard domains are not accepted".to_string());
    }

    let domain = mapped.strip_suffix('.').unwrap_or(&mapped);

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

/// Refuses `name` when it reads as an IP address: bare, with one trailing dot, in square brackets,
/// or with a numeric last label. Messages name `shown`, the trimmed input.
fn refuse_address(name: &str, shown: &str) -> Result<(), String> {
    let undotted = name.strip_suffix('.').unwrap_or(name);
    let unbracketed = undotted
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(undotted);
    if [name, undotted, unbracketed]
        .iter()
        .any(|s| s.parse::<IpAddr>().is_ok())
    {
        return Err(format!(
            "IP addresses are not accepted as domain input: {shown}"
        ));
    }
    if undotted.rsplit('.').next().is_some_and(is_number) {
        return Err(format!(
            "the last label is a number, so the name reads as an IP address: {shown}"
        ));
    }
    Ok(())
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
