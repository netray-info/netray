//! Contract harness edges (spec v2-api requirements 2 and 5): parameters, format, items,
//! additionalProperties, untyped annotation-only schemas, oneOf growth.
use netray_web::api::contract::{compat_errors, untyped_schemas};
use serde_json::{Value, json};

fn base() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "paths": {"/api/v1/x": {"get": {
            "parameters": [
                {"name": "q", "in": "query", "required": false, "schema": {"type": "string"}},
                {"name": "mode", "in": "query", "schema": {"type": "string", "enum": ["a", "b"]}}
            ],
            "responses": {"200": {"description": "ok", "content": {"application/json":
                {"schema": {"$ref": "#/components/schemas/Res"}}}}}
        }}},
        "components": {"schemas": {
            "Res": {"type": "object", "properties": {
                "s": {"type": "string"},
                "list": {"type": "array", "items": {"type": "string"}},
                "map": {"type": "object", "additionalProperties": {"type": "string"}}
            }}
        }}
    })
}

#[test]
fn base_is_compatible_with_itself() {
    assert!(compat_errors(&base(), &base()).is_empty());
}

#[test]
fn parameter_changes_are_breaking_and_name_path_and_parameter() {
    type M = fn(&mut Value);
    let cases: Vec<(&str, &str, M)> = vec![
        ("parameter removed", "q", |d| {
            d["paths"]["/api/v1/x"]["get"]["parameters"]
                .as_array_mut()
                .unwrap()
                .remove(0);
        }),
        ("enum value removed", "mode", |d| {
            d["paths"]["/api/v1/x"]["get"]["parameters"][1]["schema"]["enum"] = json!(["a"]);
        }),
        ("schema type changed", "q", |d| {
            d["paths"]["/api/v1/x"]["get"]["parameters"][0]["schema"]["type"] = json!("integer");
        }),
        ("parameter becomes required", "q", |d| {
            d["paths"]["/api/v1/x"]["get"]["parameters"][0]["required"] = json!(true);
        }),
    ];
    for (label, name, mutate) in cases {
        let mut generated = base();
        mutate(&mut generated);
        let errs = compat_errors(&base(), &generated);
        assert!(
            errs.iter()
                .any(|e| e.contains("/api/v1/x") && e.contains(name)),
            "{label}: {errs:?}"
        );
    }
}

#[test]
fn new_optional_parameter_is_compatible() {
    let mut generated = base();
    generated["paths"]["/api/v1/x"]["get"]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "extra", "in": "query", "required": false, "schema": {"type": "string"}}));
    assert_eq!(compat_errors(&base(), &generated), Vec::<String>::new());
}

#[test]
fn schema_shape_narrowing_is_breaking() {
    type M = fn(&mut Value);
    let cases: Vec<(&str, M)> = vec![
        ("format gained", |d| {
            d["components"]["schemas"]["Res"]["properties"]["s"]["format"] = json!("ipv4");
        }),
        ("items lost", |d| {
            d["components"]["schemas"]["Res"]["properties"]["list"]
                .as_object_mut()
                .unwrap()
                .remove("items");
        }),
        ("additionalProperties removed", |d| {
            d["components"]["schemas"]["Res"]["properties"]["map"]
                .as_object_mut()
                .unwrap()
                .remove("additionalProperties");
        }),
    ];
    for (label, mutate) in cases {
        let mut generated = base();
        mutate(&mut generated);
        assert!(!compat_errors(&base(), &generated).is_empty(), "{label}");
    }
}

#[test]
fn untyped_schemas_flags_annotation_only_but_not_ref_or_oneof() {
    let doc = json!({"components": {"schemas": {
        "Holder": {"type": "object", "properties": {
            "note": {"description": "x"},
            "by_ref": {"$ref": "#/components/schemas/Other"},
            "either": {"oneOf": [{"type": "string"}, {"type": "integer"}]}
        }},
        "Other": {"type": "string"}
    }}});
    let flagged = untyped_schemas(&doc);
    assert!(
        flagged
            .iter()
            .any(|p| p.contains("Holder") && p.contains("note")),
        "{flagged:?}"
    );
    assert!(!flagged.iter().any(|p| p.contains("by_ref")), "{flagged:?}");
    assert!(!flagged.iter().any(|p| p.contains("either")), "{flagged:?}");
}

fn with_one_of(variants: &[&str]) -> Value {
    let mut doc = base();
    let list: Vec<Value> = variants
        .iter()
        .map(|v| json!({"$ref": format!("#/components/schemas/{v}")}))
        .collect();
    doc["components"]["schemas"]["Res"]["properties"]["u"] = json!({"oneOf": list});
    for v in ["A", "B", "C"] {
        doc["components"]["schemas"][v] =
            json!({"type": "object", "properties": {v.to_lowercase(): {"type": "string"}}});
    }
    doc
}

#[test]
fn one_of_variant_inserted_in_the_middle_is_compatible_and_removal_is_not() {
    let committed = with_one_of(&["A", "C"]);
    assert_eq!(
        compat_errors(&committed, &with_one_of(&["A", "B", "C"])),
        Vec::<String>::new()
    );
    assert!(!compat_errors(&committed, &with_one_of(&["A"])).is_empty());
}
