//! The history reads: a domain's latest entry, its changes and its history, and one entry by id.
use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use netray_common::error::ErrorResponse;
use netray_model::Grade;
use netray_model::diff::Diff;
use netray_model::run::Run;
use netray_store::{Entry, HistoryStore};
use serde::Serialize;
use utoipa::ToSchema;

use super::error::V1Error;
use super::state::ApiState;
use crate::entries::well_formed;

#[derive(Serialize, ToSchema)]
pub(super) struct EntryView {
    id: String,
    domain: String,
    created_at: i64,
    last_seen_at: i64,
    predecessor_id: Option<String>,
    profile_version: u32,
    grade: Grade,
    engine_version: String,
    run: Run,
}

impl From<Entry> for EntryView {
    fn from(entry: Entry) -> Self {
        let Entry {
            id,
            domain,
            created_at,
            last_seen_at,
            predecessor_id,
            profile_version,
            grade,
            engine_version,
            run,
            ..
        } = entry;
        Self {
            id,
            domain,
            created_at,
            last_seen_at,
            predecessor_id,
            profile_version,
            grade,
            engine_version,
            run,
        }
    }
}

/// `entry` as `GET /api/v1/entries/{id}` answers it.
pub(crate) fn entry_json(entry: &Entry) -> String {
    serde_json::to_string(&EntryView::from(entry.clone())).unwrap_or_default()
}

#[derive(Serialize, ToSchema)]
pub(super) struct DiffView {
    entry_id: String,
    predecessor_id: Option<String>,
    diff: Option<Diff>,
}

impl From<Entry> for DiffView {
    fn from(entry: Entry) -> Self {
        let Entry {
            id,
            predecessor_id,
            diff,
            ..
        } = entry;
        Self {
            entry_id: id,
            predecessor_id,
            diff,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(super) struct HistoryItem {
    id: String,
    created_at: i64,
    last_seen_at: i64,
    grade: Grade,
    profile_version: u32,
}

impl From<Entry> for HistoryItem {
    fn from(entry: Entry) -> Self {
        let Entry {
            id,
            created_at,
            last_seen_at,
            grade,
            profile_version,
            ..
        } = entry;
        Self {
            id,
            created_at,
            last_seen_at,
            grade,
            profile_version,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(super) struct HistoryResponse {
    entries: Vec<HistoryItem>,
}

fn history(state: &ApiState) -> Result<&Arc<HistoryStore>, V1Error> {
    state
        .store()
        .ok_or_else(|| V1Error::Unavailable("the history is switched off".into()))
}

fn domain(state: &ApiState, raw: Result<Path<String>, PathRejection>) -> Result<String, V1Error> {
    let Path(raw) = raw.map_err(|e| V1Error::InvalidInput(e.body_text()))?;
    state
        .runs()
        .validate(&raw)
        .map_err(|refusal| V1Error::DomainInvalid(refusal.message))
}

fn store_failure(what: &str, e: impl std::fmt::Display) -> V1Error {
    tracing::warn!(error = %e, "history read failed: {what}");
    V1Error::Internal("the history could not be read".into())
}

async fn latest(store: &HistoryStore, domain: &str) -> Result<Entry, V1Error> {
    store
        .latest(domain)
        .await
        .map_err(|e| store_failure(domain, e))?
        .ok_or_else(|| V1Error::NotFound(format!("no entry for {domain}")))
}

async fn entry(
    store: &HistoryStore,
    id: Result<Path<String>, PathRejection>,
) -> Result<Entry, V1Error> {
    // An id the path extractor refuses (invalid UTF-8) is malformed, as on `/r/<id>`.
    let Ok(Path(id)) = id else {
        return Err(V1Error::NotFound("no such entry".into()));
    };
    if !well_formed(&id) {
        return Err(V1Error::NotFound(format!("no entry `{id}`")));
    }
    match store.get(&id).await {
        Err(e) => {
            tracing::warn!(id = %id, error = %e, "history read failed");
            Err(V1Error::Internal("the history could not be read".into()))
        }
        Ok(None) => Err(V1Error::Gone("no entry with this id".into())),
        Ok(Some(entry)) => Ok(entry),
    }
}

/// A domain's latest entry.
#[utoipa::path(
    get,
    path = "/domains/{domain}",
    tag = "history",
    params(("domain" = String, Path, description = "The domain, any form the suite accepts")),
    responses(
        (status = 200, description = "The latest entry", body = EntryView),
        (status = 400, description = "Not a domain", body = ErrorResponse),
        (status = 404, description = "No entry for this domain", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The history could not be read", body = ErrorResponse),
        (status = 503, description = "The history is switched off", body = ErrorResponse),
    )
)]
pub(super) async fn domain_entry(
    State(state): State<ApiState>,
    raw: Result<Path<String>, PathRejection>,
) -> Result<Json<EntryView>, V1Error> {
    let store = history(&state)?;
    let domain = domain(&state, raw)?;
    Ok(Json(latest(store, &domain).await?.into()))
}

/// What changed in a domain's latest entry against its predecessor.
#[utoipa::path(
    get,
    path = "/domains/{domain}/diff",
    tag = "history",
    params(("domain" = String, Path, description = "The domain, any form the suite accepts")),
    responses(
        (status = 200, description = "The changes of the latest entry", body = DiffView),
        (status = 400, description = "Not a domain", body = ErrorResponse),
        (status = 404, description = "No entry for this domain", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The history could not be read", body = ErrorResponse),
        (status = 503, description = "The history is switched off", body = ErrorResponse),
    )
)]
pub(super) async fn domain_changes(
    State(state): State<ApiState>,
    raw: Result<Path<String>, PathRejection>,
) -> Result<Json<DiffView>, V1Error> {
    let store = history(&state)?;
    let domain = domain(&state, raw)?;
    Ok(Json(latest(store, &domain).await?.into()))
}

/// A domain's entries, newest first.
#[utoipa::path(
    get,
    path = "/domains/{domain}/history",
    tag = "history",
    params(("domain" = String, Path, description = "The domain, any form the suite accepts")),
    responses(
        (status = 200, description = "The entries, newest first", body = HistoryResponse),
        (status = 400, description = "Not a domain", body = ErrorResponse),
        (status = 404, description = "No entry for this domain", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The history could not be read", body = ErrorResponse),
        (status = 503, description = "The history is switched off", body = ErrorResponse),
    )
)]
pub(super) async fn domain_history(
    State(state): State<ApiState>,
    raw: Result<Path<String>, PathRejection>,
) -> Result<Json<HistoryResponse>, V1Error> {
    let store = history(&state)?;
    let domain = domain(&state, raw)?;
    let entries = store
        .entries(&domain)
        .await
        .map_err(|e| store_failure(&domain, e))?;
    if entries.is_empty() {
        return Err(V1Error::NotFound(format!("no entry for {domain}")));
    }
    Ok(Json(HistoryResponse {
        entries: entries.into_iter().map(HistoryItem::from).collect(),
    }))
}

/// One entry by id.
#[utoipa::path(
    get,
    path = "/entries/{id}",
    tag = "history",
    params(("id" = String, Path, description = "The entry id, eight letters or digits")),
    responses(
        (status = 200, description = "The entry", body = EntryView),
        (status = 404, description = "Not an entry id", body = ErrorResponse),
        (status = 410, description = "No entry with this id", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The history could not be read", body = ErrorResponse),
        (status = 503, description = "The history is switched off", body = ErrorResponse),
    )
)]
pub(super) async fn entry_read(
    State(state): State<ApiState>,
    id: Result<Path<String>, PathRejection>,
) -> Result<Json<EntryView>, V1Error> {
    let store = history(&state)?;
    Ok(Json(entry(store, id).await?.into()))
}

/// What changed in one entry against its predecessor.
#[utoipa::path(
    get,
    path = "/entries/{id}/diff",
    tag = "history",
    params(("id" = String, Path, description = "The entry id, eight letters or digits")),
    responses(
        (status = 200, description = "The changes of the entry", body = DiffView),
        (status = 404, description = "Not an entry id", body = ErrorResponse),
        (status = 410, description = "No entry with this id", body = ErrorResponse),
        (status = 413, description = "Request body over the 8 KiB limit", body = ErrorResponse),
        (status = 429, description = "Rate limited", body = ErrorResponse),
        (status = 500, description = "The history could not be read", body = ErrorResponse),
        (status = 503, description = "The history is switched off", body = ErrorResponse),
    )
)]
pub(super) async fn entry_changes(
    State(state): State<ApiState>,
    id: Result<Path<String>, PathRejection>,
) -> Result<Json<DiffView>, V1Error> {
    let store = history(&state)?;
    Ok(Json(entry(store, id).await?.into()))
}
