//! Contract tests for the `facts_grid` component (v2-http-ui, requirement 5; scenarios C5, C19).
use std::fs;
use std::path::PathBuf;

use netray_web::components::{Cell, FactCell, FactsGridView, Tone, facts_grid};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

fn cell(label: &str, values: &[&str], chips: bool, tone: Option<Tone>) -> FactCell {
    FactCell {
        label: label.into(),
        values: values.iter().map(|v| Cell::from(*v)).collect(),
        chips,
        tone,
    }
}

#[test]
fn facts_grid_renders_cells_tone_chips_and_links() {
    let html = facts_grid(&FactsGridView {
        cells: vec![
            cell("Status", &["200"], false, None),
            cell("max-age", &["1 year (31536000 s)"], false, Some(Tone::Warn)),
            cell("Indicators", &["cf-ray", "server: cloudflare"], true, None),
        ],
    })
    .unwrap();

    assert!(
        html.contains("<div class=\"facts-grid\""),
        "root missing in: {html}"
    );
    assert_eq!(
        html.matches("class=\"facts-grid__label\"").count(),
        3,
        "in: {html}"
    );
    for text in [
        "Status",
        "max-age",
        "Indicators",
        "200",
        "1 year (31536000 s)",
        "cf-ray",
        "server: cloudflare",
    ] {
        assert!(html.contains(text), "missing `{text}` in: {html}");
    }
    assert_eq!(html.matches("data-tone=").count(), 1, "in: {html}");
    assert_eq!(html.matches("data-tone=\"warn\"").count(), 1, "in: {html}");
    assert_eq!(
        html.matches("class=\"facts-grid__chip\"").count(),
        2,
        "in: {html}"
    );

    let linked = facts_grid(&FactsGridView {
        cells: vec![FactCell {
            label: "Final URL".into(),
            values: vec![Cell {
                text: "192.0.2.1".into(),
                href: Some("/192.0.2.1".into()),
                badges: vec![],
                datetime: None,
                key: None,
            }],
            chips: false,
            tone: None,
        }],
    })
    .unwrap();
    assert!(linked.contains("href=\"/192.0.2.1\""), "in: {linked}");
    assert!(linked.contains("<a "), "in: {linked}");
}

#[test]
fn facts_grid_has_one_macro_and_a_stylesheet_with_the_board_g_columns() {
    let src = read("templates/components/facts_grid.html");
    assert_eq!(
        src.matches("{% macro facts_grid(").count(),
        1,
        "facts_grid.html must define the macro exactly once"
    );

    let css = read("static/facts_grid.css");
    assert!(
        css.contains(".facts-grid"),
        "no .facts-grid in facts_grid.css"
    );
    assert!(
        css.contains("grid-template-columns") && css.contains("minmax(240px"),
        "facts_grid.css must declare grid-template-columns with minmax(240px"
    );
}
