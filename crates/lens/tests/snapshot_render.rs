/// Tests for snapshot HTML rendering.
use chrono::Utc;
use lens::config::SiteConfig;
use lens::snapshot::{render_snapshot_html, Snapshot, SnapshotAddress, SnapshotFinding, SnapshotSection};

fn make_snapshot() -> Snapshot {
    Snapshot {
        shortid: "ABCD1234".to_string(),
        domain: "example.com".to_string(),
        grade: "A".to_string(),
        score: 95.0,
        sections: vec![
            SnapshotSection {
                name: "DNS".to_string(),
                grade: "A".to_string(),
                passes: 3,
                warns: 1,
                fails: 0,
                skips: 0,
                findings: vec![
                    SnapshotFinding {
                        check_name: "dnssec".to_string(),
                        verdict: "pass".to_string(),
                        message: "DNSSEC is enabled".to_string(),
                        earned: 5,
                        possible: 5,
                        fix_hint: None,
                        fix_owner: None,
                        guide_url: None,
                    },
                    SnapshotFinding {
                        check_name: "caa".to_string(),
                        verdict: "warn".to_string(),
                        message: "CAA record missing".to_string(),
                        earned: 2,
                        possible: 5,
                        fix_hint: None,
                        fix_owner: None,
                        guide_url: None,
                    },
                ],
            },
            SnapshotSection {
                name: "TLS".to_string(),
                grade: "B".to_string(),
                passes: 4,
                warns: 0,
                fails: 1,
                skips: 0,
                findings: vec![SnapshotFinding {
                    check_name: "ech_advertised".to_string(),
                    verdict: "fail".to_string(),
                    message: "ECH not advertised".to_string(),
                    earned: 0,
                    possible: 3,
                    fix_hint: None,
                    fix_owner: None,
                    guide_url: None,
                }],
            },
        ],
        server_addresses: vec![],
        created_at: Utc::now(),
        lens_version: "0.9.1".to_string(),
    }
}

#[test]
fn renders_html_with_doctype() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.starts_with("<!DOCTYPE html>"), "must start with DOCTYPE");
    assert!(html.contains("<html"), "must contain html element");
}

#[test]
fn contains_domain_in_title_and_body() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("example.com"), "domain must appear in HTML");
}

#[test]
fn contains_overall_grade() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains(">A<"), "overall grade A must appear");
}

#[test]
fn contains_section_names() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("DNS"), "DNS section must appear");
    assert!(html.contains("TLS"), "TLS section must appear");
}

#[test]
fn contains_check_names() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    // Check names are rendered as human-readable labels
    assert!(html.contains("DNSSEC"), "human-readable label DNSSEC must appear");
    assert!(html.contains("CAA Records"), "human-readable label CAA Records must appear");
}

#[test]
fn contains_og_meta_tags() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("og:title"), "og:title must be present");
    assert!(html.contains("og:description"), "og:description must be present");
    assert!(html.contains("og:image"), "og:image must be present");
    assert!(html.contains("og:url"), "og:url must be present");
    assert!(html.contains("twitter:card"), "twitter:card must be present");
}

#[test]
fn og_image_uses_domain_path() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("/og/example.com.png"), "og:image must reference domain png");
}

#[test]
fn og_url_uses_shortid_path() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("/r/ABCD1234"), "og:url must reference shortid");
}

#[test]
fn no_external_fonts() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(!html.contains("fonts.googleapis.com"), "must not load Google Fonts");
    assert!(!html.contains("typekit"), "must not load Typekit");
}

#[test]
fn no_script_tags() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(!html.contains("<script"), "must not contain script tags");
}

#[test]
fn contains_rerun_link() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("Re-run this check"), "must contain re-run link");
    assert!(html.contains("/?d="), "re-run link must use ?d= param");
}

#[test]
fn og_description_contains_section_grades() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("DNS: A"), "og:description should include DNS grade");
    assert!(html.contains("TLS: B"), "og:description should include TLS grade");
}

#[test]
fn escapes_xss_in_domain() {
    let mut snap = make_snapshot();
    snap.domain = "<script>alert(1)</script>.example.com".to_string();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(!html.contains("<script>alert"), "must escape domain XSS");
    assert!(html.contains("&lt;script&gt;"), "must HTML-escape angle brackets");
}

#[test]
fn brand_name_from_site_config() {
    let snap = make_snapshot();
    let mut site = SiteConfig::default();
    site.brand_name = Some("MyBrand".to_string());
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("MyBrand"), "custom brand name must appear");
}

#[test]
fn error_section_gets_synthetic_finding() {
    let mut snap = make_snapshot();
    snap.sections.push(SnapshotSection {
        name: "Email".to_string(),
        grade: "error".to_string(),
        passes: 0,
        warns: 0,
        fails: 0,
        skips: 0,
        findings: vec![SnapshotFinding {
            check_name: "backend".to_string(),
            verdict: "fail".to_string(),
            message: "Section check failed — no details available".to_string(),
            earned: 0,
            possible: 0,
            fix_hint: None,
            fix_owner: None,
            guide_url: None,
        }],
    });
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("Email"), "error section must appear");
    assert!(html.contains("Section check failed"), "synthetic finding must appear");
}

#[test]
fn skipped_chip_shown_when_skips_present() {
    let mut snap = make_snapshot();
    snap.sections[0].skips = 3;
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("SKIPPED"), "skipped chip must appear when skips > 0");
}

#[test]
fn points_per_check_shown() {
    let snap = make_snapshot();
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("5/5"), "earned/possible score must appear");
    assert!(html.contains("2/5"), "partial earned/possible score must appear");
}

#[test]
fn section_order_is_canonical() {
    let mut snap = make_snapshot();
    // Add sections out of order
    snap.sections.push(SnapshotSection {
        name: "IP".to_string(),
        grade: "A+".to_string(),
        passes: 1,
        warns: 0,
        fails: 0,
        skips: 0,
        findings: vec![],
    });
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    // Canonical order: Email, HTTP, TLS, DNS, IP
    let tls_pos = html.find("section-name\">TLS").unwrap_or(usize::MAX);
    let dns_pos = html.find("section-name\">DNS").unwrap_or(usize::MAX);
    let ip_pos  = html.find("section-name\">IP").unwrap_or(usize::MAX);
    assert!(tls_pos < dns_pos, "TLS must appear before DNS");
    assert!(dns_pos < ip_pos, "DNS must appear before IP");
}

#[test]
fn server_addresses_rendered() {
    let mut snap = make_snapshot();
    snap.server_addresses = vec![
        SnapshotAddress { role: "HTTP".to_string(), ip: "1.2.3.4".to_string(), org: None },
        SnapshotAddress { role: "IP".to_string(), ip: "5.6.7.8".to_string(), org: Some("Example ISP".to_string()) },
    ];
    let site = SiteConfig::default();
    let html = render_snapshot_html(&snap, &site);
    assert!(html.contains("1.2.3.4"), "HTTP server IP must appear");
    assert!(html.contains("5.6.7.8"), "resolved IP must appear");
    assert!(html.contains("Example ISP"), "org must appear");
}
