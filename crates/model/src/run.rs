//! The stored run: one scan's sections, check statuses and evidence, and its fingerprint.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::evidence::{Block, BlockId};
use crate::{CheckId, Grade, Protocol, Status, canonical};

/// One scan as stored for history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Run {
    pub profile_version: u32,
    pub engine_version: String,
    pub evidence_schema_version: u32,
    pub grade: Grade,
    pub complete: bool,
    pub sections: Vec<RunSection>,
    pub checks: Vec<RunCheck>,
    pub blocks: Vec<Block>,
}

/// How one protocol's section ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RunSection {
    pub protocol: Protocol,
    pub outcome: SectionKind,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Measured,
    NotApplicable,
    Incomplete,
    TimedOut,
}

/// A check's id, status, findings and evidence references. Findings and evidence are outside
/// the fingerprint and the diff.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RunCheck {
    pub id: CheckId,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<BlockId>,
}

/// Lower-case hex SHA-256 of the run's canonical JSON: profile version, check statuses and
/// evidence, each in a fixed order. The engine version, grade and sections are not part of it.
///
/// `volatile` lists evidence paths left out: `<block name>/<key>`, or
/// `<block name>/<column>@<row key>` for one table cell. The block name has no qualifier.
/// `<block name>/*` leaves out the whole block, its presence included.
pub fn fingerprint(run: &Run, volatile: &[&str]) -> String {
    let mut checks: Vec<&RunCheck> = run.checks.iter().collect();
    checks.sort_by(|a, b| a.id.cmp(&b.id));
    let mut blocks: Vec<&Block> = run
        .blocks
        .iter()
        .filter(|b| !canonical::block_volatile(&b.id, volatile))
        .collect();
    blocks.sort_by(|a, b| a.id.cmp(&b.id));
    let canonical = json!([
        run.profile_version,
        checks
            .iter()
            .map(|c| json!([c.id, c.status]))
            .collect::<Vec<_>>(),
        blocks
            .iter()
            .map(|b| canonical::canonical_block(b, volatile))
            .collect::<Vec<_>>(),
    ]);
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
}
