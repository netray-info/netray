use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionProfile {
    pub weight: u32,
    #[serde(default)]
    pub hard_fail: Vec<String>,
    #[serde(default)]
    pub checks: HashMap<String, u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScoringProfile {
    pub meta: ProfileMeta,
    /// Section name → section config. Key must match Backend::section().
    pub sections: HashMap<String, SectionProfile>,
    /// Grade → minimum percentage (inclusive). Resolved by value descending.
    pub thresholds: BTreeMap<String, u32>,
}

#[derive(Debug)]
pub enum ScoringProfileError {
    Toml(toml::de::Error),
    /// hard_fail entry "check_name" in section "section_name" is not in checks map.
    UnknownHardFail {
        section: String,
        check: String,
    },
}

impl fmt::Display for ScoringProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(e) => write!(f, "TOML parse error: {e}"),
            Self::UnknownHardFail { section, check } => {
                write!(
                    f,
                    "hard_fail entry \"{check}\" in section \"{section}\" is not in checks map"
                )
            }
        }
    }
}

impl std::error::Error for ScoringProfileError {}

impl ScoringProfile {
    /// Parse from TOML. Returns Err if TOML is invalid or any hard_fail entry
    /// names a check not present in the same section's checks map.
    pub fn from_toml(s: &str) -> Result<Self, ScoringProfileError> {
        let profile: Self = toml::from_str(s).map_err(ScoringProfileError::Toml)?;

        // Validate hard_fail entries.
        for (section_name, section) in &profile.sections {
            for check_name in &section.hard_fail {
                if !section.checks.contains_key(check_name) {
                    return Err(ScoringProfileError::UnknownHardFail {
                        section: section_name.clone(),
                        check: check_name.clone(),
                    });
                }
            }
        }

        Ok(profile)
    }

    pub fn embedded_default() -> Self {
        Self::from_toml(include_str!("../../profiles/default.toml"))
            .expect("embedded default profile is valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_hard_fail_entry_returns_error() {
        let toml = r#"
[meta]
name = "bad"
version = 2

[sections.dns]
weight = 100
hard_fail = ["nonexistent_check"]

[sections.dns.checks]
spf = 10

[thresholds]
"A+" = 97
"F" = 0
"#;
        let result = ScoringProfile::from_toml(toml);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ScoringProfileError::UnknownHardFail { ref section, ref check }
                if section == "dns" && check == "nonexistent_check"),
            "expected UnknownHardFail for dns/nonexistent_check, got: {err}"
        );
    }
}
