pub mod types;

pub use types::{
    AllResults, Category, CheckResult, Grade, IpEnrichment, MtaStsInfo, SpfFlat, SseEvent,
    SubCheck, Verdict,
};

/// Compute aggregate grade from category-level verdicts.
///
/// | Fails | Warns | Grade |
/// |-------|-------|-------|
/// | 0     | 0     | A     |
/// | 0     | 1–2   | B     |
/// | 0     | 3+    | C     |
/// | 1     | any   | D     |
/// | 2+    | any   | F     |
pub fn compute_grade(verdicts: &[Verdict]) -> Grade {
    let fails = verdicts.iter().filter(|v| **v == Verdict::Fail).count();
    let warns = verdicts.iter().filter(|v| **v == Verdict::Warn).count();

    match (fails, warns) {
        (0, 0) => Grade::A,
        (0, 1..=2) => Grade::B,
        (0, _) => Grade::C,
        (1, _) => Grade::D,
        _ => Grade::F,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_all_pass() {
        let v = vec![Verdict::Pass, Verdict::Pass, Verdict::Info];
        assert_eq!(compute_grade(&v), Grade::A);
    }

    #[test]
    fn grade_one_warn() {
        let v = vec![Verdict::Pass, Verdict::Warn];
        assert_eq!(compute_grade(&v), Grade::B);
    }

    #[test]
    fn grade_two_warns() {
        let v = vec![Verdict::Warn, Verdict::Warn];
        assert_eq!(compute_grade(&v), Grade::B);
    }

    #[test]
    fn grade_three_warns() {
        let v = vec![Verdict::Warn, Verdict::Warn, Verdict::Warn];
        assert_eq!(compute_grade(&v), Grade::C);
    }

    #[test]
    fn grade_one_fail() {
        let v = vec![Verdict::Fail, Verdict::Pass, Verdict::Pass];
        assert_eq!(compute_grade(&v), Grade::D);
    }

    #[test]
    fn grade_two_fails() {
        let v = vec![Verdict::Fail, Verdict::Fail];
        assert_eq!(compute_grade(&v), Grade::F);
    }

    #[test]
    fn grade_info_ignored() {
        let v = vec![Verdict::Info, Verdict::Info, Verdict::Info];
        assert_eq!(compute_grade(&v), Grade::A);
    }
}
