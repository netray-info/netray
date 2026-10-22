//! V2 spec v2-ui-polish, requirement 8 (C1 baseline part, C4, C5, C6, C7): the stored email
//! evidence view groups its explore links in one section, wraps check rows and their links in one
//! card; the evidence block component carries the phone swipe note and the empty-block note.
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use netray_engine::catalogue::catalogue;
use netray_model::diff::diff;
use netray_model::evidence::{Block, BlockId, EVIDENCE_SCHEMA_VERSION, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{Grade, Protocol, Status};
use netray_store::HistoryStore;
use netray_web::components::evidence_block;
use netray_web::evidence::block_view;
use netray_web::pages::{EvidenceDeps, evidence_router};
use tower::ServiceExt;

fn text(s: &str) -> Value {
    Value::Text(s.into())
}

fn block(id: &str, shape: Shape) -> Block {
    Block {
        id: BlockId::parse(id).unwrap(),
        title: "Title".into(),
        shape,
    }
}

fn table(columns: &[&str]) -> Block {
    block(
        "dns.resolvers@192.0.2.1",
        Shape::Table {
            key: columns[0].into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows: vec![columns.iter().map(|c| text(c)).collect()],
        },
    )
}

fn email_blocks() -> Vec<Block> {
    let fields = Shape::Fields(vec![("policy".into(), text("reject"))]);
    vec![
        block(
            "email.mx",
            Shape::Table {
                key: "host".into(),
                columns: vec!["host".into(), "preference".into()],
                rows: vec![vec![text("mx1.shop.example.com"), Value::Int(10)]],
            },
        ),
        block("email.dmarc", fields.clone()),
        block("email.mta_sts", fields),
    ]
}

async fn stored_email_view() -> String {
    let dir = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    let blocks = email_blocks();
    let checks: Vec<RunCheck> = catalogue()
        .entries()
        .iter()
        .filter(|e| e.protocol == Protocol::Email)
        .enumerate()
        .map(|(i, e)| RunCheck {
            id: e.id.clone(),
            status: Status::Pass,
            findings: Vec::new(),
            evidence: blocks
                .iter()
                .filter(|b| {
                    let same = b.id.to_string() == e.id.to_string();
                    same || (i == 0
                        && !catalogue()
                            .entries()
                            .iter()
                            .any(|c| c.id.to_string() == b.id.to_string()))
                })
                .map(|b| b.id.clone())
                .collect(),
        })
        .collect();
    let run = Run {
        profile_version: 3,
        engine_version: "0.26.0".into(),
        evidence_schema_version: EVIDENCE_SCHEMA_VERSION,
        grade: Grade::B,
        complete: true,
        sections: vec![RunSection {
            protocol: Protocol::Email,
            outcome: SectionKind::Measured,
            reason: None,
        }],
        checks,
        blocks,
    };
    store
        .record("shop.example.com", &run, &[], 1_800_000_000, |p, c| {
            diff(p, c, &[], None)
        })
        .await
        .unwrap();
    let app = evidence_router(EvidenceDeps {
        history: Arc::new(store),
    });
    let resp = app
        .oneshot(
            Request::get("/shop.example.com/email")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The text between `open` and the next `close` after it.
fn between<'a>(html: &'a str, open: &str, close: &str) -> &'a str {
    let from = html
        .find(open)
        .unwrap_or_else(|| panic!("`{open}` missing in: {html}"));
    let rest = &html[from..];
    let to = rest.find(close).map_or(rest.len(), |i| i + close.len());
    &rest[..to]
}

/// Anchor `(href, text)` pairs of `html`, in order.
fn anchors(html: &str) -> Vec<(String, String)> {
    html.split("<a ")
        .skip(1)
        .map(|a| {
            let href = a
                .split("href=\"")
                .nth(1)
                .and_then(|r| r.split('"').next())
                .unwrap_or_default()
                .to_string();
            let label = a[a.find('>').unwrap() + 1..].split("</a>").next().unwrap();
            (href, label.trim().to_string())
        })
        .collect()
}

#[tokio::test]
async fn email_view_groups_explore_links_into_one_section() {
    let page = stored_email_view().await;

    assert_eq!(
        page.matches(r#"<section class="evidence-view__explore">"#)
            .count(),
        1,
        "{page}"
    );
    let section = between(
        &page,
        r#"<section class="evidence-view__explore">"#,
        "</section>",
    );
    assert!(section.contains("<h2>Explore live</h2>"), "{section}");
    assert!(section.contains("<ul>"), "{section}");
    let links = anchors(section);
    let labels: Vec<&str> = links.iter().map(|l| l.1.as_str()).collect();
    assert_eq!(
        labels,
        [
            "DNS · shop.example.com MX",
            "TLS · mx1.shop.example.com:25",
            "DNS · _dmarc.shop.example.com TXT",
            "DNS · _mta-sts.shop.example.com TXT",
        ],
        "{section}"
    );
    let hrefs: Vec<&str> = links.iter().map(|l| l.0.as_str()).collect();
    assert_eq!(
        hrefs,
        [
            "/inspect/dns?q=shop.example.com%20MX",
            "/inspect/tls?h=mx1.shop.example.com:25",
            "/inspect/dns?q=_dmarc.shop.example.com%20TXT",
            "/inspect/dns?q=_mta-sts.shop.example.com%20TXT",
        ]
    );
    assert!(
        !anchors(&page).iter().any(|(_, t)| t == "Explore live"),
        "no bare Explore live link: {page}"
    );
}

#[tokio::test]
async fn email_view_wraps_check_rows_and_links_in_one_card() {
    let page = stored_email_view().await;
    let start = page
        .find(r#"<section class="evidence-section""#)
        .unwrap_or_else(|| panic!("no evidence-section in: {page}"));
    let rest = &page[start..];
    let end = rest
        .find(r#"<section class="evidence-block"#)
        .unwrap_or(rest.len());
    let section = &rest[..end];

    assert_eq!(
        section
            .matches(r#"<div class="evidence-section__rows">"#)
            .count(),
        1,
        "{section}"
    );
    let card = section
        .find(r#"<div class="evidence-section__rows">"#)
        .unwrap();
    let first_row = section
        .find("<article class=\"check-row")
        .unwrap_or_else(|| panic!("no check-row article in: {section}"));
    let last_row = section.rfind("<article class=\"check-row").unwrap();
    let first_link = section.find(r#"class="evidence-section__link""#).unwrap();
    let last_link = section.rfind(r#"class="evidence-section__link""#).unwrap();
    assert!(
        card < first_row,
        "card opens before the first row: {section}"
    );
    assert!(
        card < first_link,
        "card opens before the first link: {section}"
    );
    // The card holds the last row and the last link: no `</section>` or block between them
    // and the card, and the first link follows the row it belongs to.
    assert!(first_row < first_link, "{section}");
    assert!(last_row < section.len() && last_link < section.len());
    let closing = section[card..].find("</div>").map(|i| i + card).unwrap();
    assert!(
        closing > last_link || section[card..last_link].matches("<div").count() > 1,
        "the card closes before its last link: {section}"
    );
}

#[test]
fn table_block_of_three_columns_carries_the_swipe_note() {
    let wide = evidence_block(&block_view(&table(&["a", "b", "c"]))).unwrap();
    let narrow = evidence_block(&block_view(&table(&["a", "b"]))).unwrap();
    let note = r#"<p class="evidence-block__swipe">← swipe the table</p>"#;

    let at = wide
        .find(note)
        .unwrap_or_else(|| panic!("no swipe note: {wide}"));
    assert!(
        at > wide.find("</table>").unwrap(),
        "note after the table: {wide}"
    );
    assert!(
        at < wide.rfind("</section>").unwrap(),
        "note inside the section: {wide}"
    );
    assert!(!narrow.contains("evidence-block__swipe"), "{narrow}");
}

#[test]
fn stylesheet_hides_swipe_note_by_default_and_aligns_cells_on_the_baseline() {
    let css = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static/evidence_block.css"),
    )
    .unwrap();
    let flat: String = css.split_whitespace().collect::<Vec<_>>().join(" ");

    let media = flat
        .find("@media (max-width: 480px)")
        .unwrap_or_else(|| panic!("no 480px media query: {css}"));
    let (base, phone) = flat.split_at(media);
    let rule = |css: &str, selector: &str| -> String {
        let at = css
            .find(selector)
            .unwrap_or_else(|| panic!("no `{selector}` in {css}"));
        let body = &css[at..];
        body[body.find('{').unwrap()..body.find('}').unwrap()].to_string()
    };
    assert!(
        rule(base, ".evidence-block__swipe").contains("display: none"),
        "{css}"
    );
    assert!(
        rule(phone, ".evidence-block__swipe").contains("display: block"),
        "{css}"
    );

    let cells = flat
        .split('}')
        .find(|r| r.contains(".evidence-block__table td"))
        .unwrap_or_else(|| panic!("no cell rule in {css}"));
    assert!(cells.contains("vertical-align: baseline"), "{cells}");
}

#[test]
fn empty_block_says_nothing_recorded_and_a_filled_one_does_not() {
    let empty = evidence_block(&block_view(&block("email.bimi", Shape::Fields(vec![])))).unwrap();
    let filled = evidence_block(&block_view(&block(
        "email.bimi",
        Shape::Fields(vec![("record".into(), text("v=BIMI1"))]),
    )))
    .unwrap();
    let note = r#"<p class="evidence-block__empty">Nothing recorded.</p>"#;

    let at = empty
        .find(note)
        .unwrap_or_else(|| panic!("no empty note: {empty}"));
    assert!(at < empty.rfind("</section>").unwrap(), "{empty}");
    assert!(!filled.contains("evidence-block__empty"), "{filled}");
}
