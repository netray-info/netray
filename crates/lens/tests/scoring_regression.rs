/// Pure unit regression tests for the scoring engine.
///
/// No network access. Tests use the embedded default profile.
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

/// Build all-pass inputs from the profile's own check keys.
fn all_pass(profile: &ScoringProfile) -> (SectionInput, SectionInput, SectionInput) {
    let dns = no_error(profile.dns.keys().map(|k| pass(k)).collect());
    let tls = no_error(profile.tls.keys().map(|k| pass(k)).collect());
    let ip = no_error(profile.ip.keys().map(|k| pass(k)).collect());
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
    assert_eq!(profile.meta.version, 1);

    // Section weights are non-zero
    assert!(profile.section_weights.dns > 0);
    assert!(profile.section_weights.tls > 0);
    assert!(profile.section_weights.ip > 0);

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
        !profile.hard_fail.tls.is_empty(),
        "tls hard_fail must not be empty"
    );
    assert!(
        !profile.hard_fail.dns.is_empty(),
        "dns hard_fail must not be empty"
    );

    // Each section has at least one weighted check
    assert!(
        !profile.dns.is_empty(),
        "dns section must have weighted checks"
    );
    assert!(
        !profile.tls.is_empty(),
        "tls section must have weighted checks"
    );
    assert!(
        !profile.ip.is_empty(),
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
    let result = compute_score(&profile, dns, tls, ip);

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
    let result = compute_score(&profile, dns, tls, ip);

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
    let result = compute_score(&profile, dns, tls, ip);

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
    let mut tls_checks: Vec<CheckResult> = profile.tls.keys().map(|k| pass(k)).collect();
    for c in &mut tls_checks {
        if c.name == "not_expired" {
            c.verdict = CheckVerdict::Fail;
        }
    }
    let result = compute_score(&profile, dns, no_error(tls_checks), ip);

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
    let mut tls_checks: Vec<CheckResult> = profile.tls.keys().map(|k| pass(k)).collect();
    for c in &mut tls_checks {
        if c.name == "chain_trusted" {
            c.verdict = CheckVerdict::Fail;
        }
    }
    let result = compute_score(&profile, dns, no_error(tls_checks), ip);

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
    let dns_checks: Vec<CheckResult> = profile.dns.keys().map(|k| warn(k)).collect();
    // All TLS checks pass
    let tls_checks: Vec<CheckResult> = profile.tls.keys().map(|k| pass(k)).collect();
    // All IP checks pass
    let ip_checks: Vec<CheckResult> = profile.ip.keys().map(|k| pass(k)).collect();

    let result = compute_score(
        &profile,
        no_error(dns_checks),
        no_error(tls_checks),
        no_error(ip_checks),
    );

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

    // Construct a score that yields exactly 97.0% using errored sections so we
    // can inject a precise overall_percentage without fighting rounding.
    // We use a minimal single-section scenario:
    //   dns-only, all others errored, dns section must produce 97% by weight.
    //
    // But easier: use a custom profile-agnostic approach by overriding via the
    // lookup_grade function directly.  Since it's private, we go through
    // compute_score with crafted inputs that produce ~97%.
    //
    // With dns=35, tls=45, ip=20 and only DNS active (others errored):
    //   overall = dns_percentage * 35 / 35 = dns_percentage
    // So we need dns_percentage = 97.0.
    //
    // dns has e.g. spf=10, dmarc=10 as the main weights. Total dns possible
    // depends on which checks we include. Use a minimal crafted set:
    //   pass 97 points, fail 3 points out of 100.
    // But dns check weights don't sum to 100. Instead, use all-pass minus one
    // small check set to warn to get close.
    //
    // Simplest: use errored tls/ip, craft dns to produce exactly 97%.
    // dns weights from profile: sum them all, then fail the right amount.
    let _total_dns_weight: u32 = profile.dns.values().sum();
    // We need earned/total = 0.97 → earned = total * 0.97.
    // Use all pass except fail checks totalling (total * 0.03) weight.
    // target_fail_weight = ceil(total * 0.03) to stay at or above 97%.
    // Actually, let's find the exact threshold: score 97 → A+.
    // Rather than crafting exact weights, assert via thresholds directly.
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
///
/// Since the profile's dns map won't contain "synthetic", all real dns checks are
/// excluded from scoring (unweighted). We use a one-off profile for this.
fn lookup_grade_via_profile(profile: &ScoringProfile, target_percentage: f64) -> String {
    use lens::scoring::profile::{HardFail, ProfileMeta, ScoringProfile, SectionWeights};
    use std::collections::BTreeMap;

    // Build a minimal profile with a single dns check weighted 1000.
    // earned = round(target * 10) out of 1000 → precise to 0.1%.
    let earned: u32 = (target_percentage * 10.0).round() as u32;
    let possible: u32 = 1000;

    // Two checks: one pass (earned weight), one fail (remainder).
    let fail_weight = possible - earned;

    let mut dns_weights = std::collections::HashMap::new();
    dns_weights.insert("synthetic_pass".to_string(), earned);
    if fail_weight > 0 {
        dns_weights.insert("synthetic_fail".to_string(), fail_weight);
    }

    let mut thresholds = BTreeMap::new();
    // Copy thresholds from the real profile.
    for (grade, val) in &profile.thresholds {
        thresholds.insert(grade.clone(), *val);
    }

    let synthetic_profile = ScoringProfile {
        meta: ProfileMeta {
            name: "synthetic".to_string(),
            version: 0,
        },
        dns: dns_weights,
        tls: std::collections::HashMap::new(),
        ip: std::collections::HashMap::new(),
        section_weights: SectionWeights {
            dns: 1,
            tls: 1,
            ip: 1,
        },
        thresholds,
        hard_fail: HardFail::default(),
    };

    let mut dns_checks = vec![pass("synthetic_pass")];
    if fail_weight > 0 {
        dns_checks.push(fail("synthetic_fail"));
    }

    let result = compute_score(
        &synthetic_profile,
        no_error(dns_checks),
        errored(),
        errored(),
    );

    result.grade
}
