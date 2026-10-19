//! The reference answers of the DNS part: the query language parsed, the server catalogue and
//! the record types, callable without a route.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::api::meta::{self, RecordTypeInfo, ServerInfo};
use crate::api::parse::{Completion, TokenInfo, parse_input};
use crate::parser::{self, ServerSpec, Transport};

/// A query string to parse.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DnsParseRequest {
    /// The query input string to tokenize.
    pub input: String,
    /// Cursor byte offset within the input (defaults to end of input).
    #[serde(default)]
    pub cursor_pos: Option<usize>,
}

/// A query string as the parser reads it.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DnsParsedQuery {
    pub domain: String,
    pub record_types: Vec<String>,
    /// Servers as `google`, `system` or `ip:port`; empty means the configured defaults.
    pub servers: Vec<String>,
    /// `udp`, `tcp`, `tls` or `https`, when the query names one.
    pub transport: Option<String>,
    pub dnssec: bool,
    pub warnings: Vec<String>,
}

/// Tokens and completions of a query string, with the parsed query or the reason it does not
/// parse.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DnsParseAnswer {
    pub tokens: Vec<TokenInfo>,
    pub completions: Vec<Completion>,
    pub query: Option<DnsParsedQuery>,
    pub error: Option<String>,
}

/// A group alias and the providers it expands to.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DnsServerGroup {
    pub name: String,
    pub providers: Vec<String>,
}

/// The predefined providers and the group aliases over them.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DnsServerCatalogue {
    pub providers: Vec<ServerInfo>,
    pub groups: Vec<DnsServerGroup>,
}

/// The record types a query may ask for.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DnsRecordTypes {
    pub record_types: Vec<RecordTypeInfo>,
}

fn server_string(spec: &ServerSpec) -> String {
    match spec {
        ServerSpec::Predefined(p) => p.to_string(),
        ServerSpec::System => "system".to_owned(),
        ServerSpec::Ip { addr, port } => SocketAddr::new(*addr, *port).to_string(),
    }
}

fn transport_string(transport: Transport) -> &'static str {
    match transport {
        Transport::Udp => "udp",
        Transport::Tcp => "tcp",
        Transport::Tls => "tls",
        Transport::Https => "https",
    }
}

/// Parses the request's input; `@system` is offered in completions only when `allow_system`.
pub fn parse_answer(request: &DnsParseRequest, allow_system: bool) -> DnsParseAnswer {
    let (tokens, completions) = parse_input(&request.input, request.cursor_pos, allow_system);
    let (query, error) = match parser::parse(&request.input) {
        Ok(q) => (
            Some(DnsParsedQuery {
                domain: q.domain,
                record_types: q.record_types.iter().map(|rt| rt.to_string()).collect(),
                servers: q.servers.iter().map(server_string).collect(),
                transport: q.transport.map(|t| transport_string(t).to_owned()),
                dnssec: q.dnssec,
                warnings: q.warnings,
            }),
            None,
        ),
        Err(e) => (None, Some(e.to_string())),
    };
    DnsParseAnswer {
        tokens,
        completions,
        query,
        error,
    }
}

/// The predefined providers with their transports, and the group aliases.
pub fn server_catalogue() -> DnsServerCatalogue {
    DnsServerCatalogue {
        providers: meta::server_catalogue(),
        groups: parser::server_groups()
            .into_iter()
            .map(|(name, providers)| DnsServerGroup {
                name: name.to_owned(),
                providers,
            })
            .collect(),
    }
}

/// The record types a query may ask for.
pub fn record_types() -> DnsRecordTypes {
    DnsRecordTypes {
        record_types: meta::record_type_names()
            .into_iter()
            .map(|name| RecordTypeInfo { name })
            .collect(),
    }
}
