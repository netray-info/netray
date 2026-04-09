use crate::error::MailError;

/// Validate a DKIM selector: ASCII alphanumeric + hyphen only, 1-63 chars, no dots.
pub fn validate_dkim_selector(s: &str) -> Result<(), MailError> {
    if s.is_empty() {
        return Err(MailError::InvalidSelector {
            reason: "selector must not be empty".to_string(),
        });
    }
    if s.len() > 63 {
        return Err(MailError::InvalidSelector {
            reason: "selector exceeds 63 characters".to_string(),
        });
    }
    if s.contains('.') {
        return Err(MailError::InvalidSelector {
            reason: "selector must not contain dots".to_string(),
        });
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(MailError::InvalidSelector {
            reason: "selector contains invalid characters (only ASCII alphanumeric and hyphens allowed)".to_string(),
        });
    }
    Ok(())
}

/// Parse and validate a domain name input.
///
/// Trims whitespace, lowercases, strips trailing dot, validates labels.
pub fn parse_domain(s: &str) -> Result<String, MailError> {
    let s = s.trim().to_lowercase();
    let s = s.strip_suffix('.').unwrap_or(&s);

    if s.is_empty() {
        return Err(MailError::InvalidDomain("empty domain".to_string()));
    }

    if s.len() > 253 {
        return Err(MailError::InvalidDomain(
            "domain exceeds 253 characters".to_string(),
        ));
    }

    let labels: Vec<&str> = s.split('.').collect();
    if labels.len() < 2 {
        return Err(MailError::InvalidDomain(
            "domain must have at least two labels".to_string(),
        ));
    }

    for label in &labels {
        validate_label(label)?;
    }

    Ok(s.to_string())
}

fn validate_label(label: &str) -> Result<(), MailError> {
    if label.is_empty() {
        return Err(MailError::InvalidDomain("empty label".to_string()));
    }
    if label.len() > 63 {
        return Err(MailError::InvalidDomain(
            "label exceeds 63 characters".to_string(),
        ));
    }
    if label.starts_with('-') {
        return Err(MailError::InvalidDomain(
            "label starts with a hyphen".to_string(),
        ));
    }
    if label.ends_with('-') {
        return Err(MailError::InvalidDomain(
            "label ends with a hyphen".to_string(),
        ));
    }
    if !label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(MailError::InvalidDomain(
            "label contains invalid characters".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_domain() {
        assert_eq!(parse_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn strips_trailing_dot() {
        assert_eq!(parse_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn lowercases() {
        assert_eq!(parse_domain("Example.COM").unwrap(), "example.com");
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(parse_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_domain("").is_err());
    }

    #[test]
    fn rejects_single_label() {
        assert!(parse_domain("localhost").is_err());
    }

    #[test]
    fn rejects_label_too_long() {
        let long_label = "a".repeat(64);
        let domain = format!("{}.example.com", long_label);
        let err = parse_domain(&domain).unwrap_err();
        assert!(err.to_string().contains("63 characters"));
    }

    #[test]
    fn rejects_leading_hyphen() {
        assert!(parse_domain("-example.com").is_err());
    }

    #[test]
    fn rejects_trailing_hyphen() {
        assert!(parse_domain("example-.com").is_err());
    }

    #[test]
    fn rejects_total_too_long() {
        let label = "a".repeat(63);
        let domain = format!("{}.{}.{}.{}.com", label, label, label, label);
        assert!(parse_domain(&domain).is_err());
    }
}
