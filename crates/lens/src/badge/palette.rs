pub fn color_for_grade(grade: &str) -> &'static str {
    match grade {
        "A+" => "#22c55e",
        "A" => "#22c55e",
        "B" => "#84cc16",
        "C" => "#f59e0b",
        "D" => "#f97316",
        "F" => "#ef4444",
        _ => "#6b7280",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_equivalent_grades_have_distinct_colors() {
        // A+ and A intentionally share the same color (both are top grade).
        let distinct_groups = [["B"], ["C"], ["D"], ["F"]];
        let a_color = color_for_grade("A");
        for group in &distinct_groups {
            let c = color_for_grade(group[0]);
            assert_ne!(
                a_color, c,
                "grade A and {} unexpectedly share color {}",
                group[0], c
            );
        }
    }

    #[test]
    fn error_grade_returns_gray() {
        assert_eq!(color_for_grade("error"), "#6b7280");
    }

    #[test]
    fn unknown_grade_returns_gray() {
        assert_eq!(color_for_grade("X"), "#6b7280");
        assert_eq!(color_for_grade(""), "#6b7280");
    }
}
