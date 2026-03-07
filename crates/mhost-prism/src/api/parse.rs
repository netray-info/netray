//! Parse endpoint: tokenize partial input and return context-aware completions.
//!
//! `POST /api/parse` accepts `{"input": "...", "cursor_pos": N}` and returns
//! classified tokens with character ranges plus completions relevant to the
//! cursor position. This powers server-side autocomplete — the frontend can
//! show static completions immediately and replace them when this endpoint
//! responds.

use std::str::FromStr;

use axum::Json;
use mhost::RecordType;
use mhost::nameserver::predefined::PredefinedProvider;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ParseRequest {
    /// The query input string to tokenize.
    input: String,
    /// Cursor position within the input (defaults to end of input).
    #[serde(default)]
    cursor_pos: Option<usize>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ParseResponse {
    tokens: Vec<TokenInfo>,
    completions: Vec<Completion>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TokenInfo {
    /// Token classification: `domain`, `record_type`, `server`, `server_partial`, `flag`, `flag_partial`, `unknown`.
    kind: &'static str,
    value: String,
    /// Start byte offset of the token in the input string.
    from: usize,
    /// End byte offset of the token in the input string.
    to: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Completion {
    label: String,
    detail: String,
    /// Completion category: `record_type`, `server`, `flag`.
    category: &'static str,
}

// ---------------------------------------------------------------------------
// Completion data
// ---------------------------------------------------------------------------

const RECORD_TYPE_INFO: &[(&str, &str)] = &[
    ("A", "IPv4 address"),
    ("AAAA", "IPv6 address"),
    ("MX", "Mail exchange"),
    ("TXT", "Text record"),
    ("NS", "Name server"),
    ("SOA", "Start of authority"),
    ("CNAME", "Canonical name"),
    ("CAA", "Certification authority"),
    ("SRV", "Service locator"),
    ("PTR", "Pointer (reverse DNS)"),
    ("HTTPS", "HTTPS service binding"),
    ("SVCB", "Service binding"),
    ("SSHFP", "SSH fingerprint"),
    ("TLSA", "TLS association (DANE)"),
    ("NAPTR", "Naming authority pointer"),
    ("HINFO", "Host information"),
    ("OPENPGPKEY", "OpenPGP public key"),
    ("DNSKEY", "DNSSEC key"),
    ("DS", "Delegation signer"),
];

const SERVER_INFO: &[(&str, &str)] = &[
    ("@cloudflare", "1.1.1.1 / 1.0.0.1"),
    ("@google", "8.8.8.8 / 8.8.4.4"),
    ("@quad9", "9.9.9.9"),
    ("@mullvad", "Mullvad DNS"),
    ("@wikimedia", "Wikimedia DNS"),
    ("@dns4eu", "DNS4EU"),
    ("@system", "System resolvers"),
];

const FLAG_INFO: &[(&str, &str)] = &[
    ("+udp", "UDP transport (default)"),
    ("+tcp", "TCP transport"),
    ("+tls", "DNS-over-TLS"),
    ("+https", "DNS-over-HTTPS"),
    ("+dnssec", "DNSSEC validation"),
];

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Tokenize a partial query string and return context-aware completions.
///
/// Powers the editor's autocomplete. Classifies each token (domain, record type,
/// server, flag) and returns completions relevant to the cursor position.
#[utoipa::path(
    post, path = "/api/parse",
    tag = "Query",
    request_body = ParseRequest,
    responses(
        (status = 200, description = "Tokenized input with completions", body = ParseResponse),
    )
)]
pub async fn parse_handler(Json(body): Json<ParseRequest>) -> Json<ParseResponse> {
    let input = &body.input;
    let cursor_pos = body.cursor_pos.unwrap_or(input.len()).min(input.len());

    let tokens = tokenize(input);
    let completions = completions_at(input, cursor_pos, &tokens);

    Json(ParseResponse {
        tokens,
        completions,
    })
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

fn tokenize(input: &str) -> Vec<TokenInfo> {
    let mut tokens = Vec::new();
    let mut is_first = true;
    let bytes = input.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }

        let from = pos;
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let value = &input[from..pos];

        let kind = if is_first {
            is_first = false;
            "domain"
        } else if value.starts_with('@') {
            classify_server(value)
        } else if value.starts_with('+') {
            classify_flag(value)
        } else {
            classify_record_token(value)
        };

        tokens.push(TokenInfo {
            kind,
            value: value.to_owned(),
            from,
            to: pos,
        });
    }

    tokens
}

fn classify_server(value: &str) -> &'static str {
    let name = &value[1..];
    if name.is_empty() {
        return "server_partial";
    }
    if name.eq_ignore_ascii_case("system") || PredefinedProvider::from_str(name).is_ok() {
        return "server";
    }
    if name.parse::<std::net::IpAddr>().is_ok() {
        return "server";
    }
    // Check ip:port patterns.
    if let Some((addr_str, port_str)) = name.rsplit_once(':') {
        let addr_str = addr_str
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(addr_str);
        if addr_str.parse::<std::net::IpAddr>().is_ok() && port_str.parse::<u16>().is_ok() {
            return "server";
        }
    }
    "server_partial"
}

fn classify_flag(value: &str) -> &'static str {
    match value[1..].to_ascii_lowercase().as_str() {
        "udp" | "tcp" | "tls" | "https" | "dnssec" | "check" | "trace" => "flag",
        _ => "flag_partial",
    }
}

fn classify_record_token(value: &str) -> &'static str {
    let upper = value.to_ascii_uppercase();
    if upper == "ALL" || matches!(upper.as_str(), "ANY" | "AXFR" | "IXFR") {
        return "record_type";
    }
    if RecordType::from_str(&upper).is_ok() {
        return "record_type";
    }
    "unknown"
}

// ---------------------------------------------------------------------------
// Completions
// ---------------------------------------------------------------------------

fn completions_at(input: &str, cursor_pos: usize, tokens: &[TokenInfo]) -> Vec<Completion> {
    if tokens.is_empty() {
        return Vec::new();
    }

    // Find the token under the cursor.
    let token_at_cursor = tokens
        .iter()
        .find(|t| t.from <= cursor_pos && cursor_pos <= t.to);

    // If cursor is on the domain token, no completions.
    if let Some(t) = token_at_cursor
        && t.kind == "domain"
    {
        return Vec::new();
    }

    let prefix = match token_at_cursor {
        Some(t) => &input[t.from..cursor_pos],
        None => "",
    };

    let mut completions = Vec::new();

    if let Some(server_prefix) = prefix.strip_prefix('@') {
        let filter = server_prefix.to_ascii_lowercase();
        for &(label, detail) in SERVER_INFO {
            if label[1..].to_ascii_lowercase().starts_with(&filter) {
                completions.push(Completion {
                    label: label.to_owned(),
                    detail: detail.to_owned(),
                    category: "server",
                });
            }
        }
    } else if prefix.starts_with('+') {
        let filter = prefix.to_ascii_lowercase();
        for &(label, detail) in FLAG_INFO {
            if label.starts_with(&filter) {
                completions.push(Completion {
                    label: label.to_owned(),
                    detail: detail.to_owned(),
                    category: "flag",
                });
            }
        }
    } else {
        let filter = prefix.to_ascii_uppercase();
        for &(name, detail) in RECORD_TYPE_INFO {
            if name.starts_with(&filter) {
                completions.push(Completion {
                    label: name.to_owned(),
                    detail: detail.to_owned(),
                    category: "record_type",
                });
            }
        }
        // When cursor is at a space (no partial token), offer all categories.
        if prefix.is_empty() {
            for &(label, detail) in SERVER_INFO {
                completions.push(Completion {
                    label: label.to_owned(),
                    detail: detail.to_owned(),
                    category: "server",
                });
            }
            for &(label, detail) in FLAG_INFO {
                completions.push(Completion {
                    label: label.to_owned(),
                    detail: detail.to_owned(),
                    category: "flag",
                });
            }
        }
    }

    completions
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_no_tokens() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn domain_only() {
        let tokens = tokenize("example.com");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, "domain");
        assert_eq!(tokens[0].value, "example.com");
        assert_eq!(tokens[0].from, 0);
        assert_eq!(tokens[0].to, 11);
    }

    #[test]
    fn domain_with_record_type() {
        let tokens = tokenize("example.com MX");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, "domain");
        assert_eq!(tokens[1].kind, "record_type");
        assert_eq!(tokens[1].value, "MX");
        assert_eq!(tokens[1].from, 12);
        assert_eq!(tokens[1].to, 14);
    }

    #[test]
    fn full_query_tokens() {
        let tokens = tokenize("example.com MX @cloudflare +tls");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0].kind, "domain");
        assert_eq!(tokens[1].kind, "record_type");
        assert_eq!(tokens[2].kind, "server");
        assert_eq!(tokens[3].kind, "flag");
    }

    #[test]
    fn partial_server() {
        let tokens = tokenize("example.com @cl");
        assert_eq!(tokens[1].kind, "server_partial");
    }

    #[test]
    fn complete_server() {
        let tokens = tokenize("example.com @cloudflare");
        assert_eq!(tokens[1].kind, "server");
    }

    #[test]
    fn ip_server() {
        let tokens = tokenize("example.com @198.51.100.1");
        assert_eq!(tokens[1].kind, "server");
    }

    #[test]
    fn ip_server_with_port() {
        let tokens = tokenize("example.com @198.51.100.1:5353");
        assert_eq!(tokens[1].kind, "server");
    }

    #[test]
    fn system_server() {
        let tokens = tokenize("example.com @system");
        assert_eq!(tokens[1].kind, "server");
    }

    #[test]
    fn partial_flag() {
        let tokens = tokenize("example.com +t");
        assert_eq!(tokens[1].kind, "flag_partial");
    }

    #[test]
    fn complete_flag() {
        let tokens = tokenize("example.com +tls");
        assert_eq!(tokens[1].kind, "flag");
    }

    #[test]
    fn unknown_token() {
        let tokens = tokenize("example.com FOOBAR");
        assert_eq!(tokens[1].kind, "unknown");
    }

    #[test]
    fn token_positions_with_extra_spaces() {
        let tokens = tokenize("example.com   MX");
        assert_eq!(tokens[0].from, 0);
        assert_eq!(tokens[0].to, 11);
        assert_eq!(tokens[1].from, 14);
        assert_eq!(tokens[1].to, 16);
    }

    #[test]
    fn completions_for_partial_server() {
        let input = "example.com @cl";
        let tokens = tokenize(input);
        let completions = completions_at(input, 15, &tokens);
        assert!(!completions.is_empty());
        assert!(completions.iter().any(|c| c.label == "@cloudflare"));
        assert!(completions.iter().all(|c| c.category == "server"));
    }

    #[test]
    fn completions_for_partial_flag() {
        let input = "example.com +t";
        let tokens = tokenize(input);
        let completions = completions_at(input, 14, &tokens);
        assert!(completions.iter().any(|c| c.label == "+tcp"));
        assert!(completions.iter().any(|c| c.label == "+tls"));
        assert!(completions.iter().all(|c| c.category == "flag"));
    }

    #[test]
    fn completions_for_partial_record_type() {
        let input = "example.com M";
        let tokens = tokenize(input);
        let completions = completions_at(input, 13, &tokens);
        assert!(completions.iter().any(|c| c.label == "MX"));
        assert!(completions.iter().all(|c| c.category == "record_type"));
    }

    #[test]
    fn no_completions_on_domain() {
        let input = "exam";
        let tokens = tokenize(input);
        let completions = completions_at(input, 4, &tokens);
        assert!(completions.is_empty());
    }

    #[test]
    fn completions_after_space() {
        let input = "example.com ";
        let tokens = tokenize(input);
        let completions = completions_at(input, 12, &tokens);
        assert!(completions.iter().any(|c| c.category == "record_type"));
        assert!(completions.iter().any(|c| c.category == "server"));
        assert!(completions.iter().any(|c| c.category == "flag"));
    }

    #[test]
    fn at_sign_only_shows_all_servers() {
        let input = "example.com @";
        let tokens = tokenize(input);
        let completions = completions_at(input, 13, &tokens);
        assert_eq!(completions.len(), SERVER_INFO.len());
        assert!(completions.iter().all(|c| c.category == "server"));
    }

    #[test]
    fn plus_sign_only_shows_all_flags() {
        let input = "example.com +";
        let tokens = tokenize(input);
        let completions = completions_at(input, 13, &tokens);
        assert_eq!(completions.len(), FLAG_INFO.len());
        assert!(completions.iter().all(|c| c.category == "flag"));
    }
}
