//! The check catalogue: one [`CheckSpec`] per check ID, loaded from TOML and validated.

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use netray_model::{CheckId, FixOwner, Protocol, Severity};

/// The longest `short` label, in characters.
const SHORT_MAX: usize = 8;

/// One catalogue entry.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct CheckSpec {
    pub id: CheckId,
    pub protocol: Protocol,
    pub title: String,
    pub finding: String,
    pub impact: String,
    pub fix_hint: String,
    pub fix_owner: FixOwner,
    pub severity: Severity,
    pub source: String,
    pub guide: String,
    pub short: String,
}

/// A validated set of [`CheckSpec`]s with unique IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalogue {
    entries: Vec<CheckSpec>,
}

/// Why a catalogue could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError(String);

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for CatalogueError {}

#[derive(Deserialize)]
struct RawFile {
    #[serde(default)]
    check: Vec<RawCheck>,
}

#[derive(Deserialize)]
struct RawCheck {
    id: Option<String>,
    protocol: Option<String>,
    title: Option<String>,
    finding: Option<String>,
    impact: Option<String>,
    fix_hint: Option<String>,
    fix_owner: Option<String>,
    severity: Option<String>,
    source: Option<String>,
    guide: Option<String>,
    short: Option<String>,
}

fn field(id: &str, name: &str, value: Option<String>) -> Result<String, CatalogueError> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(v),
        Some(_) => Err(CatalogueError(format!("check {id}: field {name} is empty"))),
        None => Err(CatalogueError(format!(
            "check {id}: field {name} is missing"
        ))),
    }
}

fn enum_field<T: for<'de> Deserialize<'de>>(
    id: &str,
    name: &str,
    value: Option<String>,
) -> Result<T, CatalogueError> {
    let v = field(id, name, value)?;
    T::deserialize(v.as_str().into_deserializer()).map_err(|e: serde::de::value::Error| {
        CatalogueError(format!(
            "check {id}: field {name} has unknown value {v:?}: {e}"
        ))
    })
}

impl RawCheck {
    fn validate(self, index: usize) -> Result<CheckSpec, CatalogueError> {
        let raw_id = self
            .id
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| CatalogueError(format!("check #{}: field id is missing", index + 1)))?;
        let id = CheckId::parse(&raw_id)
            .map_err(|e| CatalogueError(format!("check {raw_id}: invalid id: {e}")))?;
        let protocol: Protocol = enum_field(&raw_id, "protocol", self.protocol)?;
        if id.protocol() != protocol {
            return Err(CatalogueError(format!(
                "check {raw_id}: id prefix does not match protocol {protocol:?}"
            )));
        }
        let short = field(&raw_id, "short", self.short)?;
        if short.chars().count() > SHORT_MAX {
            return Err(CatalogueError(format!(
                "check {raw_id}: short {short:?} exceeds {SHORT_MAX} characters"
            )));
        }
        Ok(CheckSpec {
            title: field(&raw_id, "title", self.title)?,
            finding: field(&raw_id, "finding", self.finding)?,
            impact: field(&raw_id, "impact", self.impact)?,
            fix_hint: field(&raw_id, "fix_hint", self.fix_hint)?,
            fix_owner: enum_field(&raw_id, "fix_owner", self.fix_owner)?,
            severity: enum_field(&raw_id, "severity", self.severity)?,
            source: field(&raw_id, "source", self.source)?,
            guide: field(&raw_id, "guide", self.guide)?,
            short,
            protocol,
            id,
        })
    }
}

impl Catalogue {
    /// Parses `[[check]]` tables; refuses a missing or empty field, an unknown enum value, a
    /// `short` over 8 characters, an ID whose prefix is not its protocol, and a duplicate ID.
    pub fn from_toml(s: &str) -> Result<Catalogue, CatalogueError> {
        let raw: RawFile =
            toml::from_str(s).map_err(|e| CatalogueError(format!("catalogue: {e}")))?;
        let mut seen = HashSet::new();
        let mut entries = Vec::with_capacity(raw.check.len());
        for (i, check) in raw.check.into_iter().enumerate() {
            let spec = check.validate(i)?;
            if !seen.insert(spec.id.to_string()) {
                return Err(CatalogueError(format!("check {}: duplicate id", spec.id)));
            }
            entries.push(spec);
        }
        Ok(Catalogue { entries })
    }

    pub fn entries(&self) -> &[CheckSpec] {
        &self.entries
    }

    pub fn get(&self, id: &CheckId) -> Option<&CheckSpec> {
        self.entries.iter().find(|e| &e.id == id)
    }
}

/// The embedded catalogue, parsed once; panics when the embedded file is invalid.
pub fn catalogue() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        Catalogue::from_toml(include_str!("../catalogue/checks.toml"))
            .unwrap_or_else(|e| panic!("embedded catalogue is invalid: {e}"))
    })
}
