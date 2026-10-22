//! The score under the letter (requirement 5): rounded down, float error guarded, never at or
//! above the next grade's threshold unless a hard fail set the grade.
use netray_engine::profile::Profiles;
use netray_model::Grade;
use netray_web::pages::result::views::score_number;

#[test]
fn the_score_never_contradicts_the_letter() {
    let thresholds = &Profiles::get(3).expect("profile 3").thresholds;
    let rows: [(f64, Grade, bool, u8); 8] = [
        (89.663, Grade::B, false, 89),
        (95.71, Grade::A, false, 95),
        // An exact 80 that the engine's division leaves at 79.999…
        (79.99999999999999, Grade::B, false, 80),
        (57.999999999999986, Grade::D, false, 58),
        // An exact 90 the engine graded B through the same float error stays under 90.
        (89.99999999999999, Grade::B, false, 89),
        (100.0, Grade::APlus, false, 100),
        (0.0, Grade::F, false, 0),
        // A hard fail sets F whatever the score: the number stays.
        (92.4, Grade::F, true, 92),
    ];
    for (pct, grade, hard, want) in rows {
        assert_eq!(
            score_number(pct, grade, hard, thresholds),
            want,
            "{pct} {grade:?}"
        );
    }
}
