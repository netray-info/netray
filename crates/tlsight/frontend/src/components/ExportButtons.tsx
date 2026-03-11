import { createSignal } from 'solid-js';
import type { InspectResponse } from '../lib/types';

interface Props {
  result: InspectResponse;
}

export default function ExportButtons(props: Props) {
  const [copyStatus, setCopyStatus] = createSignal<'idle' | 'success' | 'error'>('idle');

  const downloadJson = () => {
    const blob = new Blob([JSON.stringify(props.result, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `tlsight-${props.result.hostname}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const copyMarkdown = async () => {
    const r = props.result;
    const lines: string[] = [
      `# TLS Inspection: ${r.hostname}`,
      '',
      `**Verdict**: ${r.summary.verdict}`,
      '',
      '## Checks',
      ...Object.entries(r.summary.checks).map(([k, v]) => `- ${k}: ${v}`),
      '',
    ];

    for (const port of r.ports) {
      lines.push(`## Port ${port.port}`);
      for (const ip of port.ips) {
        lines.push(`### ${ip.ip} (${ip.ip_version})`);
        if (ip.tls) {
          lines.push(`- TLS: ${ip.tls.version}, ${ip.tls.cipher_suite}`);
        }
        if (ip.chain) {
          lines.push('- Chain: ' + ip.chain.map(c => c.subject).join(' \u2192 '));
        }
        if (ip.error) {
          lines.push(`- Error: ${ip.error.message}`);
        }
      }
      lines.push('');
    }

    if (r.quality || r.ports.some(p => p.quality)) {
      lines.push('## Detailed Checks');
      if (r.quality) {
        for (const c of r.quality.checks) {
          lines.push(`- ${c.status}: ${c.label} — ${c.detail}`);
        }
      }
      for (const port of r.ports) {
        if (port.quality) {
          if (r.ports.length > 1) lines.push(`### Port ${port.port}`);
          for (const c of port.quality.checks) {
            lines.push(`- ${c.status}: ${c.label} — ${c.detail}`);
          }
        }
      }
      lines.push('');
    }

    lines.push(`_Inspected in ${r.duration_ms}ms_`);

    try {
      await navigator.clipboard.writeText(lines.join('\n'));
      setCopyStatus('success');
      setTimeout(() => setCopyStatus('idle'), 2000);
    } catch {
      setCopyStatus('error');
      setTimeout(() => setCopyStatus('idle'), 2000);
    }
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
      <button class="export-buttons__btn" onClick={downloadJson} aria-label="Download as JSON">JSON</button>
    </div>
  );
}
