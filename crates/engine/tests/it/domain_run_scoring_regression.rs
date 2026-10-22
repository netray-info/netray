//! Pure unit regression tests for the domain run's scoring through the engine (copied from
//! lens's `tests/scoring_regression.rs`, spec v2-cutover Phase 6; assertions unchanged).
//!
//! No network access. Tests use the engine's current profile (v4).
use std::collections::HashMap;

use netray_engine::domain_run::{
    BackendResult, CheckResult, CheckVerdict, SectionError, grade_label, score,
};
use netray_engine::profile::{Profile, Profiles};
use netray_model::Protocol;

type Section = Result<BackendResult, SectionError>;
type Sections = HashMap<String, Section>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_profile() -> &'static Profile {
    Profiles::current()
}

fn check(name: &str, verdict: CheckVerdict) -> CheckResult {
    CheckResult {
        name: name.to_string(),
        verdict,
        messages: vec![],
        evidence: vec![],
    }
}

fn pass(name: &str) -> CheckResult {
    check(name, CheckVerdict::Pass)
}

fn warn(name: &str) -> CheckResult {
    check(name, CheckVerdict::Warn)
}

fn fail(name: &str) -> CheckResult {
    check(name, CheckVerdict::Fail)
}

fn skip(name: &str) -> CheckResult {
    check(name, CheckVerdict::Skip)
}

fn no_error(checks: Vec<CheckResult>) -> Section {
    Ok(BackendResult {
        checks,
        evidence: vec![],
    })
}

/// The weights of one section of `profile`, by bare check name; empty for an unknown section
/// (lens's `score::section_weights`).
fn section_weights(profile: &Profile, section: &str) -> HashMap<String, u32> {
    [
        Protocol::Dns,
        Protocol::Tls,
        Protocol::Http,
        Protocol::Email,
        Protocol::Ip,
    ]
    .into_iter()
    .find(|p| p.as_str() == section)
    .and_then(|p| profile.sections.get(&p))
    .map(|s| {
        s.checks
            .iter()
            .map(|(id, w)| (id.name().to_string(), *w))
            .collect()
    })
    .unwrap_or_default()
}

fn errored() -> Section {
    Err(SectionError::BackendError("down".to_string()))
}

fn inputs(dns: Section, tls: Section, ip: Section) -> Sections {
    HashMap::from([
        ("dns".to_string(), dns),
        ("tls".to_string(), tls),
        ("ip".to_string(), ip),
    ])
}

/// Every weighted check of `section`, built by `f`.
fn all_of(profile: &Profile, section: &str, f: fn(&str) -> CheckResult) -> Vec<CheckResult> {
    section_weights(profile, section)
        .keys()
        .map(|k| f(k))
        .collect()
}

/// Build all-pass inputs from the profile's own check keys.
fn all_pass(profile: &Profile) -> (Section, Section, Section) {
    (
        no_error(all_of(profile, "dns", pass)),
        no_error(all_of(profile, "tls", pass)),
        no_error(all_of(profile, "ip", pass)),
    )
}

fn section_weight(profile: &Profile, name: &str) -> u32 {
    profile
        .sections
        .iter()
        .find(|(p, _)| p.as_str() == name)
        .map(|(_, s)| s.weight)
        .unwrap_or_else(|| panic!("section {name}"))
}

// ---------------------------------------------------------------------------
// 1. Current profile parses correctly
// ---------------------------------------------------------------------------

#[test]
fn default_profile_parses_correctly() {
    let profile = default_profile();

    // Basic structural checks
    assert_eq!(profile.name, "v5");
    assert_eq!(profile.version, 5);

    // Section weights are non-zero
    for section in ["dns", "tls", "ip"] {
        assert!(section_weight(profile, section) > 0);
    }

    // Grade thresholds include the expected grades
    let grades: Vec<&str> = profile
        .thresholds
        .iter()
        .map(|(g, _)| grade_label(*g))
        .collect();
    for g in ["A+", "A", "F"] {
        assert!(grades.contains(&g), "profile must define {g} threshold");
    }

    // Hard fail lists: tls has hard fails
    assert!(
        profile
            .sections
            .iter()
            .any(|(p, s)| p.as_str() == "tls" && !s.hard_fail.is_empty()),
        "tls hard_fail must not be empty"
    );

    // Each section has at least one weighted check
    for section in ["dns", "tls", "ip"] {
        assert!(
            !section_weights(profile, section).is_empty(),
            "{section} section must have weighted checks"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Profile structure: five sections, weights sum to 100, hsts/https_redirect
//    in HTTP, not TLS (AC-6, AC-7)
// ---------------------------------------------------------------------------

#[test]
fn profile_section_weights_sum_to_100() {
    let profile = default_profile();
    let sum: u32 = profile.sections.values().map(|s| s.weight).sum();
    assert_eq!(sum, 100, "section weights must sum to 100, got {}", sum);
    assert_eq!(section_weight(profile, "dns"), 20, "dns weight must be 20");
    assert_eq!(section_weight(profile, "tls"), 35, "tls weight must be 35");
    assert_eq!(
        section_weight(profile, "http"),
        20,
        "http weight must be 20"
    );
    assert_eq!(
        section_weight(profile, "email"),
        15,
        "email weight must be 15"
    );
    assert_eq!(section_weight(profile, "ip"), 10, "ip weight must be 10");
}

#[test]
fn tls_section_does_not_contain_hsts_or_https_redirect() {
    let profile = default_profile();
    let tls_checks = section_weights(profile, "tls");
    assert!(
        !tls_checks.contains_key("hsts"),
        "tls section must not contain hsts (moved to http)"
    );
    assert!(
        !tls_checks.contains_key("https_redirect"),
        "tls section must not contain https_redirect (moved to http)"
    );
    // Both checks are owned by the http section.
    let http_checks = section_weights(profile, "http");
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
    let (dns, tls, ip) = all_pass(profile);
    let result = score(profile, &inputs(dns, tls, ip));

    assert!(!result.hard_fail_triggered);
    assert_eq!(result.grade, "A+", "all-pass should produce A+");
    assert!(result.complete, "all sections scored → complete");
    assert!(
        (result.overall_percentage - 100.0).abs() < 0.001,
        "all-pass should yield 100% score, got {:.2}",
        result.overall_percentage,
    );
}

// ---------------------------------------------------------------------------
// 5. Expired cert (not_expired=fail) → F (hard fail)
// ---------------------------------------------------------------------------

/// All TLS checks pass but `failing`.
fn tls_all_pass_but(failing: &str) -> Section {
    let mut tls_checks = all_of(default_profile(), "tls", pass);
    for c in &mut tls_checks {
        if c.name == failing {
            c.verdict = CheckVerdict::Fail;
        }
    }
    no_error(tls_checks)
}

#[test]
fn expired_cert_triggers_hard_fail_f() {
    let profile = default_profile();
    let (dns, _, ip) = all_pass(profile);

    // not_expired fails — simulates an expired certificate
    let result = score(profile, &inputs(dns, tls_all_pass_but("not_expired"), ip));

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
    let (dns, _, ip) = all_pass(profile);

    // chain_trusted fails — simulates a self-signed or otherwise untrusted cert
    let result = score(profile, &inputs(dns, tls_all_pass_but("chain_trusted"), ip));

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

    let result = score(
        profile,
        &inputs(
            // All DNS checks at Warn (half credit each)
            no_error(all_of(profile, "dns", warn)),
            no_error(all_of(profile, "tls", pass)),
            no_error(all_of(profile, "ip", pass)),
        ),
    );

    // DNS section is 50% (all warn = exactly half credit).
    // TLS and IP sections are 100%.
    // HTTP and email absent from inputs → excluded from weighted average.
    // Active weights: dns=20, tls=35, ip=10, total=65.
    //   overall = (50*20 + 100*35 + 100*10) / 65 = (1000 + 3500 + 1000) / 65 ≈ 84.6
    // → grade B (75..89.9)
    assert!(
        (result.sections["dns"].percentage - 50.0).abs() < 0.001,
        "all-warn dns must be exactly 50%, got {:.2}",
        result.sections["dns"].percentage
    );
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
    let grade = lookup_grade_via_profile(profile, 97.0);
    assert_eq!(grade, "A+", "score 97.0 must yield A+");
}

#[test]
fn grade_boundary_96_9_is_a() {
    let profile = default_profile();
    let grade = lookup_grade_via_profile(profile, 96.9);
    assert_eq!(grade, "A", "score 96.9 must yield A");
}

/// Drive grade lookup through the public `score` API by crafting a single-section scenario
/// where overall_percentage equals the target.
///
/// Uses dns-only (tls + ip errored, absent from the synthetic profile). Two synthetic checks
/// weighted 1000 in total give precise fractional control: pass 970/1000 = 97.0%, etc.
fn lookup_grade_via_profile(profile: &Profile, target_percentage: f64) -> String {
    // earned = round(target * 10) out of 1000 → precise to 0.1%.
    let earned: u32 = (target_percentage * 10.0).round() as u32;
    let possible: u32 = 1000;

    // Two checks: one pass (earned weight), one fail (remainder).
    let fail_weight = possible - earned;

    let mut toml = format!(
        "[meta]\nname = \"synthetic\"\nversion = 3\n\n[sections.dns]\nweight = 1\n\n\
         [sections.dns.checks]\nsynthetic_pass = {earned}\n"
    );
    if fail_weight > 0 {
        toml.push_str(&format!("synthetic_fail = {fail_weight}\n"));
    }
    // Copy thresholds from the real profile.
    toml.push_str("\n[thresholds]\n");
    for (grade, val) in &profile.thresholds {
        toml.push_str(&format!("\"{}\" = {val}\n", grade_label(*grade)));
    }
    let synthetic_profile = Profile::from_toml(&toml).expect("synthetic profile parses");

    let mut dns_input_checks = vec![pass("synthetic_pass")];
    if fail_weight > 0 {
        dns_input_checks.push(fail("synthetic_fail"));
    }

    let result = score(
        &synthetic_profile,
        &inputs(no_error(dns_input_checks), errored(), errored()),
    );

    result.grade
}

// ---------------------------------------------------------------------------
// Email section scoring regression (SR1–SR3, O1–O2)
// ---------------------------------------------------------------------------

/// The email checks of V1's authentication bucket.
const AUTH: [&str; 3] = ["spf", "dkim", "dmarc"];
/// The email checks that need a receiving mail server.
const MX_DEPENDENT: [&str; 7] = [
    "mx", "mta_sts", "tls_rpt", "bimi", "dane", "dnsbl", "fcrdns",
];

fn email_all_pass() -> Section {
    no_error(all_of(default_profile(), "email", pass))
}

fn email_no_mx() -> Section {
    // Authentication passes; the MX-dependent checks are Skip (no MX records).
    let mut checks: Vec<CheckResult> = AUTH.iter().map(|n| pass(n)).collect();
    checks.extend(MX_DEPENDENT.iter().map(|n| skip(n)));
    no_error(checks)
}

fn email_auth_fail() -> Section {
    let mut checks: Vec<CheckResult> = AUTH.iter().map(|n| fail(n)).collect();
    checks.extend(MX_DEPENDENT.iter().map(|n| pass(n)));
    no_error(checks)
}

fn all_inputs_with_email(email: Section) -> Sections {
    let (dns, tls, ip) = all_pass(default_profile());
    let mut map = inputs(dns, tls, ip);
    map.insert("email".to_string(), email);
    map
}

// SR1: Full-pass email section → A+, not_applicable empty, possible=22 (all ten checks)
#[test]
fn email_full_pass_scores_a_plus() {
    let profile = default_profile();
    let result = score(profile, &all_inputs_with_email(email_all_pass()));
    assert_eq!(
        result.grade, "A+",
        "all-pass including email should produce A+"
    );
    assert!(
        result.not_applicable.is_empty(),
        "not_applicable must be empty for all-Scored email section"
    );
    let email_score = &result.sections["email"];
    // spf+dkim+dmarc=10, the seven MX-dependent checks=12 → possible=22
    assert_eq!(email_score.possible, 22);
    assert_eq!(email_score.earned, 22.0);
    assert!((email_score.percentage - 100.0).abs() < 0.001);
}

// SR2: No-MX domain → auth-only scoring (possible=10), no grade penalty
#[test]
fn email_no_mx_scored_on_auth_only() {
    let profile = default_profile();
    let result = score(profile, &all_inputs_with_email(email_no_mx()));
    assert!(
        result.not_applicable.is_empty(),
        "section is Scored (not NotApplicable) — not_applicable must be empty"
    );
    let email_score = &result.sections["email"];
    // Only spf+dkim+dmarc (3+3+4) are non-Skip; the MX-dependent checks are excluded
    assert_eq!(
        email_score.possible, 10,
        "possible=10: authentication weights only"
    );
    assert_eq!(email_score.earned, 10.0);
    assert!((email_score.percentage - 100.0).abs() < 0.001);
    // Auth passes → no penalty → A+
    assert_eq!(
        result.grade, "A+",
        "no-MX domain with passing auth must not drop below A+"
    );
}

// SR3: Auth fail → grade drops via email section; dns section unaffected (auth is not in DNS)
#[test]
fn email_auth_fail_lowers_grade_not_via_dns() {
    let profile = default_profile();
    let result = score(profile, &all_inputs_with_email(email_auth_fail()));
    let email_score = &result.sections["email"];
    // spf, dkim, dmarc fail (10); the MX-dependent checks earn 12 of 22
    assert_eq!(email_score.possible, 22);
    assert_eq!(email_score.earned, 12.0);
    // Grade must drop below A+ (54.5% for email section → overall ≈ 93.2%, grade A not A+)
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
    // No hard-fail triggered (email hard_fail = [])
    assert!(
        !result.hard_fail_triggered,
        "email auth fail must not trigger hard_fail"
    );
    // DNS section still 100% — auth lives in the email section
    let dns_score = &result.sections["dns"];
    assert!(
        (dns_score.percentage - 100.0).abs() < 0.001,
        "dns section must be 100% — auth lives in the email section"
    );
}

// O1: Email backend errored → grade is "incomplete", never a clean A+; not_applicable stays empty
#[test]
fn email_backend_errored_makes_grade_incomplete() {
    let profile = default_profile();
    let result = score(profile, &all_inputs_with_email(errored()));
    assert!(
        !result.sections.contains_key("email"),
        "errored email must be absent from sections"
    );
    assert!(
        result.not_applicable.is_empty(),
        "errored section must not populate not_applicable"
    );
    assert_eq!(
        result.grade, "incomplete",
        "an errored section must not be graded on the remaining sections"
    );
    assert!(
        !result.complete,
        "errored section makes the result incomplete"
    );
}

// C4 (engine): a Scored section whose checks are all Skip (possible == 0) is incomplete
#[test]
fn scored_section_with_all_skip_checks_is_incomplete() {
    let email = no_error(all_of(default_profile(), "email", skip));
    let profile = default_profile();
    let result = score(profile, &all_inputs_with_email(email));
    assert_eq!(result.grade, "incomplete");
    assert!(!result.complete);
}

// C1 (engine): a timed-out section reaches the engine as incomplete; dns timed out → incomplete
#[test]
fn errored_dns_section_makes_grade_incomplete() {
    let profile = default_profile();
    let (_, tls, ip) = all_pass(profile);
    let result = score(profile, &inputs(Err(SectionError::Timeout), tls, ip));
    assert_eq!(result.grade, "incomplete");
    assert!(!result.complete);
}

// O2: Email NotApplicable → recorded in not_applicable; section absent from sections
#[test]
fn email_not_applicable_recorded_and_excluded() {
    let profile = default_profile();
    let result = score(
        profile,
        &all_inputs_with_email(Err(SectionError::NotApplicable {
            reason: "beacon timeout".to_string(),
        })),
    );
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
    assert!(
        result.complete,
        "NotApplicable is excluded without making the result incomplete"
    );
}
