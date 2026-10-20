//! The evidence views of a stored entry (spec v2-inspectors, requirement 6): the latest entry of
//! a domain at `/{domain}/<protocol>` and one entry at `/r/{id}/<protocol>`. Read-only.

use std::sync::Arc;

use askama::Template;
use axum::Router;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use netray_model::Protocol;
use netray_model::evidence::Block;
use netray_store::{Entry, HistoryStore};
use percent_encoding::utf8_percent_encode;

use super::errors::LOCATION_SET;
use super::headers::secure;
use super::{Frame, PageHelp, export_slot, not_found, page, server_error};
use crate::entries::V1Lookup;
use crate::entries::render::gone;
use crate::entries::well_formed;
use crate::evidence::{entry_section, render_section};
use crate::explore::{ExploreLink, view_links};
use crate::export::{self, ExportInput};

#[derive(Clone)]
pub struct EvidenceDeps {
    pub history: Arc<HistoryStore>,
    pub v1: Option<V1Lookup>,
}

#[derive(Template)]
#[template(path = "pages/evidence.html")]
struct EvidencePage {
    frame: Frame,
    domain: String,
    home_href: String,
    protocol: &'static str,
    section: Option<String>,
    links: Vec<ExploreLink>,
    help: PageHelp,
    export: Option<String>,
    selector_form: Option<String>,
}

const PROTOCOLS: [Protocol; 5] = [
    Protocol::Dns,
    Protocol::Tls,
    Protocol::Http,
    Protocol::Email,
    Protocol::Ip,
];

/// Serves `/{domain}/{dns,tls,http,email,ip}` and `/r/{id}/{segment}`.
pub fn evidence_router(deps: EvidenceDeps) -> Router {
    let mut router = Router::<EvidenceDeps>::new();
    for protocol in PROTOCOLS {
        router = router.route(
            &format!("/{{domain}}/{}", protocol.as_str()),
            get(
                move |state: State<EvidenceDeps>, domain: Result<Path<String>, PathRejection>| {
                    latest(state, domain, protocol)
                },
            ),
        );
    }
    router
        .route("/r/{id}/{segment}", get(entry))
        .with_state(deps)
}

fn view(entry: &Entry, protocol: Protocol, cache_control: &'static str) -> Response {
    let section = entry_section(&entry.id, entry.created_at, &entry.run, protocol);
    let Ok(section) = render_section(&section) else {
        return server_error();
    };
    let title = format!("{} · {}", entry.domain, protocol.as_str());
    let export = view_export(entry, protocol, &title);
    let home_href = format!("/{}", utf8_percent_encode(&entry.domain, LOCATION_SET));
    let selector_form = if protocol == Protocol::Email {
        let Some(form) = super::result::selector_form(&home_href, None) else {
            return server_error();
        };
        Some(form)
    } else {
        None
    };
    page(
        StatusCode::OK,
        cache_control,
        &EvidencePage {
            frame: Frame::new(
                &format!("{title} - netray"),
                "The evidence behind one check run.",
                &format!("/r/{}/{}", entry.id, protocol.as_str()),
            ),
            domain: entry.domain.clone(),
            // The re-check form posts to the domain's run route (requirement 25: `r` re-checks).
            home_href,
            protocol: protocol.as_str(),
            section: Some(section),
            links: view_links(protocol, &entry.domain, &entry.run.blocks),
            help: PageHelp::EVIDENCE,
            export,
            selector_form,
        },
    )
}

/// The export slot of one protocol of a stored entry: its checks, its blocks and the entry.
fn view_export(entry: &Entry, protocol: Protocol, target: &str) -> Option<String> {
    let checks = export::checks_of(&entry.run);
    let blocks: Vec<Block> = entry
        .run
        .blocks
        .iter()
        .filter(|b| b.id.module() == protocol.as_str())
        .cloned()
        .collect();
    let errors = export::section_errors(&entry.run, &[protocol]);
    let input = ExportInput {
        target,
        title: &[],
        band: Some((&[protocol], &checks)),
        blocks: &blocks,
        errors: &errors,
    };
    export_slot(
        format!(
            "netray-{}-{}-{}",
            export::slug(&entry.domain),
            protocol.as_str(),
            entry.id
        ),
        Some((
            export::markdown(&input),
            export::csv(&input),
            crate::api::entry_json(entry),
        )),
    )
}

async fn latest(
    State(deps): State<EvidenceDeps>,
    raw: Result<Path<String>, PathRejection>,
    protocol: Protocol,
) -> Response {
    let Ok(Path(raw)) = raw else {
        return not_found().await;
    };
    let Ok(domain) = netray_common::domain::validate(&raw) else {
        return not_found().await;
    };
    match deps.history.latest(&domain).await {
        Err(e) => {
            tracing::warn!(domain = %domain, error = %e, "history lookup failed");
            server_error()
        }
        Ok(Some(entry)) => view(&entry, protocol, "no-cache"),
        Ok(None) => page(
            StatusCode::NOT_FOUND,
            "no-store",
            &EvidencePage {
                frame: Frame::new(
                    &format!("{domain} - netray"),
                    "No entry is stored for this domain.",
                    "/",
                ),
                home_href: format!("/{}", utf8_percent_encode(&domain, LOCATION_SET)),
                domain,
                protocol: protocol.as_str(),
                section: None,
                links: Vec::new(),
                help: PageHelp::EVIDENCE,
                export: None,
                selector_form: None,
            },
        ),
    }
}

async fn entry(
    State(deps): State<EvidenceDeps>,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Response {
    let Ok(Path((id, segment))) = path else {
        return not_found().await;
    };
    let Some(protocol) = PROTOCOLS.into_iter().find(|p| p.as_str() == segment) else {
        return not_found().await;
    };
    if !well_formed(&id) {
        return not_found().await;
    }
    match deps.history.get(&id).await {
        Err(e) => {
            tracing::warn!(id = %id, error = %e, "history lookup failed");
            server_error()
        }
        Ok(Some(entry)) => view(&entry, protocol, "public, max-age=3600"),
        Ok(None) => {
            if let Some(v1) = &deps.v1
                && let Some(resp) = v1(id).await
            {
                return if resp.status().is_server_error() {
                    server_error()
                } else {
                    not_found().await
                };
            }
            match gone() {
                Ok(html) => secure(
                    (
                        StatusCode::GONE,
                        [
                            (CONTENT_TYPE, "text/html; charset=utf-8"),
                            (CACHE_CONTROL, "public, max-age=3600"),
                        ],
                        html,
                    )
                        .into_response(),
                ),
                Err(_) => server_error(),
            }
        }
    }
}
