import type { InspectResponse } from '../lib/types';

interface Props {
  result: InspectResponse;
}

export default function ExportButtons(props: Props) {
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

    lines.push(`_Inspected in ${r.duration_ms}ms_`);

    await navigator.clipboard.writeText(lines.join('\n'));
  };

  return (
    <div class="export-buttons">
      <button class="export-buttons__btn" onClick={copyMarkdown}>Copy MD</button>
      <button class="export-buttons__btn" onClick={downloadJson}>JSON</button>
    </div>
  );
}
