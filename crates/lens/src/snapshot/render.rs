use crate::config::SiteConfig;
use crate::snapshot::types::Snapshot;

/// CSS-variable expression for the colour associated with a grade.
/// Returns a `var(--grade-X)` string so the value adapts to the active theme.
fn grade_color_var(grade: &str) -> &'static str {
    match grade {
        "A+" => "var(--grade-a-plus)",
        "A" => "var(--grade-a)",
        "B" => "var(--grade-b)",
        "C" => "var(--grade-c)",
        "D" => "var(--grade-d)",
        "F" => "var(--grade-f)",
        _ => "var(--text-muted)",
    }
}

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

    let grade_color = grade_color_var(&snap.grade);
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
            let color = grade_color_var(&s.grade);
            let name = html_escape(&s.name);
            let grade = html_escape(&s.grade);
            format!(
                r#"<span class="sdot"><span class="sdot__dot" style="background:{color}"></span><span class="sdot__name">{name}</span><span class="sdot__grade" style="color:{color}">{grade}</span></span>"#
            )
        })
        .collect();

    // Build sections HTML
    let sections_html: String = sections.iter().map(|section| {
        let sec_color = grade_color_var(&section.grade);
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
        format!(
            r#"<span class="brand-tagline">{}</span>"#,
            html_escape(brand_tagline)
        )
    };

    let footer_about_html = if footer_about.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="footer-about">{}</p>"#,
            html_escape(footer_about)
        )
    };

    let grade = html_escape(&snap.grade);

    // Chips
    let mut chips = String::new();
    if total_fail > 0 {
        chips.push_str(&format!(
            r#"<span class="chip chip--fail">{total_fail} FAILED</span>"#
        ));
    }
    if total_warn > 0 {
        chips.push_str(&format!(
            r#"<span class="chip chip--warn">{total_warn} WARNINGS</span>"#
        ));
    }
    if total_pass > 0 {
        chips.push_str(&format!(
            r#"<span class="chip chip--pass">{total_pass} PASSED</span>"#
        ));
    }
    if total_skip > 0 {
        chips.push_str(&format!(
            r#"<span class="chip chip--skip">{total_skip} SKIPPED</span>"#
        ));
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
    /* ── Theme tokens (mirrors SPA common-frontend theme.css) ── */
    :root {{
      --bg-primary: #1a1a2e;
      --bg-secondary: #16213e;
      --bg-tertiary: #0f3460;
      --text-primary: #e0e0e0;
      --text-secondary: #a0a0a0;
      --text-muted: #888888;
      --accent: #00d4ff;
      --accent-secondary: #7b68ee;
      --bg-card-hover: #0f3460;
      --border: #2a2a4a;
      --border-subtle: #222240;
      --pass: #22c55e;
      --warn: #f59e0b;
      --fail: #ef4444;
      --skip: #94a3b8;
      --grade-a-plus: #22c55e;
      --grade-a: #22c55e;
      --grade-b: #84cc16;
      --grade-c: #f59e0b;
      --grade-d: #f97316;
      --grade-f: #ef4444;
      --font-mono: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Consolas', monospace;
      --font-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
      --radius: 8px;
      --radius-lg: 10px;
      --transition: 150ms ease;
    }}
    html:has(#theme-toggle:checked) {{
      --bg-primary: #f5f7fa;
      --bg-secondary: #ffffff;
      --bg-tertiary: #edf0f5;
      --text-primary: #1a1a2e;
      --text-secondary: #4a4a6a;
      --text-muted: #8888a0;
      --accent: #0077cc;
      --bg-card-hover: #edf0f5;
      --border: #d0d4dc;
      --border-subtle: #e4e8ee;
      --pass: #008800;
      --warn: #b86e00;
      --fail: #cc0000;
      --skip: #4a5568;
      --grade-a-plus: #008800;
      --grade-a: #008800;
      --grade-b: #4a7c00;
      --grade-c: #b86e00;
      --grade-d: #c05000;
      --grade-f: #cc0000;
    }}

    *, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      background: var(--bg-primary);
      color: var(--text-primary);
      font-family: var(--font-sans);
      font-size: 16px;
      min-height: 100vh;
      padding: 1.5rem 1rem 3rem;
    }}
    a {{ color: var(--accent); text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .page {{ max-width: 960px; margin: 0 auto; }}

    /* ── Nav (mirrors SPA .header / .logo / .tagline) ── */
    .site-nav {{
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 2rem;
      padding-bottom: 1rem;
      border-bottom: 1px solid var(--border-subtle);
    }}
    .brand-name {{
      font-family: var(--font-mono);
      font-size: 1.75rem;
      font-weight: 700;
      letter-spacing: -0.5px;
      background: linear-gradient(135deg, var(--accent), var(--accent-secondary));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
      text-decoration: none;
    }}
    .brand-name:hover {{ text-decoration: none; }}
    .brand-tagline {{ font-size: 0.875rem; color: var(--text-muted); }}
    .nav-actions {{ margin-left: auto; display: flex; align-items: center; gap: 6px; }}

    /* ── Theme toggle (CSS-only via :has() + checkbox) ── */
    .theme-toggle__input {{
      position: absolute;
      width: 1px; height: 1px;
      padding: 0; margin: -1px;
      overflow: hidden;
      clip: rect(0,0,0,0);
      white-space: nowrap;
      border: 0;
    }}
    .theme-toggle {{
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 32px; height: 32px;
      border-radius: 6px;
      cursor: pointer;
      background: transparent;
      border: 1px solid var(--border);
      color: var(--text-muted);
      transition: background var(--transition), color var(--transition), border-color var(--transition);
      user-select: none;
    }}
    .theme-toggle:hover {{ background: var(--bg-card-hover); color: var(--text-primary); }}
    .theme-toggle__input:focus-visible + .theme-toggle {{ outline: 2px solid var(--accent); outline-offset: 2px; }}
    .theme-toggle__icon--sun {{ display: none; }}
    html:has(#theme-toggle:checked) .theme-toggle__icon--moon {{ display: none; }}
    html:has(#theme-toggle:checked) .theme-toggle__icon--sun {{ display: inline; }}

    /* ── Summary card ── */
    .summary-card {{
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      padding: 1.25rem 1.5rem;
      margin-bottom: 1.5rem;
    }}
    .summary-top {{
      display: flex;
      align-items: stretch;
      gap: 1rem;
    }}
    .grade-letter {{
      font-family: var(--font-mono);
      font-size: 5.5rem;
      font-weight: 700;
      line-height: 1;
      flex-shrink: 0;
      display: flex;
      align-items: center;
      min-width: 4rem;
    }}
    .summary-meta {{
      flex: 1;
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: 4px;
    }}
    .summary-domain {{
      font-size: 1.25rem;
      font-weight: 700;
      color: var(--text-primary);
      word-break: break-all;
      line-height: 1.2;
    }}
    .summary-row {{
      display: flex;
      align-items: center;
      gap: 0.875rem;
      flex-wrap: wrap;
    }}
    .summary-score {{
      font-size: 1.375rem;
      font-weight: 700;
      color: var(--text-primary);
      flex-shrink: 0;
    }}
    .sdot {{
      display: inline-flex;
      align-items: center;
      gap: 5px;
      font-size: 0.8125rem;
      color: var(--text-secondary);
    }}
    .sdot__dot {{
      width: 10px;
      height: 10px;
      border-radius: 50%;
      flex-shrink: 0;
    }}
    .sdot__name {{ color: var(--text-secondary); }}
    .sdot__grade {{
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 0.8125rem;
      letter-spacing: 0.02em;
    }}
    .summary-descriptor {{ font-size: 0.8125rem; color: var(--text-secondary); }}
    .summary-descriptor strong {{ color: var(--text-primary); font-weight: 600; }}
    .summary-headline {{ font-size: 0.875rem; color: var(--text-primary); font-weight: 500; }}
    .summary-chips {{
      margin-top: 0.875rem;
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
    .chip--warn {{ background: color-mix(in srgb, var(--warn) 15%, transparent); color: var(--warn); border: 1px solid color-mix(in srgb, var(--warn) 30%, transparent); }}
    .chip--pass {{ background: color-mix(in srgb, var(--pass) 12%, transparent); color: var(--pass); border: 1px solid color-mix(in srgb, var(--pass) 25%, transparent); }}
    .chip--skip {{ background: color-mix(in srgb, var(--skip) 12%, transparent); color: var(--skip); border: 1px solid color-mix(in srgb, var(--skip) 25%, transparent); }}
    .chip--fail {{ background: color-mix(in srgb, var(--fail) 12%, transparent); color: var(--fail); border: 1px solid color-mix(in srgb, var(--fail) 25%, transparent); }}
    .summary-checked {{ margin-top: 0.625rem; font-size: 0.75rem; color: var(--text-muted); }}

    /* ── Server addresses ── */
    .server-addrs {{
      margin-top: 0.875rem;
      padding-top: 0.75rem;
      border-top: 1px solid var(--border-subtle);
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
    .addr-role {{ color: var(--text-muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.04em; min-width: 2.5rem; }}
    .addr-ip {{ font-family: var(--font-mono); color: var(--text-secondary); font-weight: 600; }}
    .addr-org {{ color: var(--text-muted); font-size: 0.75rem; }}

    /* ── Section cards ── */
    .section-card {{
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius-lg);
      margin-bottom: 0.75rem;
      overflow: hidden;
    }}
    .section-header {{
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0.875rem 1rem;
      border-bottom: 1px solid var(--border-subtle);
      gap: 0.75rem;
      flex-wrap: wrap;
    }}
    .section-header__left {{
      display: flex;
      align-items: center;
      gap: 0.625rem;
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
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 0.9375rem;
      letter-spacing: 0.02em;
      color: var(--text-primary);
    }}
    .issue-tag {{
      font-size: 0.75rem;
      font-weight: 600;
      font-family: var(--font-mono);
      padding: 0.1rem 0.35rem;
      border-radius: 0.2rem;
    }}
    .issue-tag--warn {{ color: var(--warn); }}
    .issue-tag--fail {{ color: var(--fail); }}
    .section-counts {{ font-size: 0.8125rem; white-space: nowrap; color: var(--text-secondary); }}
    .count-pass {{ color: var(--pass); }}
    .count-warn {{ color: var(--warn); }}
    .count-fail {{ color: var(--fail); }}
    .count-sep {{ color: var(--border-subtle); }}

    /* ── Findings ── */
    .findings {{ font-size: 0.875rem; padding: 0.5rem 0; }}
    .finding {{ padding: 0.45rem 1rem; }}
    .finding + .finding {{ border-top: 1px solid var(--border-subtle); }}
    .finding-row {{
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }}
    .verdict-sym {{
      font-size: 0.9rem;
      font-weight: 700;
      flex-shrink: 0;
      width: 1rem;
      text-align: center;
    }}
    .verdict-sym--pass {{ color: var(--pass); }}
    .verdict-sym--warn {{ color: var(--warn); }}
    .verdict-sym--fail {{ color: var(--fail); }}
    .verdict-sym--skip {{ color: var(--skip); }}
    .check-name {{ font-weight: 500; cursor: default; font-size: 0.875rem; }}
    .check-name--pass {{ color: var(--text-secondary); }}
    .check-name--warn {{ color: var(--warn); }}
    .check-name--fail {{ color: var(--fail); }}
    .check-name--skip {{ color: var(--skip); }}
    .learn-more {{ color: var(--text-muted); font-size: 0.75rem; flex-shrink: 0; text-decoration: none; }}
    .learn-more:hover {{ color: var(--accent); text-decoration: none; }}
    .finding-spacer {{ flex: 1; }}
    .check-score {{
      font-family: var(--font-mono);
      font-size: 0.75rem;
      color: var(--text-muted);
      background: var(--bg-tertiary);
      border-radius: var(--radius);
      padding: 0.1rem 0.5rem;
      white-space: nowrap;
      flex-shrink: 0;
    }}
    .finding-msg {{
      color: var(--text-secondary);
      font-size: 0.8125rem;
      margin-left: 1.5rem;
      margin-top: 0.15rem;
    }}

    /* ── Footer ── */
    .page-footer {{
      margin-top: 2rem;
      padding-top: 1rem;
      border-top: 1px solid var(--border-subtle);
      font-size: 0.8125rem;
      color: var(--text-muted);
      display: flex;
      flex-wrap: wrap;
      align-items: baseline;
      gap: 0.5rem 1.5rem;
    }}
    .footer-about {{ flex-basis: 100%; color: var(--text-secondary); }}
    .footer-links {{ display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; }}
    .rerun-link {{ margin-left: auto; }}
  </style>
</head>
<body>
<div class="page">
  <input type="checkbox" id="theme-toggle" class="theme-toggle__input" aria-label="Toggle light theme">
  <nav class="site-nav">
    <a class="brand-name" href="/">{brand_name}</a>
    {brand_tagline_html}
    <span class="nav-actions">
      <label for="theme-toggle" class="theme-toggle" title="Toggle theme">
        <svg class="theme-toggle__icon theme-toggle__icon--moon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
        </svg>
        <svg class="theme-toggle__icon theme-toggle__icon--sun" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="4"/>
          <line x1="12" y1="2" x2="12" y2="5"/>
          <line x1="12" y1="19" x2="12" y2="22"/>
          <line x1="4.22" y1="4.22" x2="6.34" y2="6.34"/>
          <line x1="17.66" y1="17.66" x2="19.78" y2="19.78"/>
          <line x1="2" y1="12" x2="5" y2="12"/>
          <line x1="19" y1="12" x2="22" y2="12"/>
          <line x1="4.22" y1="19.78" x2="6.34" y2="17.66"/>
          <line x1="17.66" y1="6.34" x2="19.78" y2="4.22"/>
        </svg>
      </label>
    </span>
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
        "A" => Some(("strong", "minor polish possible")),
        "B" => Some(("ok", "weaknesses worth fixing")),
        "C" => Some(("fair", "significant issues")),
        "D" => Some(("weak", "serious problems")),
        "F" => Some(("failing", "critical failures")),
        _ => None,
    }
}

fn count_headline(total_warn: u32, total_fail: u32) -> String {
    match (total_fail, total_warn) {
        (0, 0) => "Everything checks out".to_string(),
        (0, w) => format!(
            "{w} {} worth a look",
            if w == 1 { "warning" } else { "warnings" }
        ),
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
        "spf" => "SPF Record",
        "dmarc" => "DMARC Policy",
        "dnssec" => "DNSSEC",
        "caa" => "CAA Records",
        "mx" => "MX Records",
        "ns" => "Nameservers",
        "ns_lame" => "Lame Delegation",
        "ns_delegation" => "NS Delegation",
        "dkim" => "DKIM",
        "mta_sts" => "MTA-STS",
        "tlsrpt" => "SMTP TLS Reporting",
        "bimi" => "BIMI",
        "cname_apex" => "CNAME at Apex",
        "https_svcb" => "HTTPS Record",
        "ttl" => "TTL Consistency",
        "dnskey_algorithm" => "DNSKEY Algorithm",
        "dnssec_rollover" => "DNSSEC Key Rollover",
        "infrastructure" => "Infrastructure",
        "chain_trusted" => "Chain of Trust",
        "not_expired" => "Not Expired",
        "hostname_match" => "Hostname Match",
        "chain_complete" => "Chain Complete",
        "strong_signature" => "Signature Algorithm",
        "key_strength" => "Key Strength",
        "expiry_window" => "Expiry Window",
        "cert_lifetime" => "Certificate Lifetime",
        "san_quality" => "SAN Quality",
        "aia_reachability" => "AIA Reachability",
        "tls_version" => "TLS Version",
        "forward_secrecy" => "Forward Secrecy",
        "aead_cipher" => "AEAD Cipher",
        "ocsp_stapled" => "OCSP Stapling",
        "ct_logged" => "CT Logged",
        "caa_compliant" => "CAA Compliance",
        "dane_valid" => "DANE / TLSA",
        "consistency" => "Multi-IP Consistency",
        "alpn_consistency" => "ALPN Consistency",
        "ech_advertised" => "Encrypted Client Hello",
        "hsts" => "HSTS",
        "https_redirect" => "HTTPS Redirect",
        "security_headers" => "Security Headers",
        "cors" => "CORS Policy",
        "cookie_secure" => "Secure Cookies",
        "hygiene" => "Header Hygiene",
        "email_authentication" => "Authentication",
        "email_infrastructure" => "Infrastructure",
        "email_transport" => "Transport",
        "email_brand_policy" => "Brand Policy",
        "reputation" => "IP Reputation",
        other => other,
    }
}

fn guide_url_for(name: &str) -> Option<&'static str> {
    match name {
        "dnssec" | "dnskey_algorithm" | "dnssec_rollover" => {
            Some("https://netray.info/guide/dnssec")
        }
        "cname_apex" | "https_svcb" | "ns" | "ttl" => {
            Some("https://netray.info/guide/record-types")
        }
        "caa" => Some("https://netray.info/guide/caa-records"),
        "ns_lame" | "ns_delegation" => Some("https://netray.info/guide/lame-delegation"),
        "infrastructure" => Some("https://netray.info/guide/ip-enrichment"),
        "chain_trusted" | "chain_complete" | "strong_signature" | "key_strength"
        | "not_expired" | "hostname_match" | "ocsp_stapled" => {
            Some("https://netray.info/guide/certificate-chain")
        }
        "expiry_window" | "cert_lifetime" | "san_quality" | "aia_reachability" => {
            Some("https://netray.info/guide/certificate-management")
        }
        "tls_version" | "forward_secrecy" | "aead_cipher" => {
            Some("https://netray.info/guide/tls-protocol")
        }
        "consistency" | "alpn_consistency" => Some("https://netray.info/guide/multi-ip-tls"),
        "ech_advertised" => Some("https://netray.info/guide/encrypted-client-hello"),
        "ct_logged" => Some("https://netray.info/guide/certificate-transparency"),
        "hsts" | "https_redirect" => Some("https://netray.info/guide/hsts"),
        "security_headers" | "cors" | "cookie_secure" | "hygiene" => {
            Some("https://netray.info/guide/http-security")
        }
        "dane_valid" | "caa_compliant" => Some("https://netray.info/guide/dane-tlsa"),
        "email_authentication"
        | "email_infrastructure"
        | "email_transport"
        | "email_brand_policy" => Some("https://netray.info/guide/email-auth"),
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
