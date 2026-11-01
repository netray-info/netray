//! pustina-findings C6/C14: every catalogue check carries a `checks` text, one positive sentence
//! saying what the check verifies. The completeness test fails on a missing or empty one.
//! The file is read as a generic TOML value, so the test does not depend on `CheckSpec`'s fields.

use toml::Value;

const EMBEDDED: &str = include_str!("../../catalogue/checks.toml");

/// One problem per check entry whose `checks` is missing, empty or not one sentence.
fn checks_text_problems(toml_text: &str) -> Vec<String> {
    let doc: Value = toml::from_str(toml_text).expect("catalogue parses as TOML");
    let entries = doc
        .get("check")
        .and_then(Value::as_array)
        .expect("[[check]] tables");
    let mut problems = Vec::new();
    for e in entries {
        let id = e.get("id").and_then(Value::as_str).unwrap_or("?");
        match e.get("checks").and_then(Value::as_str) {
            None => problems.push(format!("{id}: checks is missing")),
            Some(t) if t.trim().is_empty() => problems.push(format!("{id}: checks is empty")),
            Some(t) => {
                let t = t.trim();
                let body = t.strip_suffix('.');
                match body {
                    None => problems.push(format!("{id}: checks does not end with a period")),
                    Some(b) if b.contains(". ") => {
                        problems.push(format!("{id}: checks is more than one sentence"))
                    }
                    Some(_) => {}
                }
            }
        }
    }
    problems
}

#[test]
fn every_catalogue_check_has_a_one_sentence_checks_text() {
    let problems = checks_text_problems(EMBEDDED);
    assert!(
        problems.is_empty(),
        "{} checks without a valid `checks` text:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_checks_text_completeness_helper_flags_a_missing_and_an_empty_text() {
    let snippet = "[[check]]\nid = \"tls.a\"\n\n\
                   [[check]]\nid = \"tls.b\"\nchecks = \"\"\n\n\
                   [[check]]\nid = \"tls.c\"\nchecks = \"Verifies the chain.\"\n";
    assert_eq!(
        checks_text_problems(snippet),
        vec![
            "tls.a: checks is missing".to_string(),
            "tls.b: checks is empty".to_string(),
        ]
    );
}
