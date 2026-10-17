//! The SSE bytes the Rust encoder puts on the wire, pinned to the fixture the island's
//! vitest suite (`crates/web/islands/test/stream-fixture.test.ts`) feeds to the stream island.
//!
//! The events go through axum's real `Sse` response body, so the framing is exactly what a
//! client receives. axum writes nothing for an empty `data("")`, so `end` is `event: end\n\n`.
//!
//! `UPDATE_GOLDEN=1 cargo test -p netray-web --test stream_wire` rewrites stream.sse.

use std::convert::Infallible;
use std::path::PathBuf;

use axum::response::IntoResponse;
use axum::response::sse::Sse;
use http_body_util::BodyExt;
use netray_web::stream;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("islands/test/fixtures/stream.sse")
}

async fn wire() -> String {
    let events = futures::stream::iter([
        Ok::<_, Infallible>(stream::fragment("band-dns", "<g id=\"band-dns\"></g>")),
        Ok(stream::end()),
    ]);
    let body = Sse::new(events).into_response().into_body();
    let bytes = body.collect().await.expect("collect body").to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

#[tokio::test]
async fn stream_wire_matches_island_fixture() {
    let got = wire().await;

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(fixture_path(), &got).expect("write fixture");
    }
    let want = std::fs::read_to_string(fixture_path()).expect("fixture stream.sse is missing");
    assert_eq!(
        got, want,
        "encoder output differs from the fixture; if intended, run with UPDATE_GOLDEN=1"
    );

    let events: Vec<&str> = got.split("\n\n").filter(|e| !e.is_empty()).collect();
    assert_eq!(events.len(), 2);
    let first: Vec<&str> = events[0].lines().collect();
    assert_eq!(first[0], "event: fragment");
    let data = first[1].strip_prefix("data: ").expect("data line");
    let json: serde_json::Value = serde_json::from_str(data).expect("fragment data is JSON");
    assert_eq!(json["id"], "band-dns");
    assert_eq!(json["html"], "<g id=\"band-dns\"></g>");
    assert_eq!(events[1].lines().next(), Some("event: end"));
}
