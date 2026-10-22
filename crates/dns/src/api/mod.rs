//! The DNS inspections behind the jobs: query, check, trace, DNSSEC, compare and authcompare.

pub mod authcompare;
pub mod check;
pub mod compare;
pub mod dnssec;
pub mod meta;
pub mod parse;
pub mod query;
pub mod trace;

use mhost::resolver::Lookups;
use serde::{Deserialize, Serialize};

pub use check::{CheckDoneEvent, CheckEvent, LintEvent};

/// Hard cap on total SSE stream duration (SDD §8.1).
pub const STREAM_TIMEOUT_SECS: u64 = 30;

/// SSE batch event emitted once per record type as DNS results arrive.
#[derive(Serialize, Deserialize)]
pub struct BatchEvent {
    pub request_id: String,
    pub record_type: String,
    pub lookups: Lookups,
    pub completed: u32,
    pub total: u32,
    /// Transport used for this batch (compare mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    /// Source of this batch: "recursive" or "authoritative" (auth mode only). A check batch read at
    /// the zone or an ancestor carries "zone" in memory, which the stream leaves out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Non-streaming JSON collection
// ---------------------------------------------------------------------------

/// Hard cap on simultaneous in-flight DNS resolver tasks across all fan-outs.
///
/// Each record-type × server pair acquires one permit before spawning, so the
/// total number of concurrent resolver tasks is bounded to this value regardless
/// of how many queries are in flight. 16 permits cover a worst-case single-query
/// fan-out (10 types × ~2 resolvers per provider = ~20 spawns) at half capacity,
/// leaving room for concurrent requests without starvation.
pub const QUERY_SEMAPHORE_PERMITS: usize = 16;
