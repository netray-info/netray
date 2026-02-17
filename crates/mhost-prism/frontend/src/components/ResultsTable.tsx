import { For, Show, createMemo, createSignal } from 'solid-js';

// ---------------------------------------------------------------------------
// Types — mirrors the mhost-lib Serialize output
// ---------------------------------------------------------------------------

interface RecordData {
  name: string;
  type: string;
  ttl: number;
  data: Record<string, unknown>;
}

interface ResponseResult {
  Response: {
    records: RecordData[];
    response_time: { secs: number; nanos: number };
    valid_until?: string;
  };
}

interface NxDomainResult {
  NxDomain: {
    response_time: { secs: number; nanos: number };
  };
}

// mhost error variants: Timeout, QueryRefused, ServerFailure, NoRecordsFound,
// ResolveError { reason }, ProtoError { reason }, CancelledError, RuntimePanicError
type LookupResult = ResponseResult | NxDomainResult | Record<string, unknown>;

export interface Lookup {
  query: {
    name: string;
    record_type: string;
  };
  name_server: string;
  result: LookupResult;
}

/** Raw batch from backend — lookups is a Lookups struct with inner lookups array. */
interface RawBatchEvent {
  request_id?: string;
  record_type: string;
  lookups: { lookups: Lookup[] };
  completed: number;
  total: number;
}

export interface BatchEvent {
  request_id?: string;
  record_type: string;
  lookups: Lookup[];
  completed: number;
  total: number;
}

/** Parse the raw backend format into a flat BatchEvent. */
export function parseBatchEvent(raw: RawBatchEvent): BatchEvent {
  return {
    request_id: raw.request_id,
    record_type: raw.record_type,
    lookups: raw.lookups?.lookups ?? [],
    completed: raw.completed,
    total: raw.total,
  };
}

export interface DoneStats {
  request_id?: string;
  total_queries: number;
  duration_ms: number;
  warnings: string[];
}

type Status = 'idle' | 'loading' | 'done' | 'error';

interface ResultsTableProps {
  results: BatchEvent[];
  stats: DoneStats | null;
  status: Status;
  error: string | null;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatResponseTime(rt: { secs: number; nanos: number }): string {
  const ms = rt.secs * 1000 + rt.nanos / 1_000_000;
  if (ms < 1) return '<1ms';
  return `${Math.round(ms)}ms`;
}

function formatRecordData(data: Record<string, unknown>): string {
  // mhost serializes record data as e.g. {"A": "1.2.3.4"} or {"MX": {"preference": 10, "exchange": "mail.example.com."}}
  const keys = Object.keys(data);
  if (keys.length === 0) return '';
  const value = data[keys[0]];
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  if (value && typeof value === 'object') {
    // For structured records like MX, SRV, SOA — show a concise representation
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
    case 'TXT':
      if (Array.isArray(value)) return value.join('');
      return JSON.stringify(value);
    case 'CAA':
      return `${value.issuer_critical ? '!' : ''}${value.tag} "${value.value}"`;
    case 'NAPTR':
      return `${value.order} ${value.preference} "${value.flags}" "${value.services}" "${value.regexp}" ${value.replacement}`;
    default:
      return JSON.stringify(value);
  }
}

/** Format name_server string like "udp:1.1.1.1:53,name=Cloudflare 1" for display. */
function formatServer(ns: string): string {
  // Extract the name if present: "udp:1.1.1.1:53,name=Cloudflare 1" -> "Cloudflare 1"
  const nameMatch = ns.match(/name=(.+?)(?:,|$)/);
  if (nameMatch) return nameMatch[1];
  return ns;
}

/** CSS custom property name for a record type color. */
function typeColorVar(recordType: string): string {
  const rt = recordType.toLowerCase();
  const known = ['a', 'aaaa', 'mx', 'txt', 'ns', 'soa', 'cname', 'caa', 'srv', 'ptr'];
  if (known.includes(rt)) return `var(--rt-${rt})`;
  return 'var(--rt-default)';
}

function isResponse(result: LookupResult): result is ResponseResult {
  return 'Response' in result;
}

function isNxDomain(result: LookupResult): result is NxDomainResult {
  return 'NxDomain' in result;
}

/** Check if the result is an mhost error variant (anything that isn't Response or NxDomain). */
function isLookupError(result: LookupResult): boolean {
  return !isResponse(result) && !isNxDomain(result);
}

/** Extract a human-readable message from an mhost error variant. */
function formatLookupError(result: LookupResult): string {
  // mhost error variants serialize as e.g. "Timeout", {"ResolveError":{"reason":"..."}}, etc.
  const keys = Object.keys(result);
  if (keys.length === 0) return 'Unknown error';
  const key = keys[0];
  const val = (result as Record<string, unknown>)[key];
  if (val === null || val === undefined) return key; // unit variants like "Timeout"
  if (typeof val === 'string') return `${key}: ${val}`;
  if (typeof val === 'object' && val !== null && 'reason' in val) {
    return `${key}: ${(val as { reason: string }).reason}`;
  }
  return key;
}

// ---------------------------------------------------------------------------
// Grouped results
// ---------------------------------------------------------------------------

interface GroupedResult {
  recordType: string;
  lookups: Lookup[];
}

function groupByRecordType(batches: BatchEvent[]): GroupedResult[] {
  const map = new Map<string, Lookup[]>();
  for (const batch of batches) {
    const rt = batch.record_type;
    let arr = map.get(rt);
    if (!arr) {
      arr = [];
      map.set(rt, arr);
    }
    arr.push(...batch.lookups);
  }
  return Array.from(map.entries()).map(([recordType, lookups]) => ({ recordType, lookups }));
}

// ---------------------------------------------------------------------------
// RecordGroup (collapsible section)
// ---------------------------------------------------------------------------

function RecordGroup(props: { group: GroupedResult }) {
  const [collapsed, setCollapsed] = createSignal(false);

  const totalRecords = createMemo(() => {
    let count = 0;
    for (const lookup of props.group.lookups) {
      if (isResponse(lookup.result)) {
        count += lookup.result.Response.records.length;
      } else {
        count += 1; // NxDomain or Error count as one row
      }
    }
    return count;
  });

  return (
    <div class="record-group">
      <button
        class="record-group-header"
        onClick={() => setCollapsed((c) => !c)}
        aria-expanded={!collapsed()}
      >
        <span class="type-badge" style={{ 'background-color': typeColorVar(props.group.recordType) }}>
          {props.group.recordType}
        </span>
        <span class="record-count">{totalRecords()} record{totalRecords() !== 1 ? 's' : ''}</span>
        <span class="collapse-indicator">{collapsed() ? '+' : '\u2212'}</span>
      </button>
      <Show when={!collapsed()}>
        <table class="results-table">
          <thead>
            <tr>
              <th>Name</th>
              <th>TTL</th>
              <th>Value</th>
              <th>Server</th>
              <th>Time</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.group.lookups}>
              {(lookup) => <LookupRows lookup={lookup} />}
            </For>
          </tbody>
        </table>
      </Show>
    </div>
  );
}

// ---------------------------------------------------------------------------
// LookupRows — rows for a single lookup (may have multiple records)
// ---------------------------------------------------------------------------

function LookupRows(props: { lookup: Lookup }) {
  const server = createMemo(() => formatServer(props.lookup.name_server));

  return (
    <>
      <Show when={isResponse(props.lookup.result)}>
        {(_) => {
          const resp = (props.lookup.result as ResponseResult).Response;
          return (
            <For each={resp.records}>
              {(record, i) => (
                <tr>
                  <td>{record.name}</td>
                  <td class="ttl-value">{record.ttl}s</td>
                  <td class="record-value">{formatRecordData(record.data)}</td>
                  <td>{i() === 0 ? server() : ''}</td>
                  <td>{i() === 0 ? formatResponseTime(resp.response_time) : ''}</td>
                </tr>
              )}
            </For>
          );
        }}
      </Show>
      <Show when={isNxDomain(props.lookup.result)}>
        {(_) => {
          const nx = (props.lookup.result as NxDomainResult).NxDomain;
          return (
            <tr class="row-nxdomain">
              <td>{props.lookup.query.name}</td>
              <td>-</td>
              <td class="nxdomain-value">NXDOMAIN</td>
              <td>{server()}</td>
              <td>{formatResponseTime(nx.response_time)}</td>
            </tr>
          );
        }}
      </Show>
      <Show when={isLookupError(props.lookup.result)}>
        <tr class="row-error">
          <td>{props.lookup.query.name}</td>
          <td>-</td>
          <td class="error-value">{formatLookupError(props.lookup.result)}</td>
          <td>{server()}</td>
          <td>-</td>
        </tr>
      </Show>
    </>
  );
}

// ---------------------------------------------------------------------------
// JSON View
// ---------------------------------------------------------------------------

function JsonView(props: { results: BatchEvent[]; stats: DoneStats | null }) {
  const json = createMemo(() => {
    const data: { batches: BatchEvent[]; stats: DoneStats | null } = {
      batches: props.results,
      stats: props.stats,
    };
    return JSON.stringify(data, null, 2);
  });

  return (
    <div class="json-view">
      <pre><code>{json()}</code></pre>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function ResultsTable(props: ResultsTableProps & { activeTab: 'results' | 'json' }) {
  const groups = createMemo(() => groupByRecordType(props.results));

  return (
    <div class="results-container">
      {/* Error banner */}
      <Show when={props.error}>
        <div class="error-banner">
          <span class="error-icon">!</span>
          <span>{props.error}</span>
        </div>
      </Show>

      <Show
        when={props.activeTab === 'results'}
        fallback={<JsonView results={props.results} stats={props.stats} />}
      >
        {/* Loading indicator */}
        <Show when={props.status === 'loading' && props.results.length === 0}>
          <div class="loading">
            <div class="loading-spinner" />
            <span>Resolving...</span>
          </div>
        </Show>

        {/* No results yet and idle */}
        <Show when={props.status === 'idle' && props.results.length === 0}>
          <div class="results-empty">
            <p>Enter a DNS query above to get started.</p>
            <p class="hint">
              Try: <code>example.com A AAAA @cloudflare +tls</code>
            </p>
          </div>
        </Show>

        {/* Record groups */}
        <For each={groups()}>
          {(group) => <RecordGroup group={group} />}
        </For>

        {/* Progress bar during loading */}
        <Show when={props.status === 'loading' && props.results.length > 0}>
          <div class="loading-inline">
            <div class="loading-spinner small" />
            <span>
              {props.results.length > 0
                ? `${props.results[props.results.length - 1].completed} / ${props.results[props.results.length - 1].total} batches`
                : 'Loading...'}
            </span>
          </div>
        </Show>
      </Show>

      {/* Status bar */}
      <Show when={props.status === 'done' && props.stats}>
        <div class="status-bar">
          <span>{props.stats!.total_queries} queries</span>
          <span class="status-separator">/</span>
          <span>{props.results.length} batches</span>
          <span class="status-separator">/</span>
          <span>{props.stats!.duration_ms}ms</span>
          <Show when={props.stats!.warnings.length > 0}>
            <span class="status-separator">/</span>
            <span class="status-errors">{props.stats!.warnings.length} warnings</span>
          </Show>
        </div>
      </Show>
    </div>
  );
}
