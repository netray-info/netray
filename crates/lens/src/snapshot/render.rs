use crate::badge::palette::color_for_grade;
use crate::config::SiteConfig;
use crate::snapshot::types::Snapshot;

/// Canonical section display order (matches SPA visual order).
const SECTION_ORDER: &[&str] = &["Email", "HTTP", "TLS", "DNS", "IP"];

fn section_rank(name: &str) -> usize {
    SECTION_ORDER
        .iter()
        .position(|&s| s.eq_ignore_ascii_case(name))
        .unwrap_or(99)
}

/// Render a snapshot as a self-contained HTML page.
pub fn render_snapshot_html(snap: &Snapshot, site: &SiteConfig) -> String {
    let brand_name = site.brand_name.as_deref().unwrap_or("lens");
    let brand_tagline = site.brand_tagline.as_deref().unwrap_or("");
    let footer_about = site.footer_about.as_deref().unwrap_or("");
    let footer_links = site.footer_links.as_deref().unwrap_or(&[]);

    let grade_color = color_for_grade(&snap.grade);
    let domain = html_escape(&snap.domain);
    let lens_version = html_escape(&snap.lens_version);
    let checked_at = snap.created_at.format("%Y-%m-%d %H:%M UTC").to_string();

    // Sort sections in canonical order
    let mut sections = snap.sections.clone();
    sections.sort_by_key(|s| section_rank(&s.name));

    // Totals across all sections
    let total_pass: u32 = sections.iter().map(|s| s.passes).sum();
    let total_warn: u32 = sections.iter().map(|s| s.warns).sum();
    let total_fail: u32 = sections.iter().map(|s| s.fails).sum();
    let total_skip: u32 = sections.iter().map(|s| s.skips).sum();

    let score_pct = if snap.score > 0.0 {
        format!("{:.1}%", snap.score)
    } else {
        String::new()
    };

    let descriptor = grade_descriptor(&snap.grade);
    let count_headline = count_headline(total_warn, total_fail);

    // OG description
    let og_description = sections
        .iter()
        .map(|s| format!("{}: {}", html_escape(&s.name), html_escape(&s.grade)))
        .collect::<Vec<_>>()
        .join(" | ");

    let og_title = html_escape(&format!("{} — {} | {brand_name}", snap.domain, snap.grade));
    let og_image = html_escape(&format!("/og/{}.png", snap.domain));
    let og_url = html_escape(&format!("/r/{}", snap.shortid));

    // Section grade dots row (like SPA summary dots)
    let section_dots_html: String = sections
        .iter()
        .map(|s| {
            let color = color_for_grade(&s.grade);
            let name = html_escape(&s.name);
            let grade = html_escape(&s.grade);
            format!(
                r#"<span class="sdot"><span class="sdot__dot" style="background:{color}"></span><span class="sdot__name">{name}</span><span class="sdot__grade" style="color:{color}">{grade}</span></span>"#
            )
        })
        .collect();

    // Build sections HTML
    let sections_html: String = sections.iter().map(|section| {
        let sec_color = color_for_grade(&section.grade);
        let sec_name = html_escape(&section.name);
        let sec_grade = html_escape(&section.grade);

        // Issue tags in section header: "! Security Headers ! Header Hygiene"
        let issue_tags: String = section.findings.iter()
            .filter(|f| f.verdict == "warn" || f.verdict == "fail")
            .map(|f| {
                let label = html_escape(check_label(&f.check_name));
                let class = if f.verdict == "fail" { "issue-tag issue-tag--fail" } else { "issue-tag issue-tag--warn" };
                format!(r#"<span class="{class}">! {label}</span>"#)
            })
            .collect();

        let findings_html: String = section.findings.iter().map(|f| {
            let (sym, sym_class, name_class) = match f.verdict.as_str() {
                "pass" => ("✓", "verdict-sym--pass", "check-name--pass"),
                "warn" => ("!", "verdict-sym--warn", "check-name--warn"),
                "fail" => ("!", "verdict-sym--fail", "check-name--fail"),
                _      => ("—", "verdict-sym--skip", "check-name--skip"),
            };
            let label = html_escape(check_label(&f.check_name));

            // Prefer stored guide_url; fall back to static mapping by check name.
            let resolved_guide_url = f.guide_url.as_deref().or_else(|| guide_url_for(&f.check_name));
            let learn_more_html = if let Some(url) = resolved_guide_url {
                let url = html_escape(url);
                format!(r#"<a class="learn-more" href="{url}" target="_blank" rel="noopener noreferrer" title="Learn more">?↗</a>"#)
            } else {
                String::new()
            };

            let score_html = if f.possible > 0 {
                format!(r#"<span class="check-score">{}/{}</span>"#, f.earned, f.possible)
            } else {
                String::new()
            };

            let message = html_escape(&f.message);
            let msg_html = if !f.message.is_empty() {
                format!(r#"<div class="finding-msg">{message}</div>"#)
            } else {
                String::new()
            };

            format!(
                r#"<div class="finding"><div class="finding-row"><span class="verdict-sym {sym_class}">{sym}</span><span class="check-name {name_class}">{label}</span>{learn_more_html}<span class="finding-spacer"></span>{score_html}</div>{msg_html}</div>"#
            )
        }).collect();

        format!(
            r#"<div class="section-card">
  <div class="section-header">
    <div class="section-header__left">
      <span class="section-grade-badge" style="background:{sec_color}">{sec_grade}</span>
      <span class="section-name">{sec_name}</span>
      {issue_tags}
    </div>
    <div class="section-counts">
      <span class="count-pass">{} pass</span>
      <span class="count-sep"> · </span>
      <span class="count-warn">{} warn</span>
      <span class="count-sep"> · </span>
      <span class="count-fail">{} fail</span>
    </div>
  </div>
  <div class="findings">{findings_html}</div>
</div>"#,
            section.passes, section.warns, section.fails
        )
    }).collect();

    // Server addresses block (like SPA's server info row)
    let server_addresses_html = if snap.server_addresses.is_empty() {
        String::new()
    } else {
        let rows: String = snap.server_addresses.iter().map(|a| {
            let role = html_escape(&a.role);
            let ip = html_escape(&a.ip);
            let org_span = a.org.as_deref()
                .map(|o| format!(r#"<span class="addr-org">Hosted by {}</span>"#, html_escape(o)))
                .unwrap_or_default();
            format!(r#"<div class="addr-row"><span class="addr-role">{role}</span><span class="addr-ip">{ip}</span>{org_span}</div>"#)
        }).collect();
        format!(r#"<div class="server-addrs">{rows}</div>"#)
    };

    // Build footer links HTML
    let footer_links_html: String = footer_links
        .iter()
        .map(|link| {
            let label = html_escape(&link.label);
            let href = html_escape(&link.href);
            if link.external {
                format!(r#"<a href="{href}" target="_blank" rel="noopener noreferrer">{label}</a>"#)
            } else {
                format!(r#"<a href="{href}">{label}</a>"#)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");

    let brand_tagline_html = if brand_tagline.is_empty() {
        String::new()
    } else {
        format!(r#"<span class="brand-tagline">{}</span>"#, html_escape(brand_tagline))
    };

    let footer_about_html = if footer_about.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="footer-about">{}</p>"#, html_escape(footer_about))
    };

    let grade = html_escape(&snap.grade);

    // Chips
    let mut chips = String::new();
    if total_fail > 0 {
        chips.push_str(&format!(r#"<span class="chip chip--fail">{total_fail} FAILED</span>"#));
    }
    if total_warn > 0 {
        chips.push_str(&format!(r#"<span class="chip chip--warn">{total_warn} WARNINGS</span>"#));
    }
    if total_pass > 0 {
        chips.push_str(&format!(r#"<span class="chip chip--pass">{total_pass} PASSED</span>"#));
    }
    if total_skip > 0 {
        chips.push_str(&format!(r#"<span class="chip chip--skip">{total_skip} SKIPPED</span>"#));
    }

    let descriptor_html = if let Some((desc, meaning)) = descriptor {
        format!(r#"<div class="summary-descriptor"><strong>{desc}</strong> — {meaning}</div>"#)
    } else {
        String::new()
    };

    let headline_html = if count_headline.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="summary-headline">{count_headline}</div>"#)
    };

    // Score + dots line
    let score_dots_html = if score_pct.is_empty() {
        format!(r#"<div class="summary-row summary-dots">{section_dots_html}</div>"#)
    } else {
        format!(
            r#"<div class="summary-row summary-dots"><span class="summary-score">{score_pct}</span>{section_dots_html}</div>"#
        )
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{og_title}</title>
  <meta name="description" content="{og_description}">
  <meta property="og:title" content="{og_title}">
  <meta property="og:description" content="{og_description}">
  <meta property="og:image" content="{og_image}">
  <meta property="og:url" content="{og_url}">
  <meta name="twitter:card" content="summary_large_image">
  <style>
    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: #0a0f1e;
      color: #dce6f5;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
      min-height: 100vh;
      padding: 1.25rem 1rem 3rem;
    }}
    a {{ color: #60a5fa; text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .page {{ max-width: 860px; margin: 0 auto; }}

    /* ── Nav ── */
    .site-nav {{
      display: flex;
      align-items: baseline;
      gap: 0.75rem;
      margin-bottom: 1.5rem;
      padding-bottom: 0.875rem;
      border-bottom: 1px solid #1a2540;
    }}
    .brand-name {{
      font-size: 1.375rem;
      font-weight: 800;
      color: #dce6f5;
      letter-spacing: -0.02em;
    }}
    .brand-tagline {{ font-size: 0.8rem; color: #3d5278; }}

    /* ── Summary card ── */
    .summary-card {{
      background: #101827;
      border: 1px solid #1a2844;
      border-radius: 0.625rem;
      padding: 1.125rem 1.5rem;
      margin-bottom: 1.25rem;
    }}
    .summary-top {{
      display: flex;
      align-items: stretch;
      gap: 1.25rem;
    }}
    .grade-letter {{
      font-size: 5.5rem;
      font-weight: 800;
      line-height: 1;
      flex-shrink: 0;
      letter-spacing: -0.03em;
      display: flex;
      align-items: center;
      min-width: 4rem;
    }}
    .summary-meta {{
      flex: 1;
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: 0.3rem;
    }}
    .summary-domain {{
      font-size: 1.25rem;
      font-weight: 700;
      word-break: break-all;
      line-height: 1.2;
    }}
    .summary-row {{
      display: flex;
      align-items: center;
      gap: 0.5rem;
      flex-wrap: wrap;
    }}
    .summary-score {{
      font-size: 1.375rem;
      font-weight: 700;
      color: #dce6f5;
    }}
    .sdot {{
      display: inline-flex;
      align-items: center;
      gap: 0.3rem;
      font-size: 0.8125rem;
    }}
    .sdot__dot {{
      width: 0.5rem;
      height: 0.5rem;
      border-radius: 50%;
      flex-shrink: 0;
    }}
    .sdot__name {{
      color: #5a6f94;
      text-transform: uppercase;
      font-size: 0.75rem;
      letter-spacing: 0.04em;
    }}
    .sdot__grade {{ font-weight: 700; font-size: 0.8125rem; }}
    .summary-descriptor {{ font-size: 0.9rem; color: #94afd4; }}
    .summary-descriptor strong {{ color: #dce6f5; font-weight: 600; }}
    .summary-headline {{ font-size: 0.9rem; color: #94afd4; }}
    .summary-chips {{
      margin-top: 0.75rem;
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
    }}
    .chip {{
      display: inline-flex;
      align-items: center;
      padding: 0.25rem 0.625rem;
      border-radius: 9999px;
      font-size: 0.75rem;
      font-weight: 600;
      letter-spacing: 0.03em;
    }}
    .chip--warn {{ background: rgba(245,158,11,0.15); color: #f59e0b; border: 1px solid rgba(245,158,11,0.3); }}
    .chip--pass {{ background: rgba(34,197,94,0.12); color: #22c55e; border: 1px solid rgba(34,197,94,0.25); }}
    .chip--skip {{ background: rgba(100,116,139,0.12); color: #64748b; border: 1px solid rgba(100,116,139,0.25); }}
    .chip--fail {{ background: rgba(239,68,68,0.12); color: #ef4444; border: 1px solid rgba(239,68,68,0.25); }}
    .summary-checked {{ margin-top: 0.5rem; font-size: 0.775rem; color: #3d5278; }}

    /* ── Server addresses ── */
    .server-addrs {{
      margin-top: 0.875rem;
      padding-top: 0.75rem;
      border-top: 1px solid #1a2540;
      display: flex;
      flex-direction: column;
      gap: 0.25rem;
    }}
    .addr-row {{
      display: flex;
      align-items: center;
      gap: 0.75rem;
      font-size: 0.8125rem;
    }}
    .addr-role {{ color: #3d5278; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; min-width: 2.5rem; }}
    .addr-ip {{ font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; color: #94afd4; font-weight: 600; }}
    .addr-org {{ color: #5a6f94; font-size: 0.75rem; }}

    /* ── Section cards ── */
    .section-card {{
      background: #101827;
      border: 1px solid #1a2844;
      border-radius: 0.5rem;
      margin-bottom: 0.625rem;
      overflow: hidden;
    }}
    .section-header {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0.625rem 1rem;
      background: #0d1627;
      gap: 0.75rem;
      flex-wrap: wrap;
    }}
    .section-header__left {{
      display: flex;
      align-items: center;
      gap: 0.5rem;
      flex-wrap: wrap;
    }}
    .section-grade-badge {{
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 1.875rem;
      height: 1.875rem;
      border-radius: 50%;
      font-size: 0.625rem;
      font-weight: 800;
      color: #fff;
      flex-shrink: 0;
    }}
    .section-name {{
      font-weight: 700;
      font-size: 0.9375rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #dce6f5;
    }}
    .issue-tag {{
      font-size: 0.7rem;
      font-weight: 600;
      font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace;
      padding: 0.1rem 0.35rem;
      border-radius: 0.2rem;
    }}
    .issue-tag--warn {{ color: #f59e0b; }}
    .issue-tag--fail {{ color: #ef4444; }}
    .section-counts {{ font-size: 0.8rem; white-space: nowrap; }}
    .count-pass {{ color: #22c55e; }}
    .count-warn {{ color: #f59e0b; }}
    .count-fail {{ color: #ef4444; }}
    .count-sep {{ color: #1e2d4a; }}

    /* ── Findings ── */
    .findings {{ font-size: 0.875rem; }}
    .finding {{ padding: 0.45rem 1rem; }}
    .finding + .finding {{ border-top: 1px solid #111d30; }}
    .finding-row {{
      display: flex;
      align-items: center;
      gap: 0.4rem;
    }}
    .verdict-sym {{
      font-size: 0.9rem;
      font-weight: 700;
      flex-shrink: 0;
      width: 1rem;
      text-align: center;
    }}
    .verdict-sym--pass {{ color: #22c55e; }}
    .verdict-sym--warn {{ color: #f59e0b; }}
    .verdict-sym--fail {{ color: #ef4444; }}
    .verdict-sym--skip {{ color: #3d5278; }}
    .check-name {{ font-weight: 500; cursor: default; font-size: 0.875rem; }}
    .check-name--pass {{ color: #7b90b5; }}
    .check-name--warn {{ color: #f59e0b; }}
    .check-name--fail {{ color: #ef4444; }}
    .check-name--skip {{ color: #3d5278; }}
    .learn-more {{ color: #3d5278; font-size: 0.75rem; flex-shrink: 0; text-decoration: none; }}
    .learn-more:hover {{ color: #60a5fa; text-decoration: none; }}
    .finding-spacer {{ flex: 1; }}
    .check-score {{
      font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace;
      font-size: 0.775rem;
      color: #3d5278;
      background: #0d1627;
      border-radius: 0.25rem;
      padding: 0.1rem 0.4rem;
      white-space: nowrap;
      flex-shrink: 0;
    }}
    .finding-msg {{
      color: #5a6f94;
      font-size: 0.8125rem;
      margin-left: 1.4rem;
      margin-top: 0.15rem;
    }}

    /* ── Footer ── */
    .page-footer {{
      margin-top: 1.75rem;
      padding-top: 0.875rem;
      border-top: 1px solid #111d30;
      font-size: 0.775rem;
      color: #3d5278;
      display: flex;
      flex-wrap: wrap;
      align-items: baseline;
      gap: 0.375rem 1.5rem;
    }}
    .footer-about {{ flex-basis: 100%; color: #5a6f94; }}
    .footer-links {{ display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; }}
    .rerun-link {{ margin-left: auto; }}
  </style>
</head>
<body>
<div class="page">
  <nav class="site-nav">
    <a class="brand-name" href="/">{brand_name}</a>
    {brand_tagline_html}
  </nav>

  <div class="summary-card">
    <div class="summary-top">
      <span class="grade-letter" style="color:{grade_color}">{grade}</span>
      <div class="summary-meta">
        <div class="summary-domain">{domain}</div>
        {score_dots_html}
        {descriptor_html}
        {headline_html}
      </div>
    </div>
    <div class="summary-chips">{chips}</div>
    {server_addresses_html}
    <div class="summary-checked">Checked {checked_at}</div>
  </div>

  {sections_html}

  <footer class="page-footer">
    {footer_about_html}
    <div class="footer-links">{footer_links_html}</div>
    <span>{brand_name} {lens_version}</span>
    <span class="rerun-link"><a href="/?d={domain}">Re-run this check ↗</a></span>
  </footer>
</div>
</body>
</html>"#,
        og_title = og_title,
        og_description = og_description,
        og_image = og_image,
        og_url = og_url,
        brand_name = brand_name,
        brand_tagline_html = brand_tagline_html,
        domain = domain,
        grade_color = grade_color,
        grade = grade,
        score_dots_html = score_dots_html,
        descriptor_html = descriptor_html,
        headline_html = headline_html,
        chips = chips,
        server_addresses_html = server_addresses_html,
        checked_at = checked_at,
        sections_html = sections_html,
        footer_about_html = footer_about_html,
        footer_links_html = footer_links_html,
        lens_version = lens_version,
    )
}

fn grade_descriptor(grade: &str) -> Option<(&'static str, &'static str)> {
    match grade {
        "A+" => Some(("perfect", "no findings")),
        "A"  => Some(("strong", "minor polish possible")),
        "B"  => Some(("ok", "weaknesses worth fixing")),
        "C"  => Some(("fair", "significant issues")),
        "D"  => Some(("weak", "serious problems")),
        "F"  => Some(("failing", "critical failures")),
        _    => None,
    }
}

fn count_headline(total_warn: u32, total_fail: u32) -> String {
    match (total_fail, total_warn) {
        (0, 0) => "Everything checks out".to_string(),
        (0, w) => format!("{w} {} worth a look", if w == 1 { "warning" } else { "warnings" }),
        (f, 0) => format!("{f} {} to fix", if f == 1 { "thing" } else { "things" }),
        (f, w) => format!(
            "{f} {} to fix and {w} {} worth a look",
            if f == 1 { "thing" } else { "things" },
            if w == 1 { "warning" } else { "warnings" }
        ),
    }
}

fn check_label(name: &str) -> &str {
    match name {
        "spf"                  => "SPF Record",
        "dmarc"                => "DMARC Policy",
        "dnssec"               => "DNSSEC",
        "caa"                  => "CAA Records",
        "mx"                   => "MX Records",
        "ns"                   => "Nameservers",
        "ns_lame"              => "Lame Delegation",
        "ns_delegation"        => "NS Delegation",
        "dkim"                 => "DKIM",
        "mta_sts"              => "MTA-STS",
        "tlsrpt"               => "SMTP TLS Reporting",
        "bimi"                 => "BIMI",
        "cname_apex"           => "CNAME at Apex",
        "https_svcb"           => "HTTPS Record",
        "ttl"                  => "TTL Consistency",
        "dnskey_algorithm"     => "DNSKEY Algorithm",
        "dnssec_rollover"      => "DNSSEC Key Rollover",
        "infrastructure"       => "Infrastructure",
        "chain_trusted"        => "Chain of Trust",
        "not_expired"          => "Not Expired",
        "hostname_match"       => "Hostname Match",
        "chain_complete"       => "Chain Complete",
        "strong_signature"     => "Signature Algorithm",
        "key_strength"         => "Key Strength",
        "expiry_window"        => "Expiry Window",
        "cert_lifetime"        => "Certificate Lifetime",
        "san_quality"          => "SAN Quality",
        "aia_reachability"     => "AIA Reachability",
        "tls_version"          => "TLS Version",
        "forward_secrecy"      => "Forward Secrecy",
        "aead_cipher"          => "AEAD Cipher",
        "ocsp_stapled"         => "OCSP Stapling",
        "ct_logged"            => "CT Logged",
        "caa_compliant"        => "CAA Compliance",
        "dane_valid"           => "DANE / TLSA",
        "consistency"          => "Multi-IP Consistency",
        "alpn_consistency"     => "ALPN Consistency",
        "ech_advertised"       => "Encrypted Client Hello",
        "hsts"                 => "HSTS",
        "https_redirect"       => "HTTPS Redirect",
        "security_headers"     => "Security Headers",
        "cors"                 => "CORS Policy",
        "cookie_secure"        => "Secure Cookies",
        "hygiene"              => "Header Hygiene",
        "email_authentication" => "Authentication",
        "email_infrastructure" => "Infrastructure",
        "email_transport"      => "Transport",
        "email_brand_policy"   => "Brand Policy",
        "reputation"           => "IP Reputation",
        other                  => other,
    }
}

fn check_description(name: &str) -> &str {
    match name {
        "spf"                  => "SPF authorises which servers may send email for this domain",
        "dmarc"                => "DMARC instructs receivers how to handle unauthenticated messages",
        "dnssec"               => "DNSSEC cryptographically signs DNS records, preventing cache poisoning",
        "caa"                  => "CAA restricts which Certificate Authorities may issue certificates",
        "mx"                   => "MX records must agree between recursive and authoritative resolvers",
        "ns"                   => "At least two NS records ensure availability if one nameserver fails",
        "ns_lame"              => "All NS records should respond authoritatively for the zone",
        "ns_delegation"        => "Parent and child NS records must be consistent",
        "dkim"                 => "DKIM signs outgoing mail to prove it hasn't been tampered with",
        "mta_sts"              => "MTA-STS enforces TLS for inbound SMTP connections",
        "tlsrpt"               => "SMTP TLS Reporting provides visibility into delivery failures",
        "bimi"                 => "BIMI displays a brand logo in supporting email clients",
        "cname_apex"           => "A CNAME at the zone apex breaks MX, NS and other records",
        "https_svcb"           => "HTTPS DNS records enable HTTP/3 and Encrypted Client Hello",
        "ttl"                  => "Inconsistent TTLs across record types can cause caching problems",
        "dnskey_algorithm"     => "Deprecated DNSSEC algorithms (RSA/MD5, RSA/SHA-1) must be replaced",
        "dnssec_rollover"      => "DNSSEC key rollover must be clean — no orphaned DS or duplicate KSKs",
        "chain_trusted"        => "The certificate chain must verify to a trusted root CA",
        "not_expired"          => "All certificates in the chain must be within their validity period",
        "hostname_match"       => "The leaf certificate SAN must cover the queried hostname",
        "chain_complete"       => "All intermediate certificates must be present in correct order",
        "strong_signature"     => "SHA-1 and MD5 signatures are deprecated and must not appear",
        "key_strength"         => "RSA keys must be ≥ 2048 bits; ECDSA must use P-256 or better",
        "expiry_window"        => "Certificate expires within 30 days (warn) or 7 days (fail)",
        "cert_lifetime"        => "CA/Browser Forum requires certificates valid for ≤ 398 days",
        "san_quality"          => "The SAN list should be reasonable in size and not overly broad",
        "aia_reachability"     => "AIA CA Issuers URL must be reachable when the chain is incomplete",
        "tls_version"          => "TLS 1.3 is preferred; TLS 1.2 acceptable; older versions must not be offered",
        "forward_secrecy"      => "Ephemeral key exchange protects past sessions if the key is compromised",
        "aead_cipher"          => "AEAD ciphers (GCM, ChaCha20-Poly1305) provide authenticated encryption",
        "ocsp_stapled"         => "OCSP stapling avoids a separate revocation check by the browser",
        "ct_logged"            => "Certificate Transparency requires ≥ 2 SCTs for browser trust",
        "caa_compliant"        => "The issuing CA must be authorised by the domain's CAA records",
        "dane_valid"           => "TLSA records must match the presented certificate if DANE is configured",
        "consistency"          => "All resolved IPs must present the same certificate and TLS configuration",
        "alpn_consistency"     => "ALPN protocol negotiation must be consistent across all IPs",
        "ech_advertised"       => "Encrypted Client Hello hides the hostname from passive observers",
        "hsts"                 => "HTTP Strict Transport Security forces browsers to use HTTPS",
        "https_redirect"       => "HTTP must redirect to HTTPS",
        "security_headers"     => "CSP, X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy",
        "cors"                 => "CORS policy should not allow arbitrary origins in production APIs",
        "cookie_secure"        => "Session cookies must have the Secure and HttpOnly attributes",
        "hygiene"              => "Deprecated headers, information leakage, caching directives",
        "email_authentication" => "SPF, DKIM, and DMARC — authentication for every domain",
        "email_infrastructure" => "MX records, forward-confirmed reverse DNS, and DNSBL",
        "email_transport"      => "MTA-STS, TLS-RPT, and DANE — enforces encrypted inbound delivery",
        "email_brand_policy"   => "BIMI and DMARC enforcement — brand display and spoofing protection",
        "reputation"           => "IP reputation: VPNs warn, Tor exit nodes and known C2 hosts fail",
        _                      => "",
    }
}

fn guide_url_for(name: &str) -> Option<&'static str> {
    match name {
        "dnssec" | "dnskey_algorithm" | "dnssec_rollover" => Some("https://netray.info/guide/dnssec"),
        "cname_apex" | "https_svcb" | "ns" | "ttl" => Some("https://netray.info/guide/record-types"),
        "caa" => Some("https://netray.info/guide/caa-records"),
        "ns_lame" | "ns_delegation" => Some("https://netray.info/guide/lame-delegation"),
        "infrastructure" => Some("https://netray.info/guide/ip-enrichment"),
        "chain_trusted" | "chain_complete" | "strong_signature" | "key_strength"
        | "not_expired" | "hostname_match" | "ocsp_stapled" => Some("https://netray.info/guide/certificate-chain"),
        "expiry_window" | "cert_lifetime" | "san_quality" | "aia_reachability" => {
            Some("https://netray.info/guide/certificate-management")
        }
        "tls_version" | "forward_secrecy" | "aead_cipher" => Some("https://netray.info/guide/tls-protocol"),
        "consistency" | "alpn_consistency" => Some("https://netray.info/guide/multi-ip-tls"),
        "ech_advertised" => Some("https://netray.info/guide/encrypted-client-hello"),
        "ct_logged" => Some("https://netray.info/guide/certificate-transparency"),
        "hsts" | "https_redirect" => Some("https://netray.info/guide/hsts"),
        "security_headers" | "cors" | "cookie_secure" | "hygiene" => {
            Some("https://netray.info/guide/http-security")
        }
        "dane_valid" | "caa_compliant" => Some("https://netray.info/guide/dane-tlsa"),
        "email_authentication" | "email_infrastructure" | "email_transport" | "email_brand_policy" => {
            Some("https://netray.info/guide/email-auth")
        }
        "reputation" => Some("https://netray.info/guide/ip-enrichment"),
        _ => None,
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
