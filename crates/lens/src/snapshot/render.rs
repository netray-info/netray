use crate::badge::palette::color_for_grade;
use crate::config::SiteConfig;
use crate::snapshot::types::Snapshot;

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

    // OG description: "DNS: A | TLS: B | ..."
    let og_description = snap
        .sections
        .iter()
        .map(|s| format!("{}: {}", html_escape(&s.name), html_escape(&s.grade)))
        .collect::<Vec<_>>()
        .join(" | ");

    let og_title = html_escape(&format!("{} — {} | {brand_name}", snap.domain, snap.grade));
    let og_image = html_escape(&format!("/og/{}.png", snap.domain));
    let og_url = html_escape(&format!("/r/{}", snap.shortid));

    // Build sections HTML
    let sections_html: String = snap.sections.iter().map(|section| {
        let sec_color = color_for_grade(&section.grade);
        let sec_name = html_escape(&section.name);
        let sec_grade = html_escape(&section.grade);
        let findings_html: String = section.findings.iter().map(|f| {
            let verdict_class = match f.verdict.as_str() {
                "pass" => "verdict-pass",
                "warn" => "verdict-warn",
                "fail" => "verdict-fail",
                "skip" => "verdict-skip",
                _ => "verdict-skip",
            };
            let verdict_label = match f.verdict.as_str() {
                "pass" => "PASS",
                "warn" => "WARN",
                "fail" => "FAIL",
                "skip" => "SKIP",
                _ => "—",
            };
            let check_name = html_escape(&f.check_name);
            let message = html_escape(&f.message);
            format!(
                r#"<tr><td class="check-name">{check_name}</td><td><span class="verdict {verdict_class}">{verdict_label}</span></td><td class="check-message">{message}</td></tr>"#
            )
        }).collect();

        format!(
            r#"<div class="section-card">
  <div class="section-header">
    <span class="section-name">{sec_name}</span>
    <span class="grade-pill" style="background:{sec_color}">{sec_grade}</span>
    <span class="section-counts">
      <span class="count-pass">{} pass</span>
      <span class="count-warn">{} warn</span>
      <span class="count-fail">{} fail</span>
    </span>
  </div>
  <table class="findings-table">
    <tbody>
      {findings_html}
    </tbody>
  </table>
</div>"#,
            section.passes, section.warns, section.fails
        )
    }).collect();

    // Build footer links HTML
    let footer_links_html: String = footer_links
        .iter()
        .map(|link| {
            let label = html_escape(&link.label);
            let href = html_escape(&link.href);
            if link.external {
                format!(
                    r#"<a href="{href}" target="_blank" rel="noopener noreferrer">{label}</a>"#
                )
            } else {
                format!(r#"<a href="{href}">{label}</a>"#)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");

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
      background: #0d1220;
      color: #dce6f5;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
      min-height: 100vh;
      padding: 1.5rem 1rem 3rem;
    }}
    a {{ color: #60a5fa; text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .page {{ max-width: 800px; margin: 0 auto; }}
    .site-header {{
      display: flex;
      align-items: baseline;
      gap: 0.75rem;
      margin-bottom: 2rem;
    }}
    .brand-name {{
      font-size: 1.25rem;
      font-weight: 700;
      color: #dce6f5;
    }}
    .brand-tagline {{
      font-size: 0.85rem;
      color: #7b90b5;
    }}
    .result-header {{
      display: flex;
      align-items: center;
      gap: 1rem;
      margin-bottom: 1.5rem;
      flex-wrap: wrap;
    }}
    .domain {{
      font-size: 1.5rem;
      font-weight: 700;
      word-break: break-all;
    }}
    .grade-pill {{
      display: inline-block;
      padding: 0.25rem 0.75rem;
      border-radius: 9999px;
      font-size: 1.1rem;
      font-weight: 700;
      color: #fff;
    }}
    .section-card {{
      background: #1a2540;
      border: 1px solid #2a3a5c;
      border-radius: 0.5rem;
      margin-bottom: 1rem;
      overflow: hidden;
    }}
    .section-header {{
      display: flex;
      align-items: center;
      gap: 0.75rem;
      padding: 0.75rem 1rem;
      background: #1e2d4e;
      flex-wrap: wrap;
    }}
    .section-name {{
      font-weight: 600;
      font-size: 1rem;
    }}
    .section-counts {{
      margin-left: auto;
      display: flex;
      gap: 0.5rem;
      font-size: 0.8rem;
    }}
    .count-pass {{ color: #22c55e; }}
    .count-warn {{ color: #f59e0b; }}
    .count-fail {{ color: #ef4444; }}
    .findings-table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 0.875rem;
    }}
    .findings-table tr + tr {{ border-top: 1px solid #2a3a5c; }}
    .findings-table td {{
      padding: 0.5rem 1rem;
      vertical-align: top;
    }}
    .check-name {{
      font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace;
      color: #94afd4;
      white-space: nowrap;
      width: 1%;
    }}
    .check-message {{ color: #a8bcd8; }}
    .verdict {{
      display: inline-block;
      padding: 0.1rem 0.4rem;
      border-radius: 0.25rem;
      font-size: 0.7rem;
      font-weight: 700;
      white-space: nowrap;
    }}
    .verdict-pass {{ background: #14532d; color: #22c55e; }}
    .verdict-warn {{ background: #451a03; color: #f59e0b; }}
    .verdict-fail {{ background: #450a0a; color: #ef4444; }}
    .verdict-skip {{ background: #1e293b; color: #64748b; }}
    .page-footer {{
      margin-top: 2rem;
      padding-top: 1rem;
      border-top: 1px solid #2a3a5c;
      font-size: 0.8rem;
      color: #7b90b5;
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem 1.5rem;
    }}
    .footer-about {{ flex-basis: 100%; }}
    .footer-links {{ display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; }}
    .rerun-link {{ margin-left: auto; }}
  </style>
</head>
<body>
<div class="page">
  <header class="site-header">
    <span class="brand-name">{brand_name}</span>
    {brand_tagline_html}
  </header>
  <div class="result-header">
    <span class="domain">{domain}</span>
    <span class="grade-pill" style="background:{grade_color}">{grade}</span>
  </div>
  {sections_html}
  <footer class="page-footer">
    {footer_about_html}
    <div class="footer-links">{footer_links_html}</div>
    <span>Checked {checked_at}</span>
    <span>Checked by {brand_name} {lens_version}</span>
    <span class="rerun-link"><a href="/?d={domain}">Re-run this check</a></span>
  </footer>
</div>
</body>
</html>"#,
        og_title = og_title,
        og_description = og_description,
        og_image = og_image,
        og_url = og_url,
        brand_name = brand_name,
        brand_tagline_html = if brand_tagline.is_empty() {
            String::new()
        } else {
            format!(r#"<span class="brand-tagline">{brand_tagline}</span>"#)
        },
        domain = domain,
        grade_color = grade_color,
        grade = html_escape(&snap.grade),
        sections_html = sections_html,
        footer_about_html = if footer_about.is_empty() {
            String::new()
        } else {
            format!(r#"<p class="footer-about">{}</p>"#, html_escape(footer_about))
        },
        footer_links_html = footer_links_html,
        checked_at = checked_at,
        lens_version = lens_version,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
