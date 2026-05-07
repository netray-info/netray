pub fn color_for_grade(grade: &str) -> &'static str {
    match grade {
        "A+" => "#16a34a",
        "A" => "#22c55e",
        "B" => "#eab308",
        "C" => "#f59e0b",
        "D" => "#dc2626",
        "F" => "#991b1b",
        _ => "#6b7280",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_grades_have_distinct_colors() {
        let grades = ["A+", "A", "B", "C", "D", "F"];
        let colors: Vec<_> = grades.iter().map(|g| color_for_grade(g)).collect();
        for (i, c1) in colors.iter().enumerate() {
            for (j, c2) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(c1, c2, "grades {} and {} share color {}", grades[i], grades[j], c1);
                }
            }
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
