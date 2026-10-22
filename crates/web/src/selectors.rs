//! The DKIM selector list of a domain run (spec v2-dkim-legacy-urls, requirement 1): one rule for
//! the API query, the page query and the form field.

/// The most selectors one run takes: V1's effective limit (beacon's `max_user_selectors`, which
/// lens forwarded to) and the email module's default.
pub const MAX_DKIM_SELECTORS: usize = 5;

/// Parses a comma-separated selector list. Whitespace around each token is trimmed; an absent,
/// empty or whitespace-only value is no selectors. Otherwise 1 to `MAX_DKIM_SELECTORS` tokens,
/// each a valid DKIM selector, and no empty token.
pub fn parse_dkim_selectors(raw: Option<&str>) -> Result<Vec<String>, String> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else {
        return Ok(Vec::new());
    };
    let tokens: Vec<String> = raw.split(',').map(|t| t.trim().to_string()).collect();
    if tokens.iter().any(String::is_empty) {
        return Err("dkim_selectors contains an empty token".to_string());
    }
    if tokens.len() > MAX_DKIM_SELECTORS {
        return Err(format!(
            "dkim_selectors: at most {MAX_DKIM_SELECTORS} selectors allowed, got {}",
            tokens.len()
        ));
    }
    for token in &tokens {
        netray_email::input::validate_dkim_selector(token)
            .map_err(|e| format!("dkim_selectors: {e}"))?;
    }
    Ok(tokens)
}
