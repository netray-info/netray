import { For, Show, createMemo } from 'solid-js';
import type { BatchEvent, Lookup, LookupResult } from './ResultsTable';
import { responseTimeMs, responseTimeColor } from './ResultsTable';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ServerComparisonProps {
  results: BatchEvent[];
}

interface ServerGroup {
  serverName: string;
  lookups: Lookup[];
}

interface RecordTypeComparison {
  recordType: string;
  servers: ServerGroup[];
  /** Whether all servers agree on the answer set. */
  allAgree: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatServer(ns: string): string {
  const nameMatch = ns.match(/name=(.+?)(?:,|$)/);
  if (nameMatch) return nameMatch[1];
  return ns;
}

function isResponse(result: LookupResult): boolean {
  return 'Response' in result;
}

function isNxDomain(result: LookupResult): boolean {
  return 'NxDomain' in result;
}

/** Extract a canonical set of answer values for comparison. */
function answerFingerprint(lookup: Lookup): string {
  const result = lookup.result;
  if ('Response' in result) {
    const resp = (result as { Response: { records: Array<{ data: Record<string, unknown> }> } }).Response;
    const values = resp.records
      .map((r) => JSON.stringify(r.data))
      .sort();
    return `response:${values.join('|')}`;
  }
  if ('NxDomain' in result) return 'nxdomain';
  // Error variant
  const keys = Object.keys(result);
  return `error:${keys[0] ?? 'unknown'}`;
}

function formatResponseTime(rt: { secs: number; nanos: number }): string {
  const ms = rt.secs * 1000 + rt.nanos / 1_000_000;
  if (ms < 1) return '<1ms';
  return `${Math.round(ms)}ms`;
}

function getResponseTime(lookup: Lookup): string {
  const result = lookup.result;
  if ('Response' in result) {
    return formatResponseTime((result as { Response: { response_time: { secs: number; nanos: number } } }).Response.response_time);
  }
  if ('NxDomain' in result) {
    return formatResponseTime((result as { NxDomain: { response_time: { secs: number; nanos: number } } }).NxDomain.response_time);
  }
  return '-';
}

/** Extract response time in ms from a lookup, or null for errors. */
function getResponseTimeMs(lookup: Lookup): number | null {
  const result = lookup.result;
  if ('Response' in result) {
    return responseTimeMs((result as { Response: { response_time: { secs: number; nanos: number } } }).Response.response_time);
  }
  if ('NxDomain' in result) {
    return responseTimeMs((result as { NxDomain: { response_time: { secs: number; nanos: number } } }).NxDomain.response_time);
  }
  return null;
}

function getRecordCount(lookup: Lookup): number {
  const result = lookup.result;
  if ('Response' in result) {
    return (result as { Response: { records: unknown[] } }).Response.records.length;
  }
  return 0;
}

function getStatusLabel(lookup: Lookup): { text: string; className: string } {
  const result = lookup.result;
  if ('Response' in result) {
    const count = getRecordCount(lookup);
    return { text: `${count} record${count !== 1 ? 's' : ''}`, className: 'status-ok' };
  }
  if ('NxDomain' in result) {
    return { text: 'NXDOMAIN', className: 'status-nxdomain' };
  }
  const keys = Object.keys(result);
  return { text: keys[0] ?? 'Error', className: 'status-error' };
}

function formatRecordData(data: Record<string, unknown>): string {
  const keys = Object.keys(data);
  if (keys.length === 0) return '';
  const value = data[keys[0]];
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  if (value && typeof value === 'object') {
    return formatStructuredRecord(keys[0], value as Record<string, unknown>);
  }
  return JSON.stringify(value);
}

function formatStructuredRecord(rtype: string, value: Record<string, unknown>): string {
  switch (rtype) {
    case 'MX':
      return `${value.preference} ${value.exchange}`;
    case 'SRV':
      return `${value.priority} ${value.weight} ${value.port} ${value.target}`;
    case 'SOA':
      return `${value.mname} ${value.rname} (serial: ${value.serial})`;
    case 'CAA':
      return `${value.issuer_critical ? '!' : ''}${value.tag} "${value.value}"`;
    default:
      return JSON.stringify(value);
  }
}

/** CSS custom property name for a record type color. */
function typeColorVar(recordType: string): string {
  const rt = recordType.toLowerCase();
  const known = ['a', 'aaaa', 'mx', 'txt', 'ns', 'soa', 'cname', 'caa', 'srv', 'ptr'];
  if (known.includes(rt)) return `var(--rt-${rt})`;
  return 'var(--rt-default)';
}

// ---------------------------------------------------------------------------
// Build comparison data
// ---------------------------------------------------------------------------

function buildComparisons(batches: BatchEvent[]): RecordTypeComparison[] {
  const byType = new Map<string, Lookup[]>();
  for (const batch of batches) {
    let arr = byType.get(batch.record_type);
    if (!arr) {
      arr = [];
      byType.set(batch.record_type, arr);
    }
    arr.push(...batch.lookups);
  }

  const comparisons: RecordTypeComparison[] = [];
  for (const [recordType, lookups] of byType) {
    // Group lookups by server
    const serverMap = new Map<string, Lookup[]>();
    for (const lookup of lookups) {
      const name = formatServer(lookup.name_server);
      let arr = serverMap.get(name);
      if (!arr) {
        arr = [];
        serverMap.set(name, arr);
      }
      arr.push(lookup);
    }

    const servers: ServerGroup[] = Array.from(serverMap.entries()).map(
      ([serverName, lookups]) => ({ serverName, lookups }),
    );

    // Check if all servers agree
    const fingerprints = new Set<string>();
    for (const lookup of lookups) {
      fingerprints.add(answerFingerprint(lookup));
    }
    const allAgree = fingerprints.size <= 1;

    comparisons.push({ recordType, servers, allAgree });
  }

  return comparisons;
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function ServerColumn(props: { server: ServerGroup; maxMs: number }) {
  return (
    <div class="sc-server-column">
      <div class="sc-server-name">{props.server.serverName}</div>
      <For each={props.server.lookups}>
        {(lookup) => {
          const status = getStatusLabel(lookup);
          const time = getResponseTime(lookup);
          const ms = () => getResponseTimeMs(lookup);
          const barWidth = () => {
            const m = ms();
            if (m === null || props.maxMs <= 0) return 0;
            return Math.max(2, (m / props.maxMs) * 100);
          };
          const barColor = () => {
            const m = ms();
            return m !== null ? responseTimeColor(m) : 'transparent';
          };
          return (
            <div class="sc-server-result">
              <div class="sc-result-header">
                <span class={`sc-status ${status.className}`}>{status.text}</span>
                <div class="sc-time-container">
                  <span class="sc-time">{time}</span>
                  <Show when={ms() !== null}>
                    <div
                      class="time-bar"
                      style={{ width: `${barWidth()}%`, 'background-color': barColor() }}
                    />
                  </Show>
                </div>
              </div>
              <Show when={isResponse(lookup.result)}>
                <div class="sc-records">
                  <For each={(lookup.result as { Response: { records: Array<{ data: Record<string, unknown> }> } }).Response.records}>
                    {(record) => (
                      <div class="sc-record-value">{formatRecordData(record.data)}</div>
                    )}
                  </For>
                </div>
              </Show>
              <Show when={isNxDomain(lookup.result)}>
                <div class="sc-records">
                  <div class="sc-record-value sc-nxdomain">NXDOMAIN</div>
                </div>
              </Show>
              <Show when={!isResponse(lookup.result) && !isNxDomain(lookup.result)}>
                <div class="sc-records">
                  <div class="sc-record-value sc-error">
                    {Object.keys(lookup.result)[0] ?? 'Error'}
                  </div>
                </div>
              </Show>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function ComparisonRow(props: { comparison: RecordTypeComparison }) {
  const maxMs = createMemo(() => {
    let max = 0;
    for (const server of props.comparison.servers) {
      for (const lookup of server.lookups) {
        const ms = getResponseTimeMs(lookup);
        if (ms !== null && ms > max) max = ms;
      }
    }
    return max;
  });

  return (
    <div class="sc-type-group">
      <div class="sc-type-header">
        <span class="type-badge" style={{ 'background-color': typeColorVar(props.comparison.recordType) }}>
          {props.comparison.recordType}
        </span>
        <span class={`sc-agreement ${props.comparison.allAgree ? 'agree' : 'diverge'}`}>
          {props.comparison.allAgree ? 'All servers agree' : 'Servers diverge'}
        </span>
      </div>
      <div class="sc-server-grid" style={{ 'grid-template-columns': `repeat(${props.comparison.servers.length}, 1fr)` }}>
        <For each={props.comparison.servers}>
          {(server) => <ServerColumn server={server} maxMs={maxMs()} />}
        </For>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export function ServerComparison(props: ServerComparisonProps) {
  const comparisons = createMemo(() => buildComparisons(props.results));

  return (
    <div class="server-comparison">
      <Show
        when={comparisons().length > 0}
        fallback={
          <div class="sc-empty">
            <p>No results to compare.</p>
            <p class="hint">Query multiple servers (e.g., <code>@cloudflare @google</code>) to see a comparison.</p>
          </div>
        }
      >
        <For each={comparisons()}>
          {(comp) => <ComparisonRow comparison={comp} />}
        </For>
      </Show>
    </div>
  );
}
