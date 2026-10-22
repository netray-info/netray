//! v2-http-ui Phase 2 (C2, C9, C10): the HSTS view's max-age text, tones and fallback.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_model::{CheckId, Status};
use netray_web::pages::views::{ViewContext, render};

fn block(fields: Vec<(&str, Value)>) -> Block {
    Block {
        id: BlockId::parse("http.hsts").unwrap(),
        title: "HSTS".into(),
        shape: Shape::Fields(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
    }
}

fn view(block: &Block, statuses: &[(CheckId, Status)]) -> String {
    let decorate = |_: &BlockId, _: &str, _: &Value| None;
    let ctx = ViewContext {
        statuses,
        decorate: &decorate,
        feeds: &[],
    };
    render(block, &ctx).expect("hsts block is claimed")
}

#[test]
fn c2_max_age_reads_as_the_largest_whole_unit_with_its_seconds() {
    let rows: [(i64, &str); 8] = [
        (31_536_000, "1 year (31536000 s)"),
        (63_072_000, "2 years (63072000 s)"),
        (86_400, "1 day (86400 s)"),
        (3_600, "1 h (3600 s)"),
        (120, "2 min (120 s)"),
        (42, "42 s"),
        (0, "0 s"),
        (90_061, "90061 s"),
    ];
    for (seconds, text) in rows {
        let html = view(&block(vec![("max_age", Value::Int(seconds))]), &[]);
        assert!(html.contains(text), "{seconds}: {html}");
    }
}

#[test]
fn c9_cells_carry_the_check_tone_or_none() {
    let b = block(vec![
        ("max_age", Value::Int(31_536_000)),
        ("include_subdomains", Value::Bool(true)),
        ("preload", Value::Bool(false)),
    ]);
    let statuses = [(CheckId::parse("http.hsts").unwrap(), Status::Pass)];
    let html = view(&b, &statuses);
    for text in ["1 year (31536000 s)", ">yes<", ">no<"] {
        assert!(html.contains(text), "{text}: {html}");
    }
    for label in ["max-age", "includeSubDomains", "preload"] {
        assert!(html.contains(&format!(">{label}<")), "{label}: {html}");
    }
    assert_eq!(
        html.matches(r#"<div class="facts-grid__cell" data-tone="pass">"#)
            .count(),
        3,
        "{html}"
    );
    let plain = view(&b, &[]);
    assert!(!plain.contains("data-tone"), "{plain}");
}

#[test]
fn c10_without_max_age_the_cell_says_not_parsed() {
    let html = view(&block(vec![("preload", Value::Bool(false))]), &[]);
    assert!(html.contains("not parsed"), "{html}");
}
