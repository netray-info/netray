//! Typed evidence values render as text (V2 spec v2-inspectors, Phase 1, C4).
use std::net::{IpAddr, Ipv4Addr};

use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::{block_view, format_value};

fn rows() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("count", Value::Int(3), "3"),
        ("neg", Value::Int(-7), "-7"),
        ("on", Value::Bool(true), "true"),
        ("off", Value::Bool(false), "false"),
        (
            "seen",
            Value::Time("2026-10-08T14:32:00Z".into()),
            "2026-10-08T14:32:00Z",
        ),
        ("rtt", Value::Duration(120), "120 ms"),
        (
            "v6",
            Value::Addr("2001:db8::1".parse().unwrap()),
            "2001:db8::1",
        ),
        (
            "v4",
            Value::Addr(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))),
            "192.0.2.1",
        ),
        (
            "note",
            Value::Text("as is, <verbatim>".into()),
            "as is, <verbatim>",
        ),
    ]
}

#[test]
fn format_value_renders_each_type_as_specified() {
    for (key, value, expected) in rows() {
        assert_eq!(format_value(&value), expected, "value for `{key}`");
    }
}

#[test]
fn fields_block_renders_typed_values_in_dd() {
    let block = Block {
        id: BlockId::parse("ip.addresses").unwrap(),
        title: "Values".into(),
        shape: Shape::Fields(
            rows()
                .into_iter()
                .filter(|(k, _, _)| *k != "note")
                .map(|(k, v, _)| (k.to_string(), v))
                .collect(),
        ),
    };
    let view = block_view(&block);
    assert_eq!(view.id, "ev-ip.addresses");
    let html = evidence_block(&view).unwrap();
    for (key, _, expected) in rows().into_iter().filter(|(k, _, _)| *k != "note") {
        let needle = format!("<dd>{expected}</dd>");
        assert!(
            html.contains(&needle),
            "`{key}`: missing `{needle}` in: {html}"
        );
    }
}
