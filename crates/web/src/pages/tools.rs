//! `GET /tools`, the inspector index (spec v2-site-nav, requirement 7): one card per inspector
//! with a quick form, the featured guides and the API docs.

use askama::Template;
use axum::http::StatusCode;
use axum::response::Response;

use super::guides::{GUIDES, Guide};
use super::{Frame, Tool, page};
use crate::components::QueryInputView;

const CACHE: &str = "public, max-age=3600";

/// What an inspector takes, in the card's words.
fn takes(tool: Tool) -> &'static str {
    match tool {
        Tool::Dns => "a name and record types",
        Tool::Tls => "a host and ports",
        Tool::Http => "a URL",
        Tool::Email => "a domain and DKIM selectors",
        Tool::Ip => "an address, or none for yours",
    }
}

/// The two guides each inspector features.
fn featured(tool: Tool) -> [&'static str; 2] {
    match tool {
        Tool::Dns => ["record-types", "dnssec"],
        Tool::Tls => ["certificate-chain", "tls-protocol"],
        Tool::Http => ["security-headers", "csp"],
        Tool::Email => ["email-auth", "dkim"],
        Tool::Ip => ["ip-enrichment", "asn"],
    }
}

pub(crate) struct ToolCard {
    pub tool: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    pub what: &'static str,
    pub takes: &'static str,
    pub query: QueryInputView,
    pub guides: Vec<&'static Guide>,
}

#[derive(Template)]
#[template(path = "pages/tools.html")]
struct ToolsPage {
    frame: Frame,
    cards: Vec<ToolCard>,
}

/// `GET /tools`.
pub async fn show() -> Response {
    let cards = Tool::ALL
        .into_iter()
        .map(|tool| ToolCard {
            tool: tool.as_str(),
            name: tool.name(),
            tagline: tool.tagline(),
            what: tool.what(),
            takes: takes(tool),
            query: QueryInputView {
                action: format!("/inspect/{}", tool.as_str()),
                name: tool.field().to_string(),
                label: tool.label().to_string(),
                placeholder: tool.placeholder().to_string(),
                value: None,
                submit: "Inspect".to_string(),
            },
            guides: featured(tool)
                .iter()
                .filter_map(|slug| GUIDES.iter().find(|g| g.slug == *slug))
                .collect(),
        })
        .collect();
    let frame = Frame {
        nav: "tools",
        ..Frame::new(
            "Inspectors - netray",
            "The five netray inspectors: DNS, TLS, HTTP, Email and IP, each a live check of one protocol.",
            "/tools",
        )
    };
    page(StatusCode::OK, CACHE, &ToolsPage { frame, cards })
}
