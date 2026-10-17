//! Spec v2-api requirement 2, criteria C2 and C22: the generated v1 document's shape, and the
//! untyped-schema check that guards it.
use netray_web::api::contract::untyped_schemas;
use netray_web::api::openapi;
use serde_json::{Value, json};

const ERROR_REF: &str = "#/components/schemas/ErrorResponse";

fn document() -> Value {
    serde_json::to_value(openapi()).expect("the document serialises")
}

#[test]
fn document_header_and_paths() {
    let doc = document();
    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["info"]["title"], "netray API");
    assert_eq!(doc["info"]["version"], env!("CARGO_PKG_VERSION"));

    let paths = doc["paths"].as_object().expect("paths is an object");
    for key in paths.keys() {
        assert!(key.starts_with("/api/v1/"), "path outside the mount: {key}");
        assert!(!key.contains("/docs"), "docs is not a document path: {key}");
    }
    assert!(!paths.contains_key("/api/v1/openapi.json"));
    for (path, method) in [
        ("/api/v1/catalogue", "get"),
        ("/api/v1/profiles/{n}", "get"),
        ("/api/v1/meta", "get"),
    ] {
        assert!(
            paths.get(path).is_some_and(|p| p.get(method).is_some()),
            "missing {method} {path}"
        );
    }
}

#[test]
fn every_error_response_references_error_response() {
    let doc = document();
    assert!(doc["components"]["schemas"]["ErrorResponse"].is_object());

    let mut checked = 0;
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            for (status, resp) in op["responses"].as_object().unwrap() {
                if status.starts_with('2') {
                    continue;
                }
                checked += 1;
                assert_eq!(
                    resp["content"]["application/json"]["schema"]["$ref"], ERROR_REF,
                    "{method} {path} {status} does not reference ErrorResponse"
                );
            }
        }
    }
    assert!(checked > 0, "no error response in the document");
}

#[test]
fn repository_document_has_no_untyped_schema() {
    assert_eq!(untyped_schemas(&document()), Vec::<String>::new());
}

#[test]
fn untyped_schemas_names_bare_object_by_pointer() {
    let doc = json!({"components": {"schemas": {"Loose": {"type": "object"}}}});
    let found = untyped_schemas(&doc);
    assert!(
        found.iter().any(|p| p.contains("Loose")),
        "expected a pointer naming Loose, got {found:?}"
    );
}

#[test]
fn untyped_schemas_names_empty_nested_schema() {
    let doc = json!({"components": {"schemas": {"Outer": {
        "type": "object",
        "properties": {"inner": {}}
    }}}});
    let found = untyped_schemas(&doc);
    assert!(
        found
            .iter()
            .any(|p| p.contains("Outer") && p.contains("inner")),
        "expected a pointer naming Outer and inner, got {found:?}"
    );
}

#[test]
fn untyped_schemas_accepts_typed_additional_properties() {
    let doc = json!({"components": {"schemas": {"Map": {
        "type": "object",
        "additionalProperties": {"type": "integer"}
    }}}});
    assert_eq!(untyped_schemas(&doc), Vec::<String>::new());
}
