import type { DnsEvent, TlsEvent, IpEvent, SummaryEvent, DoneEvent } from './types';
import { CHECK_LABELS } from './checkMeta';

export function toJson(
  domain: string,
  dns: DnsEvent | null,
  tls: TlsEvent | null,
  ip: IpEvent | null,
  summary: SummaryEvent | null,
  done: DoneEvent | null,
): string {
  return JSON.stringify({ domain, dns, tls, ip, summary, done }, null, 2);
}

function verdictSymbol(verdict: string): string {
  switch (verdict) {
    case 'pass':  return '✓';
    case 'warn':  return '!';
    case 'fail':
    case 'error': return '✗';
    default:      return '–';
  }
}

export function toMarkdown(
  domain: string,
  dns: DnsEvent | null,
  tls: TlsEvent | null,
  ip: IpEvent | null,
  summary: SummaryEvent | null,
  done: DoneEvent | null,
): string {
  const lines: string[] = [];

  lines.push(`# lens report: ${domain}`);
  lines.push('');

  if (summary) {
    const durationPart = done ? `  **Duration:** ${done.duration_ms}ms` : '';
    lines.push(`**Grade:** ${summary.grade} (${summary.score}%)${durationPart}`);
    lines.push('');
  }

  function renderSection(
    label: string,
    status: string,
    event: { checks: { name: string; verdict: string; messages?: string[] }[] } | null,
  ) {
    if (!event) return;
    lines.push(`## ${label} — ${status}`);
    for (const check of event.checks) {
      const sym = verdictSymbol(check.verdict);
      const name = CHECK_LABELS[check.name] ?? check.name;
      const msg = check.messages?.length ? ` · ${check.messages.join('; ')}` : '';
      lines.push(`- ${sym} ${name}${msg}`);
    }
    lines.push('');
  }

  if (summary) {
    renderSection('TLS', summary.sections['tls'] ?? '', tls);
    renderSection('DNS', summary.sections['dns'] ?? '', dns);
  } else {
    renderSection('TLS', tls?.status ?? '', tls);
    renderSection('DNS', dns?.status ?? '', dns);
  }

  if (ip) {
    const ipStatus = summary ? (summary.sections['ip'] ?? ip.status) : ip.status;
    lines.push(`## IP — ${ipStatus}`);
    for (const check of ip.checks) {
      const sym = verdictSymbol(check.verdict);
      const name = CHECK_LABELS[check.name] ?? check.name;
      const msg = check.messages?.length ? ` · ${check.messages.join('; ')}` : '';
      lines.push(`- ${sym} ${name}${msg}`);
    }
    lines.push('');
    if (ip.addresses.length > 0) {
      for (const addr of ip.addresses) {
        const parts = [addr.ip];
        if (addr.org) parts.push(addr.org);
        if (addr.geo) parts.push(addr.geo);
        parts.push(addr.network_type);
        lines.push(parts.join(' · '));
      }
      lines.push('');
    }
  }

  return lines.join('\n');
}
