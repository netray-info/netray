//! v2-http-ui Phase 2 (C3, C11, C12): the http.server view.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::pages::views::{ViewContext, render};

fn block(fields: Vec<(&str, Value)>) -> Block {
    Block {
        id: BlockId::parse("http.server").unwrap(),
        title: "Server".into(),
        shape: Shape::Fields(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        ),
    }
}

fn full(inspection_ms: u64) -> Block {
    block(vec![
        ("final_url", Value::Text("https://example.com/".into())),
        ("status_code", Value::Int(200)),
        ("http_version", Value::Text("HTTP/2".into())),
        ("inspection_ms", Value::Duration(inspection_ms)),
        ("address", Value::Addr("192.0.2.10".parse().unwrap())),
        ("org", Value::Text("Example Org".into())),
        ("network_type", Value::Text("hosting".into())),
    ])
}

fn render_with(block: &Block, linked: bool) -> Option<String> {
    let none = |_: &BlockId, _: &str, _: &Value| None;
    let links = netray_web::links::decorate(&[]);
    let ctx = ViewContext {
        statuses: &[],
        decorate: if linked { &links } else { &none },
        feeds: &[],
    };
    render(block, &ctx)
}

#[test]
fn c3_server_view_shows_labels_values_and_the_inspection_note_without_tone() {
    let html = render_with(&full(1234), false).expect("claimed");
    assert!(
        html.contains(r#"<h3 class="protocol-view__title">Server</h3>"#),
        "{html}"
    );
    for label in [
        "Final URL",
        "Status",
        "HTTP version",
        "Address",
        "Organisation",
        "Network type",
    ] {
        assert!(html.contains(&format!(">{label}<")), "{label}: {html}");
    }
    for value in [
        "https://example.com/",
        "200",
        "HTTP/2",
        "192.0.2.10",
        "Example Org",
        "hosting",
    ] {
        assert!(html.contains(value), "{value}: {html}");
    }
    assert!(
        html.contains(r#"<p class="protocol-view__note">Inspection took 1.2 s</p>"#),
        "{html}"
    );
    assert!(!html.contains("data-tone"), "{html}");
}

#[test]
fn c3_a_short_inspection_reads_in_milliseconds() {
    let html = render_with(&full(77), false).expect("claimed");
    assert!(html.contains("Inspection took 77 ms"), "{html}");
}

#[test]
fn c11_the_address_carries_the_shell_link() {
    let html = render_with(&full(1234), true).expect("claimed");
    assert!(html.contains("evidence-value__link"), "{html}");
}

#[test]
fn c12_a_block_with_an_unknown_key_is_not_claimed() {
    let b = block(vec![
        ("final_url", Value::Text("https://example.com/".into())),
        ("response_ms", Value::Duration(1234)),
    ]);
    assert_eq!(render_with(&b, false), None);
}

#[test]
fn c12_an_error_without_final_url_shows_the_error_cell() {
    let b = block(vec![("error_kind", Value::Text("refused".into()))]);
    let html = render_with(&b, false).expect("claimed");
    assert!(html.contains(">Error<"), "{html}");
    assert!(html.contains("refused"), "{html}");
}
