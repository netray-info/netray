use lens::scoring::engine::{
    CheckResult, CheckVerdict, SectionInput, SectionStatus, compute_score,
};
use lens::scoring::profile::ScoringProfile;
/// Pure unit regression tests for the scoring engine.
///
/// No network access. Tests use the embedded default profile.
use std::collections::HashMap;

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
        status: SectionStatus::Scored,
    }
}

fn errored() -> SectionInput {
    SectionInput {
        checks: vec![],
        status: SectionStatus::Errored,
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

    // Hard fail lists: tls still has hard fails; dns hard_fail cleared in this version
    assert!(
        !profile.sections["tls"].hard_fail.is_empty(),
        "tls hard_fail must not be empty"
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
// 2. Profile structure: four sections, weights sum to 100, hsts/https_redirect
//    moved out of TLS and into HTTP (AC-6, AC-7)
// ---------------------------------------------------------------------------

#[test]
fn profile_section_weights_sum_to_100() {
    let profile = default_profile();
    let sum: u32 = profile.sections.values().map(|s| s.weight).sum();
    assert_eq!(sum, 100, "section weights must sum to 100, got {}", sum);
    assert_eq!(profile.sections["dns"].weight, 20, "dns weight must be 20");
    assert_eq!(profile.sections["tls"].weight, 35, "tls weight must be 35");
    assert_eq!(
        profile.sections["http"].weight, 20,
        "http weight must be 20"
    );
    assert_eq!(
        profile.sections["email"].weight, 15,
        "email weight must be 15"
    );
    assert_eq!(profile.sections["ip"].weight, 10, "ip weight must be 10");
}

#[test]
fn tls_section_does_not_contain_hsts_or_https_redirect() {
    let profile = default_profile();
    let tls_checks = &profile.sections["tls"].checks;
    assert!(
        !tls_checks.contains_key("hsts"),
        "tls section must not contain hsts (moved to http)"
    );
    assert!(
        !tls_checks.contains_key("https_redirect"),
        "tls section must not contain https_redirect (moved to http)"
    );
    // Both checks are now owned by the http section.
    let http_checks = &profile.sections["http"].checks;
    assert!(
        http_checks.contains_key("hsts"),
        "http section must contain hsts"
    );
    assert!(
        http_checks.contains_key("https_redirect"),
        "http section must contain https_redirect"
    );
}

// ---------------------------------------------------------------------------
// 3. Perfect domain (all pass) scores A+
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

    let result = compute_score(
        &profile,
        &inputs(
            no_error(dns_checks),
            no_error(tls_checks),
            no_error(ip_checks),
        ),
    );

    // DNS section should be 50% (all warn = half credit).
    // TLS and IP sections should be 100%.
    // HTTP and email absent from inputs → excluded from weighted average.
    // Active weights: dns=20, tls=35, ip=10, total=65.
    //   overall = (50*20 + 100*35 + 100*10) / 65 = (1000 + 3500 + 1000) / 65 ≈ 84.6
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

    let result = compute_score(
        &synthetic_profile,
        &inputs(no_error(dns_input_checks), errored(), errored()),
    );

    result.grade
}

// ---------------------------------------------------------------------------
// Email section scoring regression (SR1–SR3, O1–O2)
// ---------------------------------------------------------------------------

fn email_all_pass() -> SectionInput {
    no_error(vec![
        pass("email_authentication"),
        pass("email_infrastructure"),
        pass("email_transport"),
        pass("email_brand_policy"),
    ])
}

fn email_no_mx() -> SectionInput {
    // Authentication passes; infra/transport/brand are Skip (no MX records).
    no_error(vec![
        pass("email_authentication"),
        CheckResult {
            name: "email_infrastructure".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![],
        },
        CheckResult {
            name: "email_transport".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![],
        },
        CheckResult {
            name: "email_brand_policy".to_string(),
            verdict: CheckVerdict::Skip,
            messages: vec![],
        },
    ])
}

fn email_auth_fail() -> SectionInput {
    no_error(vec![
        fail("email_authentication"),
        pass("email_infrastructure"),
        pass("email_transport"),
        pass("email_brand_policy"),
    ])
}

fn all_inputs_with_email(email: SectionInput) -> HashMap<String, SectionInput> {
    let profile = default_profile();
    let (dns, tls, ip) = all_pass(&profile);
    let mut map = inputs(dns, tls, ip);
    map.insert("email".to_string(), email);
    map
}

// SR1: Full-pass email section → A+, not_applicable empty, possible=22 (all four buckets)
#[test]
fn email_full_pass_scores_a_plus() {
    let profile = default_profile();
    let result = compute_score(&profile, &all_inputs_with_email(email_all_pass()));
    assert_eq!(
        result.grade, "A+",
        "all-pass including email should produce A+"
    );
    assert!(
        result.not_applicable.is_empty(),
        "not_applicable must be empty for all-Scored email section"
    );
    let email_score = &result.sections["email"];
    // email_authentication=10, infra=5, transport=5, brand=2 → possible=22
    assert_eq!(email_score.possible, 22);
    assert_eq!(email_score.earned, 22);
    assert!((email_score.percentage - 100.0).abs() < 0.001);
}

// SR2: No-MX domain → auth-only scoring (possible=10), no grade penalty
#[test]
fn email_no_mx_scored_on_auth_only() {
    let profile = default_profile();
    let result = compute_score(&profile, &all_inputs_with_email(email_no_mx()));
    assert!(
        result.not_applicable.is_empty(),
        "section is Scored (not NotApplicable) — not_applicable must be empty"
    );
    let email_score = &result.sections["email"];
    // Only email_authentication weight=10 is non-Skip; infra/transport/brand are Skip → excluded
    assert_eq!(
        email_score.possible, 10,
        "possible=10: authentication weight only"
    );
    assert_eq!(email_score.earned, 10);
    assert!((email_score.percentage - 100.0).abs() < 0.001);
    // Auth passes → no penalty → A+
    assert_eq!(
        result.grade, "A+",
        "no-MX domain with passing auth must not drop below A+"
    );
}

// SR3: Auth fail → grade drops via email section; dns section unaffected (auth is no longer in DNS)
#[test]
fn email_auth_fail_lowers_grade_not_via_dns() {
    let profile = default_profile();
    let result = compute_score(&profile, &all_inputs_with_email(email_auth_fail()));
    let email_score = &result.sections["email"];
    // auth fails (weight 10); infra(5)+transport(5)+brand(2)=12 earned out of 22
    assert_eq!(email_score.possible, 22);
    assert_eq!(email_score.earned, 12);
    // Grade must drop below A+ (54.5% for email section → overall ≈ 91.5%, grade A not A+)
    assert_ne!(
        result.grade, "A+",
        "auth fail must drop below A+, got {} (overall={:.1})",
        result.grade, result.overall_percentage,
    );
    // And the overall is measurably lower than 100%
    assert!(
        result.overall_percentage < 99.0,
        "overall must be below 99% when email auth fails, got {:.1}",
        result.overall_percentage,
    );
    // No hard-fail triggered (email hard_fail = [] in v1)
    assert!(
        !result.hard_fail_triggered,
        "email auth fail must not trigger hard_fail in v1"
    );
    // DNS section still 100% — auth is now in email backend
    let dns_score = &result.sections["dns"];
    assert!(
        (dns_score.percentage - 100.0).abs() < 0.001,
        "dns section must be 100% — auth moved to email section"
    );
}

// O1: Email backend errored → excluded from overall; not_applicable stays empty
#[test]
fn email_backend_errored_excluded_from_overall() {
    let profile = default_profile();
    let mut map = all_inputs_with_email(SectionInput {
        checks: vec![],
        status: SectionStatus::Errored,
    });
    let (dns, tls, ip) = all_pass(&profile);
    map.insert("dns".to_string(), dns);
    map.insert("tls".to_string(), tls);
    map.insert("ip".to_string(), ip);

    let result = compute_score(&profile, &map);
    assert!(
        !result.sections.contains_key("email"),
        "errored email must be absent from sections"
    );
    assert!(
        result.not_applicable.is_empty(),
        "errored section must not populate not_applicable"
    );
    // Remaining sections all pass → A+
    assert_eq!(result.grade, "A+");
    assert!(result.overall_percentage > 0.0);
}

// O2: Email NotApplicable → recorded in not_applicable; section absent from sections
#[test]
fn email_not_applicable_recorded_and_excluded() {
    let profile = default_profile();
    let mut map = all_inputs_with_email(SectionInput {
        checks: vec![],
        status: SectionStatus::NotApplicable {
            reason: "beacon timeout".to_string(),
        },
    });
    let (dns, tls, ip) = all_pass(&profile);
    map.insert("dns".to_string(), dns);
    map.insert("tls".to_string(), tls);
    map.insert("ip".to_string(), ip);

    let result = compute_score(&profile, &map);
    assert!(
        !result.sections.contains_key("email"),
        "N/A email must be absent from sections"
    );
    assert_eq!(
        result.not_applicable.get("email").map(|s| s.as_str()),
        Some("beacon timeout"),
        "not_applicable must record the reason"
    );
    assert_eq!(result.grade, "A+");
}
