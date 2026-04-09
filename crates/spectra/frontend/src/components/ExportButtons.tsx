import { createSignal } from 'solid-js';
import type { InspectResponse } from '../lib/types';
import { downloadFile, copyToClipboard } from '@netray-info/common-frontend/utils';

interface Props {
  result: InspectResponse;
}

export default function ExportButtons(props: Props) {
  const [copyStatus, setCopyStatus] = createSignal<'idle' | 'success' | 'error'>('idle');

  const downloadJson = () => {
    const r = props.result;
    const host = (() => { try { return new URL(r.final_url).hostname; } catch { return r.url; } })();
    downloadFile(JSON.stringify(r, null, 2), `spectra-${host}.json`, 'application/json');
  };

  const copyMarkdown = async () => {
    const r = props.result;
    const lines: string[] = [
      `# HTTP Inspection: ${r.url}`,
      '',
      `**Verdict**: ${r.quality.verdict}  `,
      `**Status**: ${r.status} ${r.http_version}  `,
      `**Duration**: ${r.duration_ms}ms  `,
      `**IP**: ${r.enrichment.ip}`,
    ];

    if (r.enrichment.org) lines.push(`**Org**: ${r.enrichment.org}  `);
    if (r.enrichment.ip_type) {
      const cat = r.enrichment.ip_type.charAt(0).toUpperCase() + r.enrichment.ip_type.slice(1);
      lines.push(`**Category**: ${cat}  `);
    }
    if (r.enrichment.threat) lines.push(`**Threat**: ${r.enrichment.threat}  `);

    lines.push('', '## Quality Checks', '');
    for (const c of r.quality.checks) {
      const msg = c.message ? ` — ${c.message}` : '';
      lines.push(`- **${c.status.toUpperCase()}** ${c.label}${msg}`);
    }

    lines.push('', '## Security Headers', '');
    const sec = r.security;
    const hstsVal = sec.hsts.max_age != null
      ? `max-age=${sec.hsts.max_age}${sec.hsts.include_sub_domains ? '; includeSubDomains' : ''}${sec.hsts.preload ? '; preload' : ''}`
      : 'Not set';
    lines.push(`- **${sec.hsts.status.toUpperCase()}** HSTS: ${hstsVal}`);
    for (const [label, check] of [
      ['X-Frame-Options', sec.x_frame_options],
      ['X-Content-Type-Options', sec.x_content_type_options],
      ['Referrer-Policy', sec.referrer_policy],
      ['Permissions-Policy', sec.permissions_policy],
      ['COOP', sec.coop],
      ['COEP', sec.coep],
      ['CORP', sec.corp],
    ] as const) {
      const val = check.value ?? check.message ?? '';
      lines.push(`- **${check.status.toUpperCase()}** ${label}${val ? `: ${val}` : ''}`);
    }

    if (r.security.csp.issues.length > 0) {
      lines.push('', '## CSP Issues', '');
      for (const issue of r.security.csp.issues) lines.push(`- ${issue}`);
    }

    if (r.cookies.length > 0) {
      lines.push('', '## Cookies', '');
      for (const c of r.cookies) {
        const flags = [
          c.secure ? 'Secure' : '**NO Secure**',
          c.httponly ? 'HttpOnly' : 'no HttpOnly',
          c.samesite ? `SameSite=${c.samesite}` : 'no SameSite',
        ].join(', ');
        lines.push(`- \`${c.name}\`: ${flags}`);
      }
    }

    lines.push('', '## CORS', '');
    lines.push(`- Allows any origin: ${r.cors.allows_any_origin ? 'yes' : 'no'}`);
    lines.push(`- Reflects origin: ${r.cors.reflects_origin ? 'yes' : 'no'}`);
    lines.push(`- Allows credentials: ${r.cors.allows_credentials ? 'yes' : 'no'}`);
    lines.push(`- Status: ${r.cors.status}`);
    lines.push(`- ${r.cors.message}`);

    if (r.redirects.length > 0) {
      lines.push('', '## Redirects', '');
      for (const hop of r.redirects) {
        const loc = hop.location ? ` → ${hop.location}` : '';
        lines.push(`- [${hop.status}] ${hop.url}${loc}`);
      }
    }

    lines.push('', `_Inspected in ${r.duration_ms}ms via [spectra](https://http.netray.info)_`);

    const ok = await copyToClipboard(lines.join('\n'));
    setCopyStatus(ok ? 'success' : 'error');
    setTimeout(() => setCopyStatus('idle'), 2000);
  };

  return (
    <div class="export-buttons">
      <button
        class="export-buttons__btn"
        onClick={copyMarkdown}
        aria-label="Copy as Markdown"
      >
        {copyStatus() === 'success' ? 'copied!' : copyStatus() === 'error' ? 'failed' : 'copy MD'}
      </button>
      <button class="export-buttons__btn" onClick={downloadJson} aria-label="Download as JSON">
        JSON
      </button>
    </div>
  );
}
