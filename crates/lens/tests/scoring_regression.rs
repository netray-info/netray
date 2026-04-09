/// Pure unit regression tests for the scoring engine.
///
/// No network access. Tests use the embedded default profile.
use std::collections::HashMap;
use lens::scoring::engine::{CheckResult, CheckVerdict, SectionInput, compute_score};
use lens::scoring::profile::ScoringProfile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn no_error(checks: Vec<CheckResult>) -> SectionInput {
    SectionInput {
        checks,
        errored: false,
    }
}

fn errored() -> SectionInput {
    SectionInput {
        checks: vec![],
        errored: true,
    }
}

fn inputs(dns: SectionInput, tls: SectionInput, ip: SectionInput) -> HashMap<String, SectionInput> {
    HashMap::from([
        ("dns".to_string(), dns),
        ("tls".to_string(), tls),
        ("ip".to_string(), ip),
    ])
}

/// Build all-pass inputs from the profile's own check keys.
fn all_pass(profile: &ScoringProfile) -> (SectionInput, SectionInput, SectionInput) {
    let dns = no_error(
        profile.sections["dns"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect(),
    );
    let tls = no_error(
        profile.sections["tls"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect(),
    );
    let ip = no_error(
        profile.sections["ip"]
            .checks
            .keys()
            .map(|k| pass(k))
            .collect(),
    );
    (dns, tls, ip)
}

// ---------------------------------------------------------------------------
// 1. Default profile parses correctly
// ---------------------------------------------------------------------------

#[test]
fn default_profile_parses_correctly() {
    let profile = default_profile();

    // Basic structural checks
    assert_eq!(profile.meta.name, "default");
    assert_eq!(profile.meta.version, 2);

    // Section weights are non-zero
    assert!(profile.sections["dns"].weight > 0);
    assert!(profile.sections["tls"].weight > 0);
    assert!(profile.sections["ip"].weight > 0);

    // Grade thresholds include the expected grades
    let grades: Vec<&String> = profile.thresholds.keys().collect();
    assert!(
        grades.iter().any(|g| g.as_str() == "A+"),
        "profile must define A+ threshold"
    );
    assert!(
        grades.iter().any(|g| g.as_str() == "A"),
        "profile must define A threshold"
    );
    assert!(
        grades.iter().any(|g| g.as_str() == "F"),
        "profile must define F threshold"
    );

    // Hard fail lists are present
    assert!(
        !profile.sections["tls"].hard_fail.is_empty(),
        "tls hard_fail must not be empty"
    );
    assert!(
        !profile.sections["dns"].hard_fail.is_empty(),
        "dns hard_fail must not be empty"
    );

    // Each section has at least one weighted check
    assert!(
        !profile.sections["dns"].checks.is_empty(),
        "dns section must have weighted checks"
    );
    assert!(
        !profile.sections["tls"].checks.is_empty(),
        "tls section must have weighted checks"
    );
    assert!(
        !profile.sections["ip"].checks.is_empty(),
        "ip section must have weighted checks"
    );
}

// ---------------------------------------------------------------------------
// 2. Perfect domain (all pass) scores A+
// ---------------------------------------------------------------------------

#[test]
fn perfect_domain_all_pass_scores_a_plus() {
    let profile = default_profile();
    let (dns, tls, ip) = all_pass(&profile);
    let result = compute_score(&profile, &inputs(dns, tls, ip));

    assert!(!result.hard_fail_triggered);
    assert_eq!(result.grade, "A+", "all-pass should produce A+");
    assert!(
        (result.overall_percentage - 100.0).abs() < 0.001,
        "all-pass should yield 100% score, got {:.2}",
        result.overall_percentage,
    );
}

// ---------------------------------------------------------------------------
// 3. Missing SPF → F (hard fail)
// ---------------------------------------------------------------------------

#[test]
fn missing_spf_triggers_hard_fail_f() {
    let profile = default_profile();
    let (_, tls, ip) = all_pass(&profile);

    // SPF not found — simulates domain with no SPF record
    let dns = no_error(vec![not_found("spf"), pass("dmarc")]);
    let result = compute_score(&profile, &inputs(dns, tls, ip));

    assert!(
        result.hard_fail_triggered,
        "missing SPF must trigger hard fail"
    );
    assert_eq!(result.grade, "F", "missing SPF must produce grade F");
    assert!(
        result.hard_fail_checks.contains(&"spf".to_string()),
        "spf must appear in hard_fail_checks, got {:?}",
        result.hard_fail_checks,
    );
}

// ---------------------------------------------------------------------------
// 4. Missing DMARC → F (hard fail)
// ---------------------------------------------------------------------------

#[test]
fn missing_dmarc_triggers_hard_fail_f() {
    let profile = default_profile();
    let (_, tls, ip) = all_pass(&profile);

    // DMARC not found — simulates domain with no DMARC policy
    let dns = no_error(vec![pass("spf"), not_found("dmarc")]);
    let result = compute_score(&profile, &inputs(dns, tls, ip));

    assert!(
        result.hard_fail_triggered,
        "missing DMARC must trigger hard fail"
    );
    assert_eq!(result.grade, "F", "missing DMARC must produce grade F");
    assert!(
        result.hard_fail_checks.contains(&"dmarc".to_string()),
        "dmarc must appear in hard_fail_checks, got {:?}",
        result.hard_fail_checks,
    );
}

// ---------------------------------------------------------------------------
// 5. Expired cert (not_expired=fail) → F (hard fail)
// ---------------------------------------------------------------------------

#[test]
fn expired_cert_triggers_hard_fail_f() {
    let profile = default_profile();
    let (dns, _, ip) = all_pass(&profile);

    // not_expired fails — simulates an expired certificate
    let mut tls_checks: Vec<CheckResult> = profile.sections["tls"]
        .checks
        .keys()
        .map(|k| pass(k))
        .collect();
    for c in &mut tls_checks {
        if c.name == "not_expired" {
            c.verdict = CheckVerdict::Fail;
        }
    }
    let result = compute_score(&profile, &inputs(dns, no_error(tls_checks), ip));

    assert!(
        result.hard_fail_triggered,
        "expired cert must trigger hard fail"
    );
    assert_eq!(result.grade, "F", "expired cert must produce grade F");
    assert!(
        result.hard_fail_checks.contains(&"not_expired".to_string()),
        "not_expired must appear in hard_fail_checks, got {:?}",
        result.hard_fail_checks,
    );
}

// ---------------------------------------------------------------------------
// 6. Untrusted chain (chain_trusted=fail) → F (hard fail)
// ---------------------------------------------------------------------------

#[test]
fn untrusted_chain_triggers_hard_fail_f() {
    let profile = default_profile();
    let (dns, _, ip) = all_pass(&profile);

    // chain_trusted fails — simulates a self-signed or otherwise untrusted cert
    let mut tls_checks: Vec<CheckResult> = profile.sections["tls"]
        .checks
        .keys()
        .map(|k| pass(k))
        .collect();
    for c in &mut tls_checks {
        if c.name == "chain_trusted" {
            c.verdict = CheckVerdict::Fail;
        }
    }
    let result = compute_score(&profile, &inputs(dns, no_error(tls_checks), ip));

    assert!(
        result.hard_fail_triggered,
        "untrusted chain must trigger hard fail"
    );
    assert_eq!(result.grade, "F", "untrusted chain must produce grade F");
    assert!(
        result
            .hard_fail_checks
            .contains(&"chain_trusted".to_string()),
        "chain_trusted must appear in hard_fail_checks, got {:?}",
        result.hard_fail_checks,
    );
}

// ---------------------------------------------------------------------------
// 7. All DNS checks warn + all TLS checks pass → grade B or C (not A/A+)
// ---------------------------------------------------------------------------

#[test]
fn all_dns_warn_with_all_tls_pass_grades_below_a() {
    let profile = default_profile();

    // All DNS checks at Warn (half credit each)
    let dns_checks: Vec<CheckResult> = profile.sections["dns"]
        .checks
        .keys()
        .map(|k| warn(k))
        .collect();
    // All TLS checks pass
    let tls_checks: Vec<CheckResult> = profile.sections["tls"]
        .checks
        .keys()
        .map(|k| pass(k))
        .collect();
    // All IP checks pass
    let ip_checks: Vec<CheckResult> = profile.sections["ip"]
        .checks
        .keys()
        .map(|k| pass(k))
        .collect();

    let result = compute_score(&profile, &inputs(no_error(dns_checks), no_error(tls_checks), no_error(ip_checks)));

    // DNS section should be 50% (all warn = half credit).
    // TLS and IP sections should be 100%.
    // Overall will be between 50% and 100% depending on section weights.
    // With section_weights dns=35, tls=45, ip=20:
    //   overall = (50*35 + 100*45 + 100*20) / 100 = (1750 + 4500 + 2000) / 100 = 82.5
    // → grade B (75..89.9)
    assert!(
        !matches!(result.grade.as_str(), "A" | "A+"),
        "all-DNS-warn should not produce A or A+, got {} (score={:.1})",
        result.grade,
        result.overall_percentage,
    );
    assert!(
        matches!(result.grade.as_str(), "B" | "C"),
        "all-DNS-warn should produce B or C, got {} (score={:.1})",
        result.grade,
        result.overall_percentage,
    );
    assert!(
        !result.hard_fail_triggered,
        "warn verdicts must not trigger hard fail"
    );
}

// ---------------------------------------------------------------------------
// 8. Grade boundary: score exactly 97 → A+, score exactly 96.9 → A
// ---------------------------------------------------------------------------

#[test]
fn grade_boundary_97_is_a_plus() {
    let profile = default_profile();
    let grade = lookup_grade_via_profile(&profile, 97.0);
    assert_eq!(grade, "A+", "score 97.0 must yield A+");
}

#[test]
fn grade_boundary_96_9_is_a() {
    let profile = default_profile();
    let grade = lookup_grade_via_profile(&profile, 96.9);
    assert_eq!(grade, "A", "score 96.9 must yield A");
}

/// Drive grade lookup through the public compute_score API by crafting a single-section
/// scenario where overall_percentage equals the target.
///
/// Uses dns-only (tls + ip errored). A single fake check named "synthetic" with weight 1000
/// gives us precise fractional control: pass 970/1000 = 97.0%, etc.
fn lookup_grade_via_profile(profile: &ScoringProfile, target_percentage: f64) -> String {
    use lens::scoring::profile::{ProfileMeta, ScoringProfile, SectionProfile};
    use std::collections::{BTreeMap, HashMap};

    // Build a minimal profile with a single dns check weighted 1000.
    // earned = round(target * 10) out of 1000 → precise to 0.1%.
    let earned: u32 = (target_percentage * 10.0).round() as u32;
    let possible: u32 = 1000;

    // Two checks: one pass (earned weight), one fail (remainder).
    let fail_weight = possible - earned;

    let mut dns_checks = HashMap::new();
    dns_checks.insert("synthetic_pass".to_string(), earned);
    if fail_weight > 0 {
        dns_checks.insert("synthetic_fail".to_string(), fail_weight);
    }

    let mut thresholds = BTreeMap::new();
    // Copy thresholds from the real profile.
    for (grade, val) in &profile.thresholds {
        thresholds.insert(grade.clone(), *val);
    }

    let mut sections = HashMap::new();
    sections.insert(
        "dns".to_string(),
        SectionProfile {
            weight: 1,
            hard_fail: vec![],
            checks: dns_checks,
        },
    );

    let synthetic_profile = ScoringProfile {
        meta: ProfileMeta {
            name: "synthetic".to_string(),
            version: 2,
        },
        sections,
        thresholds,
    };

    let mut dns_input_checks = vec![pass("synthetic_pass")];
    if fail_weight > 0 {
        dns_input_checks.push(fail("synthetic_fail"));
    }

    let result = compute_score(&synthetic_profile, &inputs(no_error(dns_input_checks), errored(), errored()));

    result.grade
}
