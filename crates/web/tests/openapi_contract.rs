//! Spec v2-api requirement 5, criteria C5 and C17-C21: the committed
//! `tests/fixtures/contracts/api-v1.openapi.json` is the v1 contract. The generated document
//! may only grow compatibly; `UPDATE_GOLDEN=1` rewrites the fixture only for such growth.
use std::path::{Path, PathBuf};

use netray_web::api::contract::compat_errors;
use netray_web::api::openapi;
use serde_json::{Value, json};

const UPDATE_CMD: &str = "UPDATE_GOLDEN=1 cargo test -p netray-web --test openapi_contract";

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/contracts/api-v1.openapi.json")
}

fn update_requested() -> bool {
    std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1")
}

fn without_version(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(info) = v.get_mut("info").and_then(Value::as_object_mut) {
        info.remove("version");
    }
    v
}

/// The update decision: a rewrite is allowed only when the generated document is compatible.
fn golden_update(committed: &Value, generated: &Value) -> Result<(), Vec<String>> {
    let errs = compat_errors(committed, generated);
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn write_fixture(path: &Path, doc: &Value) {
    let mut text = serde_json::to_string_pretty(doc).unwrap();
    text.push('\n');
    std::fs::write(path, text).unwrap();
}

/// Decide, then write; the file is untouched on Err.
fn update_file(path: &Path, committed: &Value, generated: &Value) -> Result<(), Vec<String>> {
    golden_update(committed, generated)?;
    write_fixture(path, generated);
    Ok(())
}

#[test]
fn contract() {
    let generated = serde_json::to_value(openapi()).unwrap();
    assert_eq!(generated["openapi"], "3.1.0", "C17: OpenAPI version");
    let path = fixture_path();

    if !path.exists() {
        assert!(
            update_requested(),
            "{} is missing; bootstrap with {UPDATE_CMD}",
            path.display()
        );
        write_fixture(&path, &generated);
        return;
    }

    let committed: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    if update_requested() {
        if let Err(errs) = update_file(&path, &committed, &generated) {
            panic!("incompatible change, fixture left untouched:\n  {}", errs.join("\n  "));
        }
        return;
    }

    let errs = compat_errors(&committed, &generated);
    assert!(
        errs.is_empty(),
        "generated API document breaks the committed v1 contract:\n  {}",
        errs.join("\n  ")
    );
    assert_eq!(
        without_version(&committed),
        without_version(&generated),
        "generated API document differs from the committed contract (compatible growth); \
         rewrite it with {UPDATE_CMD}"
    );
}

fn base() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": {"title": "t", "version": "1"},
        "paths": {
            "/api/v1/x": {"get": {"responses": {
                "200": {"description": "ok", "content": {"application/json":
                    {"schema": {"$ref": "#/components/schemas/Res"}}}},
                "404": {"description": "nf", "content": {"application/json":
                    {"schema": {"$ref": "#/components/schemas/Other"}}}}
            }}},
            "/api/v1/y": {
                "get": {"responses": {"200": {"description": "ok"}}},
                "post": {
                    "requestBody": {"content": {"application/json":
                        {"schema": {"$ref": "#/components/schemas/Req"}}}},
                    "responses": {"200": {"description": "ok"}}
                }
            }
        },
        "components": {"schemas": {
            "Res": {"type": "object",
                "properties": {
                    "a": {"type": "integer", "format": "int32"},
                    "b": {"type": "string", "enum": ["one", "two"]},
                    "c": {"type": "array", "items": {"type": "string"}},
                    "d": {"$ref": "#/components/schemas/Other"}
                },
                "required": ["a", "b"]},
            "Other": {"type": "object", "properties": {"m": {"type": "string"}}},
            "Extra": {"type": "object", "properties": {"n": {"type": "string"}}},
            "Req": {"type": "object",
                "properties": {"p": {"type": "string"}, "q": {"type": "string"}},
                "required": ["p"]}
        }}
    })
}

#[test]
fn synthetic_base_is_compatible_with_itself() {
    assert_eq!(compat_errors(&base(), &base()), Vec::<String>::new());
}

#[test]
fn deleted_response_property_names_path_and_property_and_keeps_fixture() {
    // C18
    let committed = base();
    let mut generated = base();
    generated["components"]["schemas"]["Res"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("b");

    let errs = compat_errors(&committed, &generated);
    assert!(
        errs.iter()
            .any(|e| (e.contains("/api/v1/x") || e.contains("Res")) && e.contains('b')),
        "errors must name the path or component and the property: {errs:?}"
    );

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("api.json");
    write_fixture(&file, &committed);
    let before = std::fs::read_to_string(&file).unwrap();
    assert!(update_file(&file, &committed, &generated).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn new_optional_response_property_is_compatible_and_update_writes() {
    // C20
    let committed = base();
    let mut generated = base();
    generated["components"]["schemas"]["Res"]["properties"]["fresh"] = json!({"type": "string"});

    assert_eq!(compat_errors(&committed, &generated), Vec::<String>::new());
    assert_ne!(without_version(&committed), without_version(&generated));

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("api.json");
    write_fixture(&file, &committed);
    update_file(&file, &committed, &generated).unwrap();
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(std::fs::read_to_string(&file).unwrap().ends_with("}\n"));
    assert_eq!(compat_errors(&written, &generated), Vec::<String>::new());
    assert_eq!(without_version(&written), without_version(&generated));
}

#[test]
fn breaking_changes_are_refused() {
    // C19, C21 and requirement 5's list.
    type Mutation = fn(&mut Value);
    let cases: Vec<(&str, Mutation)> = vec![
        ("property type integer to string (C19)", |d| {
            d["components"]["schemas"]["Res"]["properties"]["a"]["type"] = json!("string");
        }),
        ("request property becomes required (C21)", |d| {
            d["components"]["schemas"]["Req"]["required"] = json!(["p", "q"]);
        }),
        ("path removed", |d| {
            d["paths"].as_object_mut().unwrap().remove("/api/v1/y");
        }),
        ("method removed", |d| {
            d["paths"]["/api/v1/y"].as_object_mut().unwrap().remove("post");
        }),
        ("response status removed", |d| {
            d["paths"]["/api/v1/x"]["get"]["responses"]
                .as_object_mut()
                .unwrap()
                .remove("404");
        }),
        ("enum value removed", |d| {
            d["components"]["schemas"]["Res"]["properties"]["b"]["enum"] = json!(["one"]);
        }),
        ("component removed", |d| {
            d["components"]["schemas"].as_object_mut().unwrap().remove("Extra");
        }),
        ("format changed", |d| {
            d["components"]["schemas"]["Res"]["properties"]["a"]["format"] = json!("int64");
        }),
        ("items changed", |d| {
            d["components"]["schemas"]["Res"]["properties"]["c"]["items"] = json!({"type": "integer"});
        }),
        ("$ref target changed", |d| {
            d["components"]["schemas"]["Res"]["properties"]["d"]["$ref"] =
                json!("#/components/schemas/Extra");
        }),
        ("property dropped from required", |d| {
            d["components"]["schemas"]["Res"]["required"] = json!(["b"]);
        }),
    ];
    for (name, mutate) in cases {
        let committed = base();
        let mut generated = base();
        mutate(&mut generated);
        let errs = compat_errors(&committed, &generated);
        assert!(!errs.is_empty(), "{name}: expected a compatibility error");
        assert!(golden_update(&committed, &generated).is_err(), "{name}: update must be refused");
    }
}
