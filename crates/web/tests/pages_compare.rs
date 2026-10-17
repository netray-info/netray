//! `/compare` through the composed router against the V1 page
//! `site/compare/index.html` (v2-pages requirements 23-25, Phase 5).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use http_body_util::BodyExt;
use netray_web::pages;
use tower::ServiceExt;

fn app() -> Router {
    Router::new()
        .route("/", get(pages::query_redirect))
        .merge(pages::router())
        .fallback(pages::not_found)
}

async fn get_compare() -> (StatusCode, String) {
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/compare")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

// ---- minimal markup helpers -------------------------------------------------

fn decode(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "nbsp" => Some(' '),
            _ => name
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Strip tags, decode entities, collapse whitespace.
fn text(html: &str) -> String {
    let mut plain = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                plain.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }
    decode(&plain)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Inner HTML of every `<tag ...>...</tag>` in `html` (tag name followed by space or `>`).
fn elements<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        if !after.starts_with([' ', '>', '\n']) {
            rest = after;
            continue;
        }
        let gt = after.find('>').unwrap();
        let attrs = &after[..gt];
        let body = &after[gt + 1..];
        let end = body
            .find(&close)
            .unwrap_or_else(|| panic!("unclosed <{tag}>"));
        out.push((attrs, &body[..end]));
        rest = &body[end + close.len()..];
    }
    out
}

struct Row {
    label: String,
    symbols: Vec<String>, // every td, netray first
    text: String,         // whole row, normalised
    notes: Vec<String>,   // competitor notes only (V1 detail block; empty for rendered rows)
}

struct Table {
    headers: Vec<String>, // th scope="col"
    rows: Vec<Row>,
}

fn parse_table(html: &str) -> Table {
    let headers = elements(html, "th")
        .into_iter()
        .filter(|(a, _)| a.contains(r#"scope="col""#))
        .map(|(_, b)| text(b))
        .collect();
    let rows = elements(html, "tr")
        .into_iter()
        .filter(|(_, b)| b.contains(r#"scope="row""#))
        .map(|(_, b)| {
            let label = text(elements(b, "summary")[0].1);
            let symbols = elements(b, "td")
                .into_iter()
                .map(|(_, c)| text(c))
                .collect();
            let notes = elements(b, "div")
                .into_iter()
                .find(|(a, _)| a.contains("row-detail"))
                .map(|(_, d)| {
                    d.split("<br>")
                        .filter(|s| s.contains("</strong>"))
                        .skip(1) // the netray entry
                        .map(|s| text(s.split("</strong>").nth(1).unwrap()))
                        .collect()
                })
                .unwrap_or_default();
            Row {
                label,
                symbols,
                text: text(b),
                notes,
            }
        })
        .collect();
    Table { headers, rows }
}

fn tables(html: &str) -> Vec<Table> {
    elements(html, "table")
        .into_iter()
        .map(|(_, b)| parse_table(b))
        .collect()
}

fn v1() -> String {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../site/compare/index.html");
    std::fs::read_to_string(p).expect("read V1 compare page")
}

fn v1_tables() -> Vec<Table> {
    let t = tables(&v1());
    assert_eq!(t.len(), 5, "V1: Table 1 plus four axes");
    t
}

// ---- tests ------------------------------------------------------------------

#[tokio::test]
async fn compare_page_carries_every_v1_row_header_and_competitor_cell() {
    let (status, body) = get_compare().await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"<html lang="en">"#), "frame");
    let plain = text(&body);
    assert!(
        plain.contains("Table 1 \u{2014} Unified domain checkers"),
        "Table 1 heading"
    );
    assert!(
        plain.contains("Table 2 \u{2014} Per-axis specialists"),
        "Table 2 heading"
    );

    let old = v1_tables();
    let new = tables(&body);
    assert_eq!(
        new.len(),
        old.len(),
        "rendered tables: Table 1 plus the axes"
    );

    for (n, (o, r)) in old.iter().zip(new.iter()).enumerate() {
        assert_eq!(r.headers.len(), o.headers.len(), "table {n}: header count");
        assert_eq!(r.headers[1], "netray", "table {n}: netray header");
        for (i, h) in o.headers.iter().enumerate().skip(2) {
            assert_eq!(&r.headers[i], h, "table {n}: competitor header {i}");
        }
        assert_ne!(
            r.headers[1], o.headers[1],
            "table {n}: V1 netray header kept"
        );
        assert_eq!(r.rows.len(), o.rows.len(), "table {n}: row count");
        for orow in &o.rows {
            let hits: Vec<&Row> = r.rows.iter().filter(|x| x.label == orow.label).collect();
            assert_eq!(
                hits.len(),
                1,
                "table {n}: label {:?} exactly once",
                orow.label
            );
            let rrow = hits[0];
            assert_eq!(
                rrow.symbols.len(),
                orow.symbols.len(),
                "{:?}: cells",
                orow.label
            );
            for (i, sym) in orow.symbols.iter().enumerate().skip(1) {
                assert_eq!(
                    &rrow.symbols[i], sym,
                    "{:?}: symbol of column {i}",
                    orow.label
                );
            }
            assert_eq!(
                orow.notes.len(),
                orow.symbols.len() - 1,
                "V1 {:?}: notes",
                orow.label
            );
            for note in &orow.notes {
                assert!(
                    rrow.text.contains(note.as_str()),
                    "{:?}: note {note:?}",
                    orow.label
                );
            }
        }
    }
}

#[tokio::test]
async fn compare_page_shape_details_rows_no_island_honesty_and_review_date() {
    let (_, body) = get_compare().await;
    let main = body
        .split("<main")
        .nth(1)
        .and_then(|m| m.split("</main>").next())
        .expect("<main>");
    assert!(!main.contains("data-island"), "no island inside <main>");

    let old = v1();
    let n_rows: usize = v1_tables().iter().map(|t| t.rows.len()).sum();
    assert_eq!(
        elements(main, "details").len(),
        n_rows,
        "one <details> per row"
    );

    let plain = text(&body);
    assert!(
        plain.contains("Where netray trades depth for breadth"),
        "honesty section"
    );
    assert!(
        !plain.contains("Where lens trades"),
        "no V1 codename in the heading"
    );
    let date = text(
        old.split("Last reviewed:")
            .nth(1)
            .unwrap()
            .split("</p>")
            .next()
            .unwrap(),
    );
    assert_eq!(date, "2026-05-09");
    assert!(
        plain.contains(&format!("Last reviewed: {date}")),
        "Last reviewed line"
    );
}

#[test]
fn compare_rows_cover_every_v1_row_and_every_v1_unsupported_row_has_no_checks() {
    let old: Vec<Row> = v1_tables().into_iter().flat_map(|t| t.rows).collect();
    let rows = pages::compare::rows();
    assert_eq!(rows.len(), old.len(), "row count across both tables");
    for (o, r) in old.iter().zip(rows.iter()) {
        assert_eq!(r.label, o.label, "document order");
        if o.symbols[0] == "\u{2717}" {
            assert_eq!(
                r.checks,
                Some(vec![]),
                "{:?}: V1 netray cell is not-done",
                o.label
            );
        }
    }
}
