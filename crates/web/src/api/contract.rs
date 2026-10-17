//! The contract harness (spec v2-api requirement 5): compatibility of a generated OpenAPI
//! document against the committed one, and the untyped-schema walk (requirement 2).
use std::collections::BTreeSet;

use serde_json::Value;

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];
const SCHEMA_PREFIX: &str = "#/components/schemas/";

/// Which side of the wire a schema sits on; decides the `required` rule.
#[derive(Clone, Copy, Default)]
struct Direction {
    request: bool,
    response: bool,
}

/// Compatibility errors of `generated` against `committed`; `info.version` is ignored.
pub fn compat_errors(committed: &Value, generated: &Value) -> Vec<String> {
    let mut errs = Vec::new();
    let (requests, responses) = directions(committed);

    let no_paths = serde_json::Map::new();
    let committed_paths = committed["paths"].as_object().unwrap_or(&no_paths);
    for (path, item) in committed_paths {
        let Some(gen_item) = generated["paths"].get(path) else {
            errs.push(format!("{path}: path removed"));
            continue;
        };
        for method in METHODS.iter().filter(|m| item.get(**m).is_some()) {
            let Some(gen_op) = gen_item.get(*method) else {
                errs.push(format!("{path} {method}: method removed"));
                continue;
            };
            let op = &item[*method];
            compare_parameters(&format!("{path} {method}"), op, gen_op, &mut errs);
            if let Some(statuses) = op["responses"].as_object() {
                for (status, resp) in statuses {
                    let loc = format!("{path} {method} {status}");
                    let Some(gen_resp) = gen_op["responses"].get(status) else {
                        errs.push(format!("{loc}: response status removed"));
                        continue;
                    };
                    compare_media(
                        &loc,
                        resp,
                        gen_resp,
                        Direction {
                            response: true,
                            ..Default::default()
                        },
                        &mut errs,
                    );
                }
            }
            if op.get("requestBody").is_some() {
                let loc = format!("{path} {method} request");
                compare_media(
                    &loc,
                    &op["requestBody"],
                    &gen_op["requestBody"],
                    Direction {
                        request: true,
                        ..Default::default()
                    },
                    &mut errs,
                );
            }
        }
    }

    if let Some(schemas) = committed["components"]["schemas"].as_object() {
        for (name, schema) in schemas {
            let loc = format!("component `{name}`");
            let Some(gen_schema) = generated["components"]["schemas"].get(name) else {
                errs.push(format!("{loc}: component removed"));
                continue;
            };
            let dir = Direction {
                request: requests.contains(name),
                response: responses.contains(name),
            };
            compare_schema(&loc, schema, gen_schema, dir, &mut errs);
        }
    }
    errs
}

fn parameter_key(param: &Value) -> Option<(&str, &str)> {
    Some((param["name"].as_str()?, param["in"].as_str()?))
}

fn parameters(op: &Value) -> Vec<&Value> {
    op["parameters"].as_array().into_iter().flatten().collect()
}

fn is_required(param: &Value) -> bool {
    param["required"].as_bool().unwrap_or(false)
}

/// Operation parameters, matched by name and location.
fn compare_parameters(loc: &str, committed: &Value, generated: &Value, errs: &mut Vec<String>) {
    let gen_params = parameters(generated);
    let find = |key| {
        gen_params
            .iter()
            .copied()
            .find(|p| parameter_key(p) == Some(key))
    };
    for param in parameters(committed) {
        let Some((name, place)) = parameter_key(param) else {
            continue;
        };
        let ploc = format!("{loc} parameter `{name}` ({place})");
        let Some(gen_param) = find((name, place)) else {
            errs.push(format!("{ploc}: parameter removed"));
            continue;
        };
        if is_required(gen_param) && !is_required(param) {
            errs.push(format!("{ploc}: parameter became required"));
        }
        match (param.get("schema"), gen_param.get("schema")) {
            (Some(schema), Some(gen_schema)) => compare_schema(
                &ploc,
                schema,
                gen_schema,
                Direction {
                    request: true,
                    ..Default::default()
                },
                errs,
            ),
            (Some(_), None) => errs.push(format!("{ploc}: schema removed")),
            _ => {}
        }
    }
    let committed_params = parameters(committed);
    for gen_param in gen_params {
        let Some((name, place)) = parameter_key(gen_param) else {
            continue;
        };
        let known = committed_params
            .iter()
            .any(|p| parameter_key(p) == Some((name, place)));
        if !known && is_required(gen_param) {
            errs.push(format!(
                "{loc} parameter `{name}` ({place}): new parameter is required"
            ));
        }
    }
}

fn compare_media(
    loc: &str,
    committed: &Value,
    generated: &Value,
    dir: Direction,
    errs: &mut Vec<String>,
) {
    let Some(content) = committed["content"].as_object() else {
        return;
    };
    for (media, body) in content {
        let Some(gen_body) = generated["content"].get(media) else {
            errs.push(format!("{loc}: media type `{media}` removed"));
            continue;
        };
        if let Some(schema) = body.get("schema") {
            let loc = format!("{loc} {media}");
            match gen_body.get("schema") {
                Some(gen_schema) => compare_schema(&loc, schema, gen_schema, dir, errs),
                None => errs.push(format!("{loc}: schema removed")),
            }
        }
    }
}

/// Compare one schema node against its generated counterpart, without following `$ref`s: a
/// reference is compared by target, its target by its own component entry.
fn compare_schema(
    loc: &str,
    committed: &Value,
    generated: &Value,
    dir: Direction,
    errs: &mut Vec<String>,
) {
    let (Some(c), Some(g)) = (committed.as_object(), generated.as_object()) else {
        return;
    };
    if let Some(target) = c.get("$ref") {
        if g.get("$ref") != Some(target) {
            errs.push(format!(
                "{loc}: $ref target changed from {target} to {}",
                show(g.get("$ref"))
            ));
        }
        return;
    }
    for key in ["type", "format"] {
        if c.get(key) != g.get(key) {
            errs.push(format!(
                "{loc}: {key} changed from {} to {}",
                show(c.get(key)),
                show(g.get(key))
            ));
        }
    }
    if let Some(values) = c.get("enum").and_then(Value::as_array) {
        let have = g.get("enum").and_then(Value::as_array);
        for v in values
            .iter()
            .filter(|v| !have.is_some_and(|h| h.contains(v)))
        {
            errs.push(format!("{loc}: enum value {v} removed"));
        }
    }
    if let Some(props) = c.get("properties").and_then(Value::as_object) {
        for (name, prop) in props {
            match g.get("properties").and_then(|p| p.get(name)) {
                Some(gen_prop) => {
                    compare_schema(
                        &format!("{loc} property `{name}`"),
                        prop,
                        gen_prop,
                        dir,
                        errs,
                    );
                }
                None => errs.push(format!("{loc}: property `{name}` removed")),
            }
        }
    }
    if dir.response {
        for name in strings(c.get("required")).difference(&strings(g.get("required"))) {
            errs.push(format!("{loc}: property `{name}` dropped from required"));
        }
    }
    if dir.request {
        for name in strings(g.get("required")).difference(&strings(c.get("required"))) {
            errs.push(format!("{loc}: property `{name}` added to required"));
        }
    }
    for key in ["items", "additionalProperties"] {
        match (c.get(key), g.get(key)) {
            (None, None) => {}
            (Some(cv), Some(gv)) if cv.is_object() && gv.is_object() => {
                compare_schema(&format!("{loc} {key}"), cv, gv, dir, errs);
            }
            (cv, gv) if cv == gv => {}
            (cv, gv) => errs.push(format!(
                "{loc}: {key} changed from {} to {}",
                show(cv),
                show(gv)
            )),
        }
    }
    if let (Some(cv), Some(gv)) = (c.get("not"), g.get("not")) {
        compare_schema(&format!("{loc} not"), cv, gv, dir, errs);
    }
    for key in ["oneOf", "anyOf"] {
        let Some(variants) = c.get(key).and_then(Value::as_array) else {
            continue;
        };
        let gen_variants = g
            .get(key)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        for (i, variant) in variants.iter().enumerate() {
            let kept = gen_variants.iter().any(|gen_variant| {
                let mut inner = Vec::new();
                compare_schema(loc, variant, gen_variant, dir, &mut inner);
                inner.is_empty()
            });
            if !kept {
                errs.push(format!(
                    "{loc}: {key} variant {i} ({}) removed or changed",
                    variant
                        .get("$ref")
                        .map_or_else(|| variant.to_string(), Value::to_string)
                ));
            }
        }
    }
    for key in ["prefixItems", "allOf"] {
        let Some(variants) = c.get(key).and_then(Value::as_array) else {
            continue;
        };
        let gen_variants = g.get(key).and_then(Value::as_array);
        for (i, variant) in variants.iter().enumerate() {
            match gen_variants.and_then(|v| v.get(i)) {
                Some(gen_variant) => {
                    compare_schema(
                        &format!("{loc} {key}[{i}]"),
                        variant,
                        gen_variant,
                        dir,
                        errs,
                    );
                }
                None => errs.push(format!("{loc}: {key}[{i}] removed")),
            }
        }
    }
}

fn show(v: Option<&Value>) -> String {
    v.map_or_else(|| "nothing".into(), Value::to_string)
}

fn strings(v: Option<&Value>) -> BTreeSet<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

/// The component names reachable from request bodies and from responses of `doc`.
fn directions(doc: &Value) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut requests, mut responses) = (BTreeSet::new(), BTreeSet::new());
    let Some(paths) = doc["paths"].as_object() else {
        return (requests, responses);
    };
    for item in paths.values() {
        for method in METHODS.iter().filter_map(|m| item.get(*m)) {
            if let Some(body) = method.get("requestBody") {
                collect_refs(doc, body, &mut requests);
            }
            if let Some(resps) = method.get("responses") {
                collect_refs(doc, resps, &mut responses);
            }
        }
    }
    (requests, responses)
}

/// Add every component named by a `$ref` under `node`, transitively.
fn collect_refs(doc: &Value, node: &Value, seen: &mut BTreeSet<String>) {
    match node {
        Value::Object(map) => {
            for (key, value) in map {
                match (key.as_str(), value.as_str()) {
                    ("$ref", Some(target)) => {
                        if let Some(name) = target.strip_prefix(SCHEMA_PREFIX)
                            && seen.insert(name.to_owned())
                            && let Some(schema) = doc["components"]["schemas"].get(name)
                        {
                            collect_refs(doc, schema, seen);
                        }
                    }
                    _ => collect_refs(doc, value, seen),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect_refs(doc, v, seen)),
        _ => {}
    }
}

/// Every untyped schema of `doc`, each named by its JSON pointer: a schema with no `type`,
/// `$ref`, `oneOf`, `anyOf`, `allOf`, `enum`, `const` or `not` (empty or annotation-only), or
/// `type: object` with neither `properties` nor a typed `additionalProperties`.
pub fn untyped_schemas(doc: &Value) -> Vec<String> {
    let mut found = Vec::new();
    if let Some(schemas) = doc["components"]["schemas"].as_object() {
        for (name, schema) in schemas {
            walk(
                &format!("#/components/schemas/{}", escape(name)),
                schema,
                &mut found,
            );
        }
    }
    if let Some(paths) = doc["paths"].as_object() {
        for (path, item) in paths {
            for method in METHODS.iter().filter(|m| item.get(**m).is_some()) {
                let op = format!("#/paths/{}/{method}", escape(path));
                let op_value = &item[*method];
                for (i, param) in op_value["parameters"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if let Some(schema) = param.get("schema") {
                        walk(&format!("{op}/parameters/{i}/schema"), schema, &mut found);
                    }
                }
                if let Some(body) = op_value.get("requestBody") {
                    walk_content(&format!("{op}/requestBody"), body, &mut found);
                }
                for (status, resp) in op_value["responses"].as_object().into_iter().flatten() {
                    walk_content(&format!("{op}/responses/{status}"), resp, &mut found);
                }
            }
        }
    }
    found
}

fn walk_content(ptr: &str, node: &Value, found: &mut Vec<String>) {
    for (media, body) in node["content"].as_object().into_iter().flatten() {
        if let Some(schema) = body.get("schema") {
            walk(
                &format!("{ptr}/content/{}/schema", escape(media)),
                schema,
                found,
            );
        }
    }
}

fn walk(ptr: &str, schema: &Value, found: &mut Vec<String>) {
    let Some(map) = schema.as_object() else {
        return;
    };
    let is_object = match map.get("type") {
        Some(Value::String(t)) => t == "object",
        Some(Value::Array(ts)) => ts.iter().any(|t| t == "object"),
        _ => false,
    };
    let typed_additional = map
        .get("additionalProperties")
        .is_some_and(Value::is_object)
        && !map["additionalProperties"]
            .as_object()
            .is_some_and(|m| m.is_empty());
    let annotation_only = ![
        "type", "$ref", "oneOf", "anyOf", "allOf", "enum", "const", "not",
    ]
    .iter()
    .any(|k| map.contains_key(*k));
    if annotation_only || (is_object && !map.contains_key("properties") && !typed_additional) {
        found.push(ptr.to_owned());
    }
    for (name, prop) in map
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        walk(&format!("{ptr}/properties/{}", escape(name)), prop, found);
    }
    for key in ["items", "additionalProperties", "not"] {
        if let Some(inner) = map.get(key) {
            walk(&format!("{ptr}/{key}"), inner, found);
        }
    }
    for key in ["prefixItems", "oneOf", "anyOf", "allOf"] {
        for (i, inner) in map
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            walk(&format!("{ptr}/{key}/{i}"), inner, found);
        }
    }
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
