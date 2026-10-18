//! Composing the v1 document from parts that define the same component name differently: tlsight
//! and spectra both have an `InspectResponse` and a `CheckStatus`, and utoipa keeps one silently.
use std::collections::BTreeMap;

use serde_json::Value;
use utoipa::openapi::OpenApi;

const SCHEMAS: &str = "/components/schemas";

fn to_value(doc: &OpenApi) -> Value {
    serde_json::to_value(doc).expect("the document serialises")
}

fn schemas(doc: &Value) -> BTreeMap<String, Value> {
    doc.pointer(SCHEMAS)
        .and_then(Value::as_object)
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

fn rewrite_refs(node: &mut Value, from: &str, to: &str) {
    match node {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                match value {
                    Value::String(s)
                        if key == "$ref" && s == &format!("#/components/schemas/{from}") =>
                    {
                        *s = format!("#/components/schemas/{to}");
                    }
                    _ => rewrite_refs(value, from, to),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| rewrite_refs(v, from, to)),
        _ => {}
    }
}

fn rename(part: &mut Value, from: &str, to: &str) {
    let map = part
        .pointer_mut(SCHEMAS)
        .and_then(Value::as_object_mut)
        .expect("a part defining a schema has components.schemas");
    let definition = map.remove(from).expect("the part defines the name");
    map.insert(to.to_string(), definition);
    rewrite_refs(part, from, to);
}

/// `base` plus every part. A component name that two of base and the parts define differently is
/// renamed to `<prefix><Name>` in each part defining it; base names never change. Panics when a
/// renamed name is taken.
pub(super) fn compose(mut base: OpenApi, parts: Vec<(&'static str, OpenApi)>) -> OpenApi {
    let base_schemas = schemas(&to_value(&base));
    let mut values: Vec<(&'static str, Value)> = parts
        .iter()
        .map(|(prefix, doc)| (*prefix, to_value(doc)))
        .collect();
    loop {
        let mut definitions: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for (name, def) in &base_schemas {
            definitions
                .entry(name.clone())
                .or_default()
                .push(def.clone());
        }
        for (_, value) in &values {
            for (name, def) in schemas(value) {
                definitions.entry(name).or_default().push(def);
            }
        }
        let colliding: Vec<String> = definitions
            .into_iter()
            .filter(|(_, defs)| defs.iter().any(|d| d != &defs[0]))
            .map(|(name, _)| name)
            .collect();
        if colliding.is_empty() {
            break;
        }
        for name in colliding {
            for index in 0..values.len() {
                let prefix = values[index].0;
                if !schemas(&values[index].1).contains_key(&name) {
                    continue;
                }
                let renamed = format!("{prefix}{name}");
                let taken = base_schemas.contains_key(&renamed)
                    || values
                        .iter()
                        .any(|(_, v)| schemas(v).contains_key(&renamed));
                assert!(
                    !taken,
                    "cannot rename component `{name}`: `{renamed}` is taken"
                );
                rename(&mut values[index].1, &name, &renamed);
            }
        }
    }
    for (_, value) in values {
        let part: OpenApi =
            serde_json::from_value(value).expect("the renamed part is still a document");
        base.merge(part);
    }
    base
}
