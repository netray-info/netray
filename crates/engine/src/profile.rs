//! Scoring profiles: the per-check and per-section weights, hard-fail checks and grade
//! thresholds. Released profiles are embedded and pinned by `profiles/lock.toml`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};

use netray_model::{CheckId, Grade, Protocol};

const V3: &str = include_str!("../profiles/v3.toml");
const V4: &str = include_str!("../profiles/v4.toml");
const V5: &str = include_str!("../profiles/v5.toml");
const LOCK: &str = include_str!("../profiles/lock.toml");

/// Why a profile or its lock entry is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileError(String);

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileError {}

/// One section's weight, its weighted checks and the checks whose failure forces `F`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct SectionWeights {
    pub weight: u32,
    pub checks: BTreeMap<CheckId, u32>,
    pub hard_fail: Vec<CheckId>,
}

/// A parsed scoring profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct Profile {
    pub name: String,
    pub version: u32,
    pub sections: BTreeMap<Protocol, SectionWeights>,
    /// Grade thresholds, highest first.
    #[serde(serialize_with = "serialize_thresholds")]
    #[schema(value_type = Vec<Threshold>)]
    pub thresholds: Vec<(Grade, u32)>,
}

/// One grade threshold as served: the lowest score that earns `grade`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct Threshold {
    pub grade: Grade,
    pub min: u32,
}

fn serialize_thresholds<S: Serializer>(
    thresholds: &[(Grade, u32)],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(
        thresholds
            .iter()
            .map(|&(grade, min)| Threshold { grade, min }),
    )
}

#[derive(Deserialize)]
struct RawProfile {
    meta: RawMeta,
    sections: BTreeMap<Protocol, RawSection>,
    thresholds: BTreeMap<String, u32>,
}

#[derive(Deserialize)]
struct RawMeta {
    name: String,
    version: u32,
}

#[derive(Deserialize)]
struct RawSection {
    weight: u32,
    #[serde(default)]
    hard_fail: Vec<String>,
    checks: BTreeMap<String, u32>,
}

fn err(msg: impl Into<String>) -> ProfileError {
    ProfileError(msg.into())
}

fn parse_grade(s: &str) -> Result<Grade, ProfileError> {
    match s {
        "A+" => Ok(Grade::APlus),
        "A" => Ok(Grade::A),
        "B" => Ok(Grade::B),
        "C" => Ok(Grade::C),
        "D" => Ok(Grade::D),
        "F" => Ok(Grade::F),
        _ => Err(err(format!("unknown grade {s:?} in thresholds"))),
    }
}

impl Profile {
    /// Parses a profile; refuses a weight 0, a check name that is no valid ID and a hard-fail
    /// name that is not among the section's checks.
    pub fn from_toml(toml: &str) -> Result<Profile, ProfileError> {
        let raw: RawProfile = toml::from_str(toml).map_err(|e| err(e.to_string()))?;
        let mut sections = BTreeMap::new();
        for (protocol, section) in raw.sections {
            let prefix = protocol.as_str();
            if section.weight == 0 {
                return Err(err(format!("section {prefix}: weight 0")));
            }
            let id = |name: &str| {
                CheckId::parse(&format!("{prefix}.{name}")).map_err(|e| err(e.to_string()))
            };
            let mut checks = BTreeMap::new();
            for (name, weight) in &section.checks {
                if *weight == 0 {
                    return Err(err(format!("check {prefix}.{name}: weight 0")));
                }
                checks.insert(id(name)?, *weight);
            }
            let mut hard_fail = Vec::new();
            for name in &section.hard_fail {
                let hf = id(name)?;
                if !checks.contains_key(&hf) {
                    return Err(err(format!(
                        "hard_fail {hf} is not among the section's checks"
                    )));
                }
                hard_fail.push(hf);
            }
            sections.insert(
                protocol,
                SectionWeights {
                    weight: section.weight,
                    checks,
                    hard_fail,
                },
            );
        }
        let mut thresholds = raw
            .thresholds
            .iter()
            .map(|(g, t)| Ok((parse_grade(g)?, *t)))
            .collect::<Result<Vec<_>, ProfileError>>()?;
        thresholds.sort_by_key(|t| std::cmp::Reverse(t.1));
        Ok(Profile {
            name: raw.meta.name,
            version: raw.meta.version,
            sections,
            thresholds,
        })
    }
}

/// The embedded, released profiles.
pub struct Profiles;

impl Profiles {
    fn all() -> &'static [Profile] {
        static ALL: OnceLock<Vec<Profile>> = OnceLock::new();
        ALL.get_or_init(|| {
            vec![
                Profile::from_toml(V3).expect("embedded v3.toml parses"),
                Profile::from_toml(V4).expect("embedded v4.toml parses"),
                Profile::from_toml(V5).expect("embedded v5.toml parses"),
            ]
        })
    }

    pub fn get(version: u32) -> Option<&'static Profile> {
        Self::all().iter().find(|p| p.version == version)
    }

    pub fn versions() -> Vec<u32> {
        Self::all().iter().map(|p| p.version).collect()
    }

    /// The highest embedded version.
    pub fn current() -> &'static Profile {
        Self::all()
            .iter()
            .max_by_key(|p| p.version)
            .expect("a profile is embedded")
    }
}

/// Refuses `toml` when its SHA-256 differs from the lock entry of `version` or the version has
/// none.
pub fn check_lock(version: u32, toml: &str) -> Result<(), ProfileError> {
    let lock: BTreeMap<String, String> =
        toml::from_str(LOCK).map_err(|e| err(format!("lock.toml: {e}")))?;
    let want = lock
        .get(&format!("v{version}"))
        .ok_or_else(|| err(format!("no lock entry for profile v{version}")))?;
    let got = format!("{:x}", Sha256::digest(toml.as_bytes()));
    if &got != want {
        return Err(err(format!(
            "profile v{version} hash {got} differs from the lock {want}"
        )));
    }
    Ok(())
}
