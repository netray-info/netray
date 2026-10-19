//! The grade legend's wording, held once: a grade's descriptor and meaning as V1's grade legend
//! states them (`crates/lens/frontend/src/lib/gradeLegend.ts`).

use netray_model::Grade;

/// "descriptor, meaning" of `grade`.
pub fn descriptor(grade: Grade) -> &'static str {
    match grade {
        Grade::APlus => "excellent, ahead of most domains",
        Grade::A => "strong, minor polish possible",
        Grade::B => "ok, weaknesses worth fixing",
        Grade::C => "risky, fix before your next audit",
        Grade::D => "broken, users will notice",
        Grade::F => "critical, fix today",
        Grade::Incomplete => "incomplete, not every section measured",
    }
}
