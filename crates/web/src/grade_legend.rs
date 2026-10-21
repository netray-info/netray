//! The grade legend's wording, held once: a grade's descriptor and meaning as V1's grade legend
//! states them (`crates/lens/frontend/src/lib/gradeLegend.ts`).

use netray_model::Grade;

/// The home page legend's rows, A+ to F: letter, `data-tone`, descriptor, meaning.
pub fn legend() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    use crate::entries::view::{grade_letter, grade_tone};
    [
        Grade::APlus,
        Grade::A,
        Grade::B,
        Grade::C,
        Grade::D,
        Grade::F,
    ]
    .into_iter()
    .map(|grade| {
        let (word, meaning) = descriptor(grade)
            .split_once(", ")
            .unwrap_or((descriptor(grade), ""));
        (
            grade_letter(grade),
            grade_tone(grade).as_str(),
            word,
            meaning,
        )
    })
    .collect()
}

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
