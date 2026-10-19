// V2 http-checks C3, C14-C16: `http.headers` keeps one row per `Set-Cookie` value (`set-cookie`,
// `set-cookie#2`, ...), joins other repeated headers with `, `, and a JSON replay without raw
// headers yields exactly one `set-cookie` row.

use netray_engine::SectionOutcome;
use netray_http::inspect::assembler::InspectResponse;
use netray_http::module::{REGISTER, translate};
use netray_model::evidence::{Shape, Value as Ev, validate};
use serde_json::{Value, json};

const SPECTRA_JSON: &str =
    include_str!("../../../../tests/fixtures/contracts/spectra-inspect.json");

fn fixture(mutate: impl FnOnce(&mut Value)) -> InspectResponse {
    let mut v: Value = serde_json::from_str(SPECTRA_JSON).unwrap();
    mutate(&mut v);
    serde_json::from_value(v).unwrap()
}

/// The `http.headers` rows as (name, value) text pairs.
fn header_rows(resp: &InspectResponse) -> Vec<(String, String)> {
    let SectionOutcome::Measured { evidence, .. } = translate(resp) else {
        panic!("expected Measured");
    };
    validate("http", &REGISTER, &evidence).expect("blocks fit the register");
    let block = evidence
        .iter()
        .find(|b| b.id.to_string() == "http.headers")
        .expect("http.headers block");
    let Shape::Table { columns, rows, .. } = &block.shape else {
        panic!("http.headers must be a table");
    };
    let at = |c: &str| columns.iter().position(|x| x == c).unwrap();
    let text = |e: &Ev| match e {
        Ev::Text(t) => t.clone(),
        other => panic!("expected text, got {other:?}"),
    };
    rows.iter()
        .map(|r| (text(&r[at("name")]), text(&r[at("value")])))
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

fn named<'a>(rows: &'a [(String, String)], prefix: &str) -> Vec<&'a (String, String)> {
    rows.iter().filter(|(n, _)| n.starts_with(prefix)).collect()
}

#[test]
fn three_set_cookie_values_get_ordinal_rows_in_received_order() {
    let mut resp = fixture(|_| {});
    resp.raw_headers = pairs(&[
        ("content-type", "text/html"),
        ("set-cookie", "a=1; Path=/"),
        ("set-cookie", "b=2; Path=/"),
        ("set-cookie", "c=3; Path=/"),
    ]);
    let rows = header_rows(&resp);
    let cookies: Vec<(String, String)> = named(&rows, "set-cookie").into_iter().cloned().collect();
    assert_eq!(
        cookies,
        pairs(&[
            ("set-cookie", "a=1; Path=/"),
            ("set-cookie#2", "b=2; Path=/"),
            ("set-cookie#3", "c=3; Path=/"),
        ])
    );
}

#[test]
fn repeated_vary_stays_one_row_joined_with_comma() {
    let mut resp = fixture(|_| {});
    resp.raw_headers = pairs(&[("vary", "accept"), ("vary", "accept-encoding")]);
    let rows = header_rows(&resp);
    let vary: Vec<(String, String)> = named(&rows, "vary").into_iter().cloned().collect();
    assert_eq!(vary, pairs(&[("vary", "accept, accept-encoding")]));
}

#[test]
fn json_replay_without_raw_headers_has_one_set_cookie_row() {
    let resp = fixture(|v| {
        v["headers"] = json!({"set-cookie": "a=1; Path=/"});
    });
    assert!(resp.raw_headers.is_empty());
    let rows = header_rows(&resp);
    let cookies: Vec<(String, String)> = named(&rows, "set-cookie").into_iter().cloned().collect();
    assert_eq!(cookies, pairs(&[("set-cookie", "a=1; Path=/")]));
}
