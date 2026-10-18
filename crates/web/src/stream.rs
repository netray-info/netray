//! The server-sent events of a streamed run: one `fragment` per band, then `end`.

use axum::response::sse::Event;
use serde::Serialize;

#[derive(Serialize)]
struct Fragment<'a> {
    id: &'a str,
    html: &'a str,
}

/// A `fragment` event carrying `{"id", "html"}` as JSON.
pub fn fragment(id: &str, html: &str) -> Event {
    let data = serde_json::to_string(&Fragment { id, html }).expect("a fragment serialises");
    Event::default().event("fragment").data(data)
}

/// The `end` event: nothing follows it in this run.
pub fn end() -> Event {
    Event::default().event("end").data("{}")
}
