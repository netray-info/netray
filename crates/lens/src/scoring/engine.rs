use std::collections::HashMap;

use crate::scoring::profile::ScoringProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckVerdict {
    Pass,
    Warn,
    Fail,
    NotFound,
    /// Excluded from both earned and possible totals.
    Skip,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub verdict: CheckVerdict,
    /// Diagnostic messages explaining a non-passing verdict (warn/fail/not-found).
    /// Empty for pass and skip.
    pub messages: Vec<String>,
}

/// Status of a section for scoring purposes.
#[derive(Debug, Clone)]
pub enum SectionStatus {
    Scored,
    Errored,
    NotApplicable { reason: String },
}

/// Per-section input to the scoring engine.
pub struct SectionInput {
    pub checks: Vec<CheckResult>,
    pub status: SectionStatus,
}

#[derive(Debug, Clone)]
pub struct SectionScore {
    pub earned: u32,
    pub possible: u32,
    /// 0.0–100.0. If no weighted checks exist (possible == 0), returns 100.0 (full credit).
    pub percentage: f64,
}

#[derive(Debug, Clone)]
pub struct OverallScore {
    /// Per-section scores. Missing key = section errored or absent from profile.
    pub sections: HashMap<String, SectionScore>,
    pub overall_percentage: f64,
    pub grade: String,
    pub hard_fail_triggered: bool,
    /// Which specific checks triggered a hard fail.
    pub hard_fail_checks: Vec<String>,
    /// Section name → reason. Always present (may be empty). Populated only for NotApplicable.
    pub not_applicable: HashMap<String, String>,
}

/// Score a single section. Returns None if the section errored or is not applicable.
///
/// Checks not present in `section_checks` are ignored (unweighted).
/// Skip verdicts are excluded from both earned and possible.
/// Pass = full weight, Warn = weight / 2, Fail/NotFound = 0.
/// If possible == 0 (no weighted checks at all), returns 100% (full credit).
pub fn score_section(
    section_checks: &HashMap<String, u32>,
    input: &SectionInput,
) -> Option<SectionScore> {
    match input.status {
        SectionStatus::Errored | SectionStatus::NotApplicable { .. } => return None,
        SectionStatus::Scored => {}
    }

    let mut earned: u32 = 0;
    let mut possible: u32 = 0;

    for check in &input.checks {
        let weight = match section_checks.get(&check.name) {
            Some(&w) => w,
            None => continue, // unweighted check — ignore
        };

        match check.verdict {
            CheckVerdict::Skip => {
                // excluded from both totals
            }
            CheckVerdict::Pass => {
                earned += weight;
                possible += weight;
            }
            CheckVerdict::Warn => {
                earned += weight / 2;
                possible += weight;
            }
            CheckVerdict::Fail | CheckVerdict::NotFound => {
                possible += weight;
            }
        }
    }

    // If no weighted checks were present at all, exclude section (return None)
    // rather than awarding 100% — an empty section carries no signal.
    if possible == 0 {
        return None;
    }

    let percentage = (earned as f64 / possible as f64) * 100.0;

    Some(SectionScore {
        earned,
        possible,
        percentage,
    })
}

/// Compute the overall score across all sections.
///
/// Overall percentage = weighted average of available (non-errored) section scores.
/// Grade is determined by comparing overall_percentage against thresholds (desc order).
/// Hard fail: if any check in `profile.hard_fail.{section}` has verdict Fail or NotFound,
/// the grade is forced to "F" regardless of score.
pub fn compute_score(
    profile: &ScoringProfile,
    inputs: &HashMap<String, SectionInput>,
) -> OverallScore {
    let mut sections: HashMap<String, SectionScore> = HashMap::new();
    let mut not_applicable: HashMap<String, String> = HashMap::new();
    let mut weighted_sum: f64 = 0.0;
    let mut total_weight: u32 = 0;

    for (name, section) in &profile.sections {
        if let Some(input) = inputs.get(name) {
            if let SectionStatus::NotApplicable { reason } = &input.status {
                not_applicable.insert(name.clone(), reason.clone());
                continue;
            }
            if let Some(score) = score_section(&section.checks, input) {
                weighted_sum += score.percentage * section.weight as f64;
                total_weight += section.weight;
                sections.insert(name.clone(), score);
            }
        }
    }

    // If all sections errored or had no weighted checks, we have no signal.
    if total_weight == 0 {
        return OverallScore {
            sections,
            overall_percentage: 0.0,
            grade: "error".to_string(),
            hard_fail_triggered: false,
            hard_fail_checks: vec![],
            not_applicable,
        };
    }

    let overall_percentage = weighted_sum / total_weight as f64;

    // Check hard fails before assigning grade.
    let mut hard_fail_checks: Vec<String> = Vec::new();

    for (name, section) in &profile.sections {
        if let Some(input) = inputs.get(name) {
            check_hard_fails(&section.hard_fail, &input.checks, &mut hard_fail_checks);
        }
    }

    let hard_fail_triggered = !hard_fail_checks.is_empty();

    let grade = if hard_fail_triggered {
        "F".to_string()
    } else {
        lookup_grade(&profile.thresholds, overall_percentage)
    };

    OverallScore {
        sections,
        overall_percentage,
        grade,
        hard_fail_triggered,
        hard_fail_checks,
        not_applicable,
    }
}

fn check_hard_fails(
    hard_fail_names: &[String],
    checks: &[CheckResult],
    triggered: &mut Vec<String>,
) {
    for name in hard_fail_names {
        let found = checks.iter().find(|c| &c.name == name);
        match found {
            Some(c) if matches!(c.verdict, CheckVerdict::Fail | CheckVerdict::NotFound) => {
                triggered.push(name.clone());
            }
            _ => {}
        }
    }
}

/// Return the highest grade whose threshold value is <= percentage.
/// Thresholds are iterated in descending value order.
/// Falls back to "F" if no threshold matches (should not happen with a valid profile).
pub fn lookup_grade(
    thresholds: &std::collections::BTreeMap<String, u32>,
    percentage: f64,
) -> String {
    // Sort entries by value descending.
    let mut entries: Vec<(&String, &u32)> = thresholds.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));

    for (grade, threshold) in &entries {
        if percentage >= **threshold as f64 {
            return grade.to_string();
        }
    }

    "F".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::profile::ScoringProfile;

    fn default_profile() -> ScoringProfile {
        ScoringProfile::embedded_default()
    }

    fn pass(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::Pass,
            messages: vec![],
        }
    }

    fn warn(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::Warn,
            messages: vec![],
        }
    }

    fn fail(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::Fail,
            messages: vec![],
        }
    }

    fn not_found(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::NotFound,
            messages: vec![],
        }
    }

    fn skip(name: &str) -> CheckResult {
        CheckResult {
            name: name.to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![],
        }
    }

    fn simple_weights() -> HashMap<String, u32> {
        let mut m = HashMap::new();
        m.insert("check_a".to_string(), 10);
        m.insert("check_b".to_string(), 4);
        m
    }

    fn no_error(checks: Vec<CheckResult>) -> SectionInput {
        SectionInput {
            checks,
            status: SectionStatus::Scored,
        }
    }

    fn errored() -> SectionInput {
        SectionInput {
            checks: vec![],
            status: SectionStatus::Errored,
        }
    }

    fn not_applicable(reason: &str) -> SectionInput {
        SectionInput {
            checks: vec![],
            status: SectionStatus::NotApplicable { reason: reason.to_string() },
        }
    }

    fn inputs(
        dns: SectionInput,
        tls: SectionInput,
        ip: SectionInput,
    ) -> HashMap<String, SectionInput> {
        HashMap::from([
            ("dns".to_string(), dns),
            ("tls".to_string(), tls),
            ("ip".to_string(), ip),
        ])
    }

    // --- score_section verdict tests ---

    #[test]
    fn pass_earns_full_weight() {
        let weights = simple_weights();
        let input = no_error(vec![pass("check_a")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 10);
        assert_eq!(score.possible, 10);
        assert_eq!(score.percentage, 100.0);
    }

    #[test]
    fn warn_earns_half_weight() {
        let weights = simple_weights();
        let input = no_error(vec![warn("check_a")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 5);
        assert_eq!(score.possible, 10);
        assert_eq!(score.percentage, 50.0);
    }

    #[test]
    fn fail_earns_zero() {
        let weights = simple_weights();
        let input = no_error(vec![fail("check_a")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 0);
        assert_eq!(score.possible, 10);
        assert_eq!(score.percentage, 0.0);
    }

    #[test]
    fn not_found_earns_zero() {
        let weights = simple_weights();
        let input = no_error(vec![not_found("check_a")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 0);
        assert_eq!(score.possible, 10);
        assert_eq!(score.percentage, 0.0);
    }

    #[test]
    fn skip_excluded_from_totals() {
        let weights = simple_weights();
        // check_a skipped, check_b passes — only check_b counts
        let input = no_error(vec![skip("check_a"), pass("check_b")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 4);
        assert_eq!(score.possible, 4);
        assert_eq!(score.percentage, 100.0);
    }

    #[test]
    fn all_skipped_returns_none() {
        // All checks skipped → no signal → section excluded (None), not 100%.
        let weights = simple_weights();
        let input = no_error(vec![skip("check_a"), skip("check_b")]);
        assert!(score_section(&weights, &input).is_none());
    }

    #[test]
    fn empty_checks_returns_none() {
        // No checks at all → no signal → section excluded (None).
        let weights = simple_weights();
        let input = no_error(vec![]);
        assert!(score_section(&weights, &input).is_none());
    }

    #[test]
    fn empty_profile_section_returns_none() {
        // No weighted checks → no signal → section excluded (None).
        let weights: HashMap<String, u32> = HashMap::new();
        let input = no_error(vec![pass("check_a"), fail("check_b")]);
        assert!(score_section(&weights, &input).is_none());
    }

    #[test]
    fn errored_section_returns_none() {
        let weights = simple_weights();
        let input = errored();
        assert!(score_section(&weights, &input).is_none());
    }

    #[test]
    fn unweighted_checks_ignored() {
        let weights = simple_weights();
        // "unknown_check" has no weight — should be ignored entirely
        let input = no_error(vec![fail("unknown_check"), pass("check_a")]);
        let score = score_section(&weights, &input).unwrap();
        assert_eq!(score.earned, 10);
        assert_eq!(score.possible, 10);
    }

    #[test]
    fn warn_integer_division_weight_3() {
        let mut weights = HashMap::new();
        weights.insert("odd".to_string(), 3u32);
        let input = no_error(vec![warn("odd")]);
        let score = score_section(&weights, &input).unwrap();
        // 3 / 2 = 1 (integer division)
        assert_eq!(score.earned, 1);
        assert_eq!(score.possible, 3);
    }

    // --- hard fail tests ---

    #[test]
    fn hard_fail_on_fail_verdict_forces_grade_f() {
        let profile = default_profile();
        // chain_trusted is in tls hard_fail list
        let tls_input = no_error(vec![fail("chain_trusted"), pass("not_expired")]);
        let dns_input = no_error(vec![pass("spf"), pass("dmarc")]);
        let ip_input = no_error(vec![pass("reputation")]);

        let result = compute_score(&profile, &inputs(dns_input, tls_input, ip_input));
        assert!(result.hard_fail_triggered);
        assert_eq!(result.grade, "F");
        assert!(
            result
                .hard_fail_checks
                .contains(&"chain_trusted".to_string())
        );
    }

    #[test]
    fn hard_fail_pass_verdict_does_not_trigger() {
        let profile = default_profile();
        let dns_input = no_error(vec![pass("dnssec"), pass("caa")]);
        let tls_input = no_error(vec![pass("chain_trusted"), pass("not_expired")]);
        let ip_input = no_error(vec![pass("reputation")]);

        let result = compute_score(&profile, &inputs(dns_input, tls_input, ip_input));
        assert!(!result.hard_fail_triggered);
        assert!(result.hard_fail_checks.is_empty());
    }

    #[test]
    fn hard_fail_skip_verdict_does_not_trigger() {
        let profile = default_profile();
        // skip on a hard_fail check should not trigger
        let tls_input = no_error(vec![skip("chain_trusted"), pass("not_expired")]);
        let dns_input = no_error(vec![pass("dnssec"), pass("caa")]);
        let ip_input = no_error(vec![]);

        let result = compute_score(&profile, &inputs(dns_input, tls_input, ip_input));
        assert!(!result.hard_fail_triggered);
    }

    // --- errored section exclusion ---

    #[test]
    fn errored_section_excluded_from_overall() {
        let profile = default_profile();
        // tls errored — overall score is computed from dns + ip only
        let dns_input = no_error(vec![pass("dnssec"), pass("caa")]);
        let tls_input = errored();
        let ip_input = no_error(vec![pass("reputation")]);

        let result = compute_score(&profile, &inputs(dns_input, tls_input, ip_input));
        assert!(!result.sections.contains_key("tls"));
        assert!(result.sections.contains_key("dns"));
        assert!(result.sections.contains_key("ip"));
        // overall_percentage > 0 (dns and ip contributed)
        assert!(result.overall_percentage > 0.0);
    }

    #[test]
    fn all_sections_errored_gives_zero_percent() {
        let profile = default_profile();
        let result = compute_score(&profile, &inputs(errored(), errored(), errored()));
        assert_eq!(result.overall_percentage, 0.0);
    }

    // --- SectionStatus::NotApplicable tests ---

    #[test]
    fn not_applicable_section_returns_none_from_score_section() {
        let weights = simple_weights();
        let input = not_applicable("no MX");
        assert!(score_section(&weights, &input).is_none());
    }

    #[test]
    fn not_applicable_section_recorded_in_overall_score() {
        let profile = default_profile();
        let mut all_inputs = inputs(
            no_error(vec![pass("dnssec")]),
            no_error(vec![pass("chain_trusted")]),
            no_error(vec![pass("reputation")]),
        );
        all_inputs.insert(
            "email".to_string(),
            not_applicable("beacon timeout"),
        );

        let result = compute_score(&profile, &all_inputs);
        assert!(!result.sections.contains_key("email"), "N/A section must not appear in sections");
        assert_eq!(
            result.not_applicable.get("email").map(|s| s.as_str()),
            Some("beacon timeout"),
            "not_applicable must contain the email reason"
        );
    }

    #[test]
    fn errored_section_not_recorded_in_not_applicable() {
        let profile = default_profile();
        let mut all_inputs = inputs(
            no_error(vec![pass("dnssec")]),
            errored(),
            no_error(vec![pass("reputation")]),
        );
        all_inputs.insert("email".to_string(), errored());

        let result = compute_score(&profile, &all_inputs);
        assert!(
            result.not_applicable.is_empty(),
            "errored sections must not populate not_applicable, got {:?}",
            result.not_applicable
        );
    }

    #[test]
    fn all_scored_sections_not_applicable_is_empty() {
        let profile = default_profile();
        let result = compute_score(
            &profile,
            &inputs(
                no_error(vec![pass("dnssec")]),
                no_error(vec![pass("chain_trusted")]),
                no_error(vec![pass("reputation")]),
            ),
        );
        assert!(
            result.not_applicable.is_empty(),
            "not_applicable must be empty when all sections are Scored"
        );
    }

    // --- grade boundary tests ---

    #[test]
    fn grade_boundaries_from_default_profile() {
        let profile = default_profile();

        let cases: &[(&str, f64)] = &[
            ("A+", 97.0),
            ("A", 96.9),
            ("A", 90.0),
            ("B", 89.9),
            ("B", 75.0),
            ("C", 74.9),
            ("C", 60.0),
            ("D", 59.9),
            ("D", 40.0),
            ("F", 39.9),
            ("F", 0.0),
        ];

        for (expected_grade, pct) in cases {
            let grade = super::lookup_grade(&profile.thresholds, *pct);
            assert_eq!(
                &grade, expected_grade,
                "percentage {pct} should give grade {expected_grade}, got {grade}"
            );
        }
    }

    // --- end-to-end scenarios ---

    #[test]
    fn perfect_scores_give_a_plus() {
        let profile = default_profile();

        // Build full pass inputs using the profile's own keys.
        let dns_checks: Vec<CheckResult> = profile.sections["dns"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();
        let tls_checks: Vec<CheckResult> = profile.sections["tls"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();
        let ip_checks: Vec<CheckResult> = profile.sections["ip"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();

        let result = compute_score(
            &profile,
            &inputs(
                no_error(dns_checks),
                no_error(tls_checks),
                no_error(ip_checks),
            ),
        );

        assert!(!result.hard_fail_triggered);
        assert_eq!(result.grade, "A+");
        assert!((result.overall_percentage - 100.0).abs() < 0.001);
    }

    #[test]
    fn hard_fail_with_otherwise_perfect_scores_gives_f() {
        let profile = default_profile();

        let mut tls_checks: Vec<CheckResult> = profile.sections["tls"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();
        // Override chain_trusted to fail
        for c in &mut tls_checks {
            if c.name == "chain_trusted" {
                c.verdict = CheckVerdict::Fail;
            }
        }
        let dns_checks: Vec<CheckResult> = profile.sections["dns"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();
        let ip_checks: Vec<CheckResult> = profile.sections["ip"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect();

        let result = compute_score(
            &profile,
            &inputs(
                no_error(dns_checks),
                no_error(tls_checks),
                no_error(ip_checks),
            ),
        );

        assert!(result.hard_fail_triggered);
        assert_eq!(result.grade, "F");
    }

    #[test]
    fn multiple_hard_fail_checks_all_reported() {
        let profile = default_profile();
        // Both tls hard_fail checks fail
        let tls_input = no_error(vec![fail("chain_trusted"), fail("not_expired")]);
        let dns_input = no_error(vec![pass("dnssec"), pass("caa")]);
        let ip_input = no_error(vec![]);

        let result = compute_score(&profile, &inputs(dns_input, tls_input, ip_input));
        assert!(result.hard_fail_triggered);
        assert!(
            result
                .hard_fail_checks
                .contains(&"chain_trusted".to_string())
        );
        assert!(result.hard_fail_checks.contains(&"not_expired".to_string()));
    }
}
