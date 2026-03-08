import type { BatchEvent, Lookup, LookupResult, DoneStats } from '../components/ResultsTable';
import { formatRecordData, formatServer } from '../components/ResultsTable';

// ---------------------------------------------------------------------------
// Row extraction — flatten batches into tabular rows
// ---------------------------------------------------------------------------

interface ExportRow {
  name: string;
  ttl: string;
  type: string;
  value: string;
  server: string;
  time: string;
}

function responseTimeMs(rt: { secs: number; nanos: number }): number {
  return rt.secs * 1000 + rt.nanos / 1_000_000;
}

function formatTime(rt: { secs: number; nanos: number }): string {
  const ms = responseTimeMs(rt);
  if (ms < 1) return '<1ms';
  return `${Math.round(ms)}ms`;
}

function isResponse(result: LookupResult): boolean {
  return 'Response' in result;
}

function isNxDomain(result: LookupResult): boolean {
  return 'NxDomain' in result;
}

function formatLookupError(result: LookupResult): string {
  const keys = Object.keys(result);
  if (keys.length === 0) return 'Unknown error';
  const key = keys[0];
  const val = (result as Record<string, unknown>)[key];
  if (val === null || val === undefined) return key;
  if (typeof val === 'string') return `${key}: ${val}`;
  if (typeof val === 'object' && val !== null && 'reason' in val) {
    return `${key}: ${(val as { reason: string }).reason}`;
  }
  return key;
}

function extractRows(batches: BatchEvent[]): ExportRow[] {
  const rows: ExportRow[] = [];
  for (const batch of batches) {
    for (const lookup of batch.lookups) {
      const server = formatServer(lookup.name_server);
      if (isResponse(lookup.result)) {
        const resp = (lookup.result as { Response: { records: { name: string; ttl: number; type: string; data: Record<string, unknown> }[]; response_time: { secs: number; nanos: number } } }).Response;
        const time = formatTime(resp.response_time);
        for (const record of resp.records) {
          rows.push({
            name: record.name,
            ttl: `${record.ttl}s`,
            type: record.type ?? batch.record_type,
            value: formatRecordData(record.data),
            server,
            time,
          });
        }
      } else if (isNxDomain(lookup.result)) {
        const nx = (lookup.result as { NxDomain: { response_time: { secs: number; nanos: number } } }).NxDomain;
        rows.push({
          name: lookup.query.name,
          ttl: '-',
          type: batch.record_type,
          value: 'NXDOMAIN',
          server,
          time: formatTime(nx.response_time),
        });
      } else {
        rows.push({
          name: lookup.query.name,
          ttl: '-',
          type: batch.record_type,
          value: formatLookupError(lookup.result),
          server,
          time: '-',
        });
      }
    }
  }
  return rows;
}

// ---------------------------------------------------------------------------
// Markdown
// ---------------------------------------------------------------------------

export function toMarkdown(batches: BatchEvent[]): string {
  const rows = extractRows(batches);
  if (rows.length === 0) return '';

  const header = '| Name | TTL | Type | Value | Server | Time |';
  const separator = '|------|-----|------|-------|--------|------|';
  const lines = rows.map(
    (r) => `| ${r.name} | ${r.ttl} | ${r.type} | ${r.value} | ${r.server} | ${r.time} |`,
  );
  return [header, separator, ...lines].join('\n');
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------

function csvEscape(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export function toCsv(batches: BatchEvent[]): string {
  const rows = extractRows(batches);
  const header = 'Name,TTL,Type,Value,Server,Time';
  const lines = rows.map(
    (r) =>
      [r.name, r.ttl, r.type, r.value, r.server, r.time].map(csvEscape).join(','),
  );
  return [header, ...lines].join('\n');
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

export function toJson(batches: BatchEvent[], stats: DoneStats | null): string {
  return JSON.stringify({ batches, stats }, null, 2);
}

// ---------------------------------------------------------------------------
// Download / clipboard helpers
// ---------------------------------------------------------------------------

export function downloadFile(content: string, filename: string, mime: string): void {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
