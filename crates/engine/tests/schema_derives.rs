//! v2-api R3 / C3: the model and engine types derive `ToSchema`; `CheckSpec` and `Profile` serialise.
use netray_engine::catalogue::CheckSpec;
use netray_engine::profile::{Profile, Profiles, SectionWeights};
use netray_model::diff::{
    BlockChange, BlockChangeKind, ChangeClass, CheckChange, Diff, GradeChange,
};
use netray_model::evidence::{Block, BlockId, Node, Record, Shape, Value};
use netray_model::run::{Run, RunCheck, RunSection, SectionKind};
use netray_model::{CheckId, FixOwner, Grade, Protocol, Severity, Status};
use serde_json::json;
use utoipa::{PartialSchema, ToSchema};

fn schema_of<T: ToSchema>() -> serde_json::Value {
    serde_json::to_value(T::schema()).expect("schema serialises")
}

fn assert_schema<T: ToSchema>(name: &str) {
    let v = schema_of::<T>();
    assert!(v.is_object(), "{name}: schema is not an object: {v}");
}

#[test]
fn every_listed_type_implements_to_schema() {
    assert_schema::<Run>("Run");
    assert_schema::<RunSection>("RunSection");
    assert_schema::<SectionKind>("SectionKind");
    assert_schema::<RunCheck>("RunCheck");
    assert_schema::<Status>("Status");
    assert_schema::<Grade>("Grade");
    assert_schema::<Protocol>("Protocol");
    assert_schema::<Severity>("Severity");
    assert_schema::<FixOwner>("FixOwner");
    assert_schema::<Block>("Block");
    assert_schema::<Shape>("Shape");
    assert_schema::<Value>("Value");
    assert_schema::<Record>("Record");
    assert_schema::<Node>("Node");
    assert_schema::<Diff>("Diff");
    assert_schema::<CheckChange>("CheckChange");
    assert_schema::<ChangeClass>("ChangeClass");
    assert_schema::<BlockChange>("BlockChange");
    assert_schema::<BlockChangeKind>("BlockChangeKind");
    assert_schema::<GradeChange>("GradeChange");
    assert_schema::<CheckSpec>("CheckSpec");
    assert_schema::<Profile>("Profile");
    assert_schema::<SectionWeights>("SectionWeights");
}

#[test]
fn check_id_and_block_id_schemas_are_strings() {
    assert_eq!(schema_of::<CheckId>()["type"], json!("string"));
    assert_eq!(schema_of::<BlockId>()["type"], json!("string"));
}

#[test]
fn check_spec_serialises_exactly_its_eleven_fields() {
    let spec = &netray_engine::catalogue().entries()[0];
    let v = serde_json::to_value(spec).unwrap();
    let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "fix_hint",
            "fix_owner",
            "finding",
            "guide",
            "id",
            "impact",
            "protocol",
            "severity",
            "short",
            "source",
            "title"
        ]
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );
}

#[test]
fn current_profile_serialises_name_version_sections_thresholds() {
    let v = serde_json::to_value(Profiles::current()).unwrap();
    let obj = v.as_object().unwrap();
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["name", "sections", "thresholds", "version"]);
    assert_eq!(v["version"], json!(3));
    let thresholds = v["thresholds"].as_array().unwrap();
    assert_eq!(thresholds[0]["grade"], json!("A+"));
    assert!(thresholds[0]["min"].is_u64());
    let mins: Vec<u64> = thresholds
        .iter()
        .map(|t| t["min"].as_u64().unwrap())
        .collect();
    assert!(
        mins.windows(2).all(|w| w[0] >= w[1]),
        "highest first: {mins:?}"
    );
    let dns = &v["sections"]["dns"];
    assert!(dns["weight"].is_u64());
    assert!(dns["checks"].is_object());
    assert!(dns["hard_fail"].is_array());
}
