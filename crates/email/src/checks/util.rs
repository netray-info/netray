use std::collections::HashMap;

/// Parse semicolon-delimited `key=value` tag strings.
/// Keys are lowercased; values are trimmed. Used by DMARC, TLS-RPT, BIMI, and MTA-STS DNS records.
pub fn parse_tags(input: &str) -> HashMap<String, String> {
    let mut tags = HashMap::new();
    for part in input.split(';') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            tags.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    tags
}
