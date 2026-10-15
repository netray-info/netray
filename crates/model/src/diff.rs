//! The difference between two stored runs: check changes, evidence changes and the grade.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::evidence::{Block, BlockId, ShapeKind};
use crate::run::{Run, SectionKind};
use crate::{CheckId, Grade, Protocol, Status, canonical};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diff {
    pub checks: Vec<CheckChange>,
    pub evidence: Vec<BlockChange>,
    pub grade: GradeChange,
    /// Whether the two runs' engine versions differ.
    pub engine_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckChange {
    pub id: CheckId,
    pub class: ChangeClass,
    pub from: Option<Status>,
    pub to: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClass {
    Fixed,
    Improved,
    New,
    Worse,
    NowApplicable,
    NoLongerApplicable,
    IntoNotTested,
    OutOfNotTested,
    IntoNotMeasured,
    OutOfNotMeasured,
    Added,
    Removed,
    AddedByRelease,
    RemovedByRelease,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockChange {
    pub block: BlockId,
    pub kind: BlockChangeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockChangeKind {
    Added,
    Removed,
    /// `items` by shape: fields and table, `~ key` per changed key or row, `- key` per removed
    /// and `+ key` per added one; records, a signed descriptor per added or removed record;
    /// chain, the changed positions as strings; tree, the changed node paths; text, none.
    Changed {
        items: Vec<String>,
    },
}

/// The grade of both runs. `rescored` is the predecessor re-scored under the current profile,
/// set only when the profile versions differ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeChange {
    pub from: Grade,
    pub rescored: Option<Grade>,
    pub to: Grade,
}

/// Compares `cur` with its predecessor `prev`. `volatile` are evidence paths ignored, as in
/// [`crate::run::fingerprint`].
pub fn diff(prev: &Run, cur: &Run, volatile: &[&str], rescored_prev: Option<Grade>) -> Diff {
    let engine_changed = prev.engine_version != cur.engine_version;
    Diff {
        checks: check_changes(prev, cur, engine_changed),
        evidence: evidence_changes(prev, cur, volatile),
        grade: GradeChange {
            from: prev.grade,
            rescored: rescored_prev,
            to: cur.grade,
        },
        engine_changed,
    }
}

fn check_changes(prev: &Run, cur: &Run, engine_changed: bool) -> Vec<CheckChange> {
    let from: BTreeMap<&CheckId, Status> = prev.checks.iter().map(|c| (&c.id, c.status)).collect();
    let to: BTreeMap<&CheckId, Status> = cur.checks.iter().map(|c| (&c.id, c.status)).collect();
    let ids: BTreeSet<&CheckId> = from.keys().chain(to.keys()).copied().collect();
    ids.into_iter()
        .filter_map(|id| {
            let (f, t) = (from.get(id).copied(), to.get(id).copied());
            let class = match (f, t) {
                (Some(f), Some(t)) if f == t => return None,
                (Some(f), Some(t)) => classify(f, t),
                (None, _) if engine_changed => ChangeClass::AddedByRelease,
                (None, _) => ChangeClass::Added,
                (_, None) if engine_changed => ChangeClass::RemovedByRelease,
                (_, None) => ChangeClass::Removed,
            };
            Some(CheckChange {
                id: id.clone(),
                class,
                from: f,
                to: t,
            })
        })
        .collect()
}

/// The class of a status change; `from` and `to` differ. A not-tested or unmeasured side
/// classifies the change, and `to` decides when both are.
fn classify(from: Status, to: Status) -> ChangeClass {
    use Status::*;
    match (from, to) {
        (_, NotTested) => ChangeClass::IntoNotTested,
        (_, Unmeasured) => ChangeClass::IntoNotMeasured,
        (NotTested, _) => ChangeClass::OutOfNotTested,
        (Unmeasured, _) => ChangeClass::OutOfNotMeasured,
        (NotApplicable, _) => ChangeClass::NowApplicable,
        (_, NotApplicable) => ChangeClass::NoLongerApplicable,
        (Warn | Fail, Pass) => ChangeClass::Fixed,
        (Fail, Warn) => ChangeClass::Improved,
        (Pass, _) => ChangeClass::New,
        (Warn | Fail, _) => ChangeClass::Worse,
    }
}

fn measured(run: &Run) -> BTreeSet<Protocol> {
    run.sections
        .iter()
        .filter(|s| s.outcome == SectionKind::Measured)
        .map(|s| s.protocol)
        .collect()
}

/// The blocks of `run` whose section is measured in both runs, by id.
fn blocks_of<'a>(run: &'a Run, both: &BTreeSet<&str>) -> BTreeMap<&'a BlockId, &'a Block> {
    run.blocks
        .iter()
        .filter(|b| both.contains(b.id.module()))
        .map(|b| (&b.id, b))
        .collect()
}

/// Changes of the blocks whose section is measured in both runs.
fn evidence_changes(prev: &Run, cur: &Run, volatile: &[&str]) -> Vec<BlockChange> {
    let both: BTreeSet<&str> = measured(prev)
        .intersection(&measured(cur))
        .map(|p| p.as_str())
        .collect();
    let (from, to) = (blocks_of(prev, &both), blocks_of(cur, &both));
    let ids: BTreeSet<&BlockId> = from.keys().chain(to.keys()).copied().collect();
    ids.into_iter()
        .filter_map(|id| {
            let kind = match (from.get(id), to.get(id)) {
                (Some(p), Some(c)) => BlockChangeKind::Changed {
                    items: changed_items(p, c, volatile)?,
                },
                (None, _) => BlockChangeKind::Added,
                (_, None) => BlockChangeKind::Removed,
            };
            Some(BlockChange {
                block: id.clone(),
                kind,
            })
        })
        .collect()
}

fn array(v: Option<&Json>) -> &[Json] {
    v.and_then(Json::as_array).map_or(&[], Vec::as_slice)
}

/// The changed items of a block present in both runs, or `None` when its canonical forms are
/// equal. A block whose shape kind changed has no items.
fn changed_items(prev: &Block, cur: &Block, volatile: &[&str]) -> Option<Vec<String>> {
    let kind = prev.shape.kind();
    let (p, c) = (
        canonical::canonical_shape(prev, volatile),
        canonical::canonical_shape(cur, volatile),
    );
    if kind != cur.shape.kind() {
        return Some(vec![]);
    }
    if p == c {
        return None;
    }
    let (p, c) = (Some(&p), Some(&c));
    Some(match kind {
        ShapeKind::Fields => field_items(array(p), array(c)),
        ShapeKind::Table => table_items(array(p), array(c)),
        ShapeKind::Records => record_items(array(p), array(c)),
        ShapeKind::Chain => (0..array(p).len().max(array(c).len()))
            .filter(|&i| array(p).get(i) != array(c).get(i))
            .map(|i| i.to_string())
            .collect(),
        ShapeKind::Tree => {
            let mut out = Vec::new();
            if let (Some(p), Some(c)) = (p, c) {
                node_items("root", p, c, &mut out);
            }
            out
        }
        ShapeKind::Text => vec![],
    })
}

/// Entries of canonical fields grouped by key.
fn by_key(entries: &[Json]) -> BTreeMap<&str, Vec<Option<&Json>>> {
    let mut out: BTreeMap<&str, Vec<Option<&Json>>> = BTreeMap::new();
    for e in entries {
        if let Some(key) = e.get(0).and_then(Json::as_str) {
            out.entry(key).or_default().push(e.get(1));
        }
    }
    out
}

/// `~ key` for a key whose value differs, `- key` for one only in `prev`, `+ key` for one only
/// in `cur`, in key order.
fn signed_items<'a, T: PartialEq>(
    prev: &BTreeMap<&'a str, T>,
    cur: &BTreeMap<&'a str, T>,
) -> Vec<String> {
    let keys: BTreeSet<&str> = prev.keys().chain(cur.keys()).copied().collect();
    keys.into_iter()
        .filter_map(|k| match (prev.get(k), cur.get(k)) {
            (Some(p), Some(c)) if p == c => None,
            (Some(_), Some(_)) => Some(format!("~ {k}")),
            (Some(_), None) => Some(format!("- {k}")),
            _ => Some(format!("+ {k}")),
        })
        .collect()
}

fn field_items(prev: &[Json], cur: &[Json]) -> Vec<String> {
    signed_items(&by_key(prev), &by_key(cur))
}

/// Canonical table rows by row key.
fn rows_by_key(rows: &[Json]) -> BTreeMap<&str, Option<&Json>> {
    rows.iter()
        .filter_map(|r| Some((r.get(0)?.as_str()?, r.get(1))))
        .collect()
}

fn table_items(prev: &[Json], cur: &[Json]) -> Vec<String> {
    signed_items(&rows_by_key(prev), &rows_by_key(cur))
}

/// A readable descriptor of a canonical record: the sign, then its values.
fn describe(sign: char, record: &Json) -> String {
    let values: Vec<String> = array(Some(record))
        .iter()
        .filter_map(|pair| pair.get(1)?.get("v"))
        .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
        .collect();
    format!("{sign} {}", values.join(" "))
}

/// One descriptor per record of the multiset difference.
fn record_items(prev: &[Json], cur: &[Json]) -> Vec<String> {
    let mut counts: BTreeMap<String, (i64, &Json)> = BTreeMap::new();
    for r in prev {
        counts.entry(r.to_string()).or_insert((0, r)).0 += 1;
    }
    for r in cur {
        counts.entry(r.to_string()).or_insert((0, r)).0 -= 1;
    }
    let mut out = Vec::new();
    for (n, record) in counts.into_values() {
        let (sign, times) = if n > 0 { ('-', n) } else { ('+', -n) };
        out.extend((0..times).map(|_| describe(sign, record)));
    }
    out
}

/// Paths of changed nodes; a canonical node is `[fields, children]`.
fn node_items(path: &str, prev: &Json, cur: &Json, out: &mut Vec<String>) {
    if prev.get(0) != cur.get(0) {
        out.push(path.to_string());
    }
    let (p, c) = (array(prev.get(1)), array(cur.get(1)));
    for i in 0..p.len().max(c.len()) {
        let child = format!("{path}/{i}");
        match (p.get(i), c.get(i)) {
            (Some(p), Some(c)) => node_items(&child, p, c, out),
            _ => out.push(child),
        }
    }
}
