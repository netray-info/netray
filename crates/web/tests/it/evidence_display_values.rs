//! V2 spec v2-http-ui, requirement 6 (C6, C13, C14, C15, C16): the evidence block shows values
//! for people (durations, booleans, times), while `format_value` stays raw for the exports.
use netray_model::evidence::{Block, BlockId, Shape, Value};
use netray_web::components::evidence_block;
use netray_web::evidence::{block_view, display_value, format_value};

fn fields(pairs: Vec<(&str, Value)>) -> String {
    let block = Block {
        id: BlockId::parse("x.fields").unwrap(),
        title: "Values".into(),
        shape: Shape::Fields(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()),
    };
    evidence_block(&block_view(&block)).unwrap()
}

#[test]
fn display_value_formats_durations() {
    for (ms, expected) in [
        (77, "77 ms"),
        (1234, "1.2 s"),
        (59_960, "59.9 s"),
        (60_000, "1 min 0 s"),
        (125_000, "2 min 5 s"),
    ] {
        assert_eq!(display_value(&Value::Duration(ms)), expected, "{ms} ms");
    }
}

#[test]
fn format_value_stays_raw() {
    assert_eq!(format_value(&Value::Bool(true)), "true");
    assert_eq!(
        format_value(&Value::Time("2024-01-01T00:00:00Z".into())),
        "2024-01-01T00:00:00Z"
    );
    assert_eq!(format_value(&Value::Duration(1234)), "1234 ms");
}

#[test]
fn fields_block_shows_duration_without_ms_suffix() {
    let html = fields(vec![("response_ms", Value::Duration(77))]);
    assert!(html.contains("<dt>response</dt>"), "{html}");
    assert!(!html.contains("response_ms"), "{html}");
    assert!(html.contains("77 ms"), "{html}");
}

#[test]
fn fields_block_renders_time_element() {
    let html = fields(vec![("seen", Value::Time("2024-01-01T00:00:00Z".into()))]);
    assert!(
        html.contains(r#"<time datetime="2024-01-01T00:00:00Z">2024-01-01 00:00 UTC</time>"#),
        "{html}"
    );
}

#[test]
fn fields_block_renders_bool_as_yes() {
    let html = fields(vec![("enabled", Value::Bool(true))]);
    assert!(html.contains("<dd>yes</dd>"), "{html}");
    assert!(!html.contains("true"), "{html}");
}

#[test]
fn times_before_1970_and_with_an_offset_read_in_utc() {
    for (raw, shown) in [
        ("1960-01-01T00:00:00Z", "1960-01-01 00:00 UTC"),
        ("1969-12-31T23:59:00Z", "1969-12-31 23:59 UTC"),
        ("2024-01-01T01:30:00+02:00", "2023-12-31 23:30 UTC"),
    ] {
        assert_eq!(display_value(&Value::Time(raw.into())), shown, "{raw}");
    }
}
