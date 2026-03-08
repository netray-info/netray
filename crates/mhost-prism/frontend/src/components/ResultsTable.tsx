import { For, Show, createMemo, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import { ServerComparison } from './ServerComparison';
import type { IpInfo } from '../App';

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
export type LookupResult = ResponseResult | NxDomainResult | Record<string, unknown>;

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
  transport?: string;
  source?: string;
}

export interface BatchEvent {
  request_id?: string;
  record_type: string;
  lookups: Lookup[];
  completed: number;
  total: number;
  transport?: string;
  source?: string;
}

/** Parse the raw backend format into a flat BatchEvent. */
export function parseBatchEvent(raw: RawBatchEvent): BatchEvent {
  const ev: BatchEvent = {
    request_id: raw.request_id,
    record_type: raw.record_type,
    lookups: raw.lookups?.lookups ?? [],
    completed: raw.completed,
    total: raw.total,
  };
  if (raw.transport) ev.transport = raw.transport;
  if (raw.source) ev.source = raw.source;
  return ev;
}

export interface DoneStats {
  request_id?: string;
  total_queries: number;
  duration_ms: number;
  warnings: string[];
  transport?: string;
  dnssec?: boolean;
  cache_key?: string;
}

type Status = 'idle' | 'loading' | 'done' | 'error';
type ActiveTab = 'results' | 'servers';

interface ResultsTableProps {
  results: BatchEvent[];
  stats: DoneStats | null;
  status: Status;
  error: string | null;
  activeTab: ActiveTab;
  hideNx: boolean;
  compact: boolean;
  devOnly: boolean;
  sort: boolean;
  explain: boolean;
  expandAll?: number;
  collapseAll?: number;
  ifconfigUrl?: string | null;
  enrichments?: Record<string, IpInfo>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatResponseTime(rt: { secs: number; nanos: number }): string {
  const ms = rt.secs * 1000 + rt.nanos / 1_000_000;
  if (ms < 1) return '<1ms';
  return `${Math.round(ms)}ms`;
}

/** Convert {secs, nanos} to milliseconds, or null if undefined. */
export function responseTimeMs(rt: { secs: number; nanos: number } | undefined): number | null {
  if (!rt) return null;
  return rt.secs * 1000 + rt.nanos / 1_000_000;
}

/** Color CSS var based on absolute latency: green <50ms, yellow 50-200ms, red >200ms. */
export function responseTimeColor(ms: number): string {
  if (ms < 50) return 'var(--success)';
  if (ms <= 200) return 'var(--warning)';
  return 'var(--error)';
}

/** Extract response_time from a lookup result (Response or NxDomain). */
function lookupResponseTime(lookup: Lookup): { secs: number; nanos: number } | undefined {
  if (isResponse(lookup.result)) {
    return (lookup.result as ResponseResult).Response.response_time;
  }
  if (isNxDomain(lookup.result)) {
    return (lookup.result as NxDomainResult).NxDomain.response_time;
  }
  return undefined;
}

export function formatRecordData(data: Record<string, unknown>): string {
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

/** Map of record type → injected *_human field name. */
const humanFields: Record<string, string> = {
  'CAA': 'caa_human', 'MX': 'mx_human', 'SOA': 'soa_human',
  'SVCB': 'svcb_human', 'HTTPS': 'svcb_human', 'TLSA': 'tlsa_human',
  'NAPTR': 'naptr_human', 'DNSKEY': 'dnskey_human', 'DS': 'ds_human',
  'NSEC': 'nsec_human',
};

/** Map of record type → injected *_explain field name. */
const explainFields: Record<string, string> = {
  'CAA': 'caa_explain', 'MX': 'mx_explain', 'SOA': 'soa_explain',
  'SVCB': 'svcb_explain', 'HTTPS': 'svcb_explain', 'TLSA': 'tlsa_explain',
  'NAPTR': 'naptr_explain', 'DNSKEY': 'dnskey_explain', 'DS': 'ds_explain',
  'NSEC': 'nsec_explain',
};

function formatStructuredRecord(rtype: string, value: Record<string, unknown>): string {
  // Prefer backend-injected *_human field (concise formatted display)
  const hf = humanFields[rtype];
  if (hf && typeof value[hf] === 'string') return value[hf] as string;

  // Fallback for types without enrichment
  switch (rtype) {
    case 'SRV':
      return `${value.priority} ${value.weight} ${value.port} ${value.target}`;
    case 'TXT': {
      const txt = value as Record<string, unknown>;
      if (typeof txt.txt_human === 'string') return txt.txt_human;
      if (typeof txt.txt_string === 'string') return txt.txt_string;
      if (Array.isArray(txt.txt_data)) return JSON.stringify(txt.txt_data);
      return JSON.stringify(value);
    }
    default:
      return JSON.stringify(value);
  }
}

/** Format name_server string like "udp:1.1.1.1:53,name=Cloudflare 1" for display. */
export function formatServer(ns: string): string {
  // Extract the name if present: "udp:1.1.1.1:53,name=Cloudflare 1" -> "Cloudflare 1"
  const nameMatch = ns.match(/name=(.+?)(?:,|$)/);
  if (nameMatch) return nameMatch[1];
  return ns;
}

/** Extract the transport protocol from a name_server string. */
function extractTransport(ns: string): string {
  const match = ns.match(/^(\w+):/);
  return match ? match[1].toUpperCase() : '';
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

/** Return the *_explain educational text for a record, or null if none. */
export function getExplanation(rtype: string, data: Record<string, unknown>): string | null {
  const keys = Object.keys(data);
  if (keys.length === 0) return null;
  const value = data[keys[0]];

  if (rtype === 'TXT' || keys[0] === 'TXT') {
    const txtObj = value as Record<string, unknown>;
    return typeof txtObj?.txt_explain === 'string' ? txtObj.txt_explain : null;
  }

  if (value && typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    const field = explainFields[rtype] || explainFields[keys[0]];
    if (field && typeof obj[field] === 'string') return obj[field] as string;
  }

  return null;
}


// ---------------------------------------------------------------------------
// IP enrichment components
// ---------------------------------------------------------------------------

/** Extract the raw IP string from A/AAAA record data. */
function extractIpFromData(data: Record<string, unknown>): string | null {
  const val = data['A'] ?? data['AAAA'];
  return typeof val === 'string' ? val : null;
}

/** Render an IP address as a clickable link (when ifconfigUrl is set) or plain text. */
function IpValue(props: { ip: string; ifconfigUrl?: string | null }) {
  if (props.ifconfigUrl) {
    const href = `${props.ifconfigUrl}/?ip=${encodeURIComponent(props.ip)}`;
    return <a class="ip-link" href={href} target="_blank" rel="noopener noreferrer" onClick={(e) => e.stopPropagation()}>{props.ip}</a>;
  }
  return <>{props.ip}</>;
}

/** Render small badge pills for IP enrichment metadata. */
function IpBadges(props: { info: IpInfo }) {
  const badges: Array<{ label: string; cls: string }> = [];
  const cloud = props.info.cloud;
  if (cloud?.provider) badges.push({ label: cloud.provider, cls: 'ip-badge--cloud' });
  else if (props.info.ip_type) badges.push({ label: props.info.ip_type, cls: props.info.ip_type === 'residential' ? 'ip-badge--residential' : 'ip-badge--datacenter' });
  if (props.info.is_tor) badges.push({ label: 'Tor', cls: 'ip-badge--threat' });
  if (props.info.is_vpn) badges.push({ label: 'VPN', cls: 'ip-badge--threat' });
  if (props.info.is_spamhaus) badges.push({ label: 'Spamhaus', cls: 'ip-badge--threat' });
  if (props.info.is_c2) badges.push({ label: 'C2', cls: 'ip-badge--threat' });
  if (props.info.org && badges.length === 0) badges.push({ label: props.info.org, cls: 'ip-badge--datacenter' });
  if (badges.length === 0) return null;
  return (
    <span class="ip-badges">
      <For each={badges}>
        {(b) => <span class={`ip-badge ${b.cls}`}>{b.label}</span>}
      </For>
    </span>
  );
}

// ---------------------------------------------------------------------------
// Agreement detection
// ---------------------------------------------------------------------------

/** Returns true if all lookups in a group return identical results (all NxDomain or all
 *  Response with the same record values). Used for compact rendering. */
export function lookupsAgree(lookups: Lookup[]): boolean {
  if (lookups.length < 2) return false;

  // All NxDomain → trivially agree
  if (lookups.every((l) => isNxDomain(l.result))) return true;

  // All Response with same record data → agree (TTL may differ, order may differ)
  const responses = lookups.filter((l) => isResponse(l.result));
  if (responses.length !== lookups.length) return false; // mixed outcome

  const normalize = (l: Lookup) => {
    const recs = (l.result as ResponseResult).Response.records.map((r) => JSON.stringify(r.data));
    recs.sort();
    return recs.join('\n');
  };

  const ref = normalize(responses[0]);
  return responses.every((l) => normalize(l) === ref);
}

/** Returns true if servers returned different results for this group. */
export function hasDeviation(lookups: Lookup[]): boolean {
  if (lookups.length < 2) return false;
  return !lookupsAgree(lookups);
}

/** Sort tier: 0 = deviation, 1 = normal records, 2 = all-NxDomain. */
function groupTier(g: GroupedResult): number {
  if (hasDeviation(g.lookups)) return 0;
  if (g.lookups.every((l) => isNxDomain(l.result))) return 2;
  return 1;
}

// ---------------------------------------------------------------------------
// Grouped results
// ---------------------------------------------------------------------------

interface GroupedResult {
  recordType: string;
  lookups: Lookup[];
}

export function groupByRecordType(batches: BatchEvent[]): GroupedResult[] {
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
// Row key helpers
// ---------------------------------------------------------------------------

/** Check if a row key refers to an expandable row (Response record, not NxDomain/Error). */
function isExpandableKey(key: string): boolean {
  return !key.endsWith(':nx') && !key.endsWith(':err');
}

// ---------------------------------------------------------------------------
// TimeCell — response time with relative bar
// ---------------------------------------------------------------------------

function TimeCell(props: { rt: { secs: number; nanos: number }; maxMs: number }) {
  const ms = () => responseTimeMs(props.rt) ?? 0;
  const text = () => formatResponseTime(props.rt);
  const color = () => responseTimeColor(ms());
  const widthPct = () => props.maxMs > 0 ? Math.max(2, (ms() / props.maxMs) * 100) : 0;

  return (
    <div class="time-cell">
      <span class="time-text">{text()}</span>
      <div
        class="time-bar"
        style={{ width: `${widthPct()}%`, 'background-color': color() }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// RecordGroup (collapsible section)
// ---------------------------------------------------------------------------

function RecordGroup(props: {
  group: GroupedResult;
  focusedKey: string | null;
  expandedKeys: Set<string>;
  onRowClick: (key: string) => void;
  compact: boolean;
  explain: boolean;
  ifconfigUrl?: string | null;
  enrichments?: Record<string, IpInfo>;
}) {
  const [collapsed, setCollapsed] = createSignal(false);

  const deviated = createMemo(() => hasDeviation(props.group.lookups));
  const agreed = createMemo(() => props.compact && lookupsAgree(props.group.lookups));

  // In compact+agree mode render only the first lookup; all others are identical.
  const displayLookups = createMemo(() =>
    agreed() ? [props.group.lookups[0]] : props.group.lookups,
  );

  // Label shown in the Server column when all servers agree.
  const serverAgreementLabel = createMemo(() =>
    agreed() ? `${props.group.lookups.length}/${props.group.lookups.length} servers` : null,
  );

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

  const maxResponseTimeMs = createMemo(() => {
    let max = 0;
    for (const lookup of props.group.lookups) {
      const ms = responseTimeMs(lookupResponseTime(lookup));
      if (ms !== null && ms > max) max = ms;
    }
    return max;
  });

  const tableId = `rg-table-${props.group.recordType.toLowerCase()}`;

  return (
    <div class="record-group">
      <button
        class="record-group-header"
        onClick={() => setCollapsed((c) => !c)}
        aria-expanded={!collapsed()}
        aria-controls={tableId}
      >
        <span class="type-badge" style={{ 'background-color': typeColorVar(props.group.recordType) }}>
          {props.group.recordType}
        </span>
        <span class="record-count">{totalRecords()} record{totalRecords() !== 1 ? 's' : ''}</span>
        <Show when={deviated()}>
          <span class="deviation-badge" aria-label="Results diverge">{'\u26A0'} differs</span>
        </Show>
        <Show when={agreed()}>
          <span class="agree-badge" aria-label="All servers agree">{'\u2713'} agree</span>
        </Show>
        <span class="collapse-indicator">{collapsed() ? '+' : '\u2212'}</span>
      </button>
      <Show when={!collapsed()}>
        <table class="results-table" id={tableId}>
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
            <For each={displayLookups()}>
              {(lookup, i) => (
                <LookupRows
                  lookup={lookup}
                  recordType={props.group.recordType}
                  lookupIndex={i()}
                  focusedKey={props.focusedKey}
                  expandedKeys={props.expandedKeys}
                  onRowClick={props.onRowClick}
                  maxResponseTimeMs={maxResponseTimeMs()}
                  serverOverride={serverAgreementLabel()}
                  explain={props.explain}
                  ifconfigUrl={props.ifconfigUrl}
                  enrichments={props.enrichments}
                />
              )}
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

function LookupRows(props: {
  lookup: Lookup;
  recordType: string;
  lookupIndex: number;
  focusedKey: string | null;
  expandedKeys: Set<string>;
  onRowClick: (key: string) => void;
  maxResponseTimeMs: number;
  serverOverride?: string | null;
  explain: boolean;
  ifconfigUrl?: string | null;
  enrichments?: Record<string, IpInfo>;
}) {
  const server = createMemo(() => props.serverOverride ?? formatServer(props.lookup.name_server));
  const transport = createMemo(() => extractTransport(props.lookup.name_server));

  return (
    <>
      <Show when={isResponse(props.lookup.result)}>
        {(_) => {
          const resp = (props.lookup.result as ResponseResult).Response;
          return (
            <For each={resp.records}>
              {(record, i) => {
                const rowKey = () => `${props.recordType}:${props.lookupIndex}:${i()}`;
                const explanation = () => props.explain ? getExplanation(props.recordType, record.data) : null;
                const isExpanded = () => props.expandedKeys.has(rowKey());
                const isFocused = () => props.focusedKey === rowKey();

                return (
                  <>
                    <tr
                      data-row-key={rowKey()}
                      class={`expandable-row navigable-row ${isExpanded() ? 'expanded' : ''} ${isFocused() ? 'row-focused' : ''}${i() === 0 && props.lookupIndex > 0 ? ' lookup-separator' : ''}`}
                      tabIndex={0}
                      role="row"
                      aria-expanded={isExpanded()}
                      onClick={() => props.onRowClick(rowKey())}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          props.onRowClick(rowKey());
                        }
                      }}
                    >
                      <td data-label="Name">{record.name}</td>
                      <td data-label="TTL" class="ttl-value">{record.ttl}s</td>
                      <td data-label="Value" class="record-value">
                        {(() => {
                          const ip = (props.recordType === 'A' || props.recordType === 'AAAA') ? extractIpFromData(record.data) : null;
                          if (ip) {
                            const info = props.enrichments?.[ip];
                            return <>
                              <IpValue ip={ip} ifconfigUrl={props.ifconfigUrl} />
                              <Show when={info}>{(i) => <IpBadges info={i()} />}</Show>
                            </>;
                          }
                          return formatRecordData(record.data);
                        })()}
                        <Show when={explanation()}>
                          <div class="record-explanation">{explanation()}</div>
                        </Show>
                      </td>
                      <td data-label="Server" class={i() > 0 ? 'card-hide-empty' : ''}>{i() === 0 ? server() : ''}</td>
                      <td data-label="Time" class={i() > 0 ? 'card-hide-empty' : ''}>
                        {i() === 0 ? <TimeCell rt={resp.response_time} maxMs={props.maxResponseTimeMs} /> : ''}
                      </td>
                    </tr>
                    <Show when={isExpanded()}>
                      <tr class="detail-row">
                        <td colSpan={5}>
                          <div class="detail-content">
                            <div class="detail-grid">
                              <div class="detail-section">
                                <h4>Record Details</h4>
                                <dl class="detail-list">
                                  <dt>Name</dt><dd>{record.name}</dd>
                                  <dt>Type</dt><dd>{record.type ?? props.recordType}</dd>
                                  <dt>TTL</dt><dd>{record.ttl}s ({formatTTLHuman(record.ttl)})</dd>
                                  <dt>Data</dt><dd class="detail-data">{JSON.stringify(record.data, null, 2)}</dd>
                                </dl>
                              </div>
                              <div class="detail-section">
                                <h4>Server Info</h4>
                                <dl class="detail-list">
                                  <dt>Server</dt><dd>{server()}</dd>
                                  <Show when={transport()}>
                                    <dt>Transport</dt><dd>{transport()}</dd>
                                  </Show>
                                  <dt>Response Time</dt><dd>{formatResponseTime(resp.response_time)}</dd>
                                  <dt>Raw Name Server</dt><dd class="detail-data">{props.lookup.name_server}</dd>
                                </dl>
                              </div>
                            </div>
                          </div>
                        </td>
                      </tr>
                    </Show>
                  </>
                );
              }}
            </For>
          );
        }}
      </Show>
      <Show when={isNxDomain(props.lookup.result)}>
        {(_) => {
          const nx = (props.lookup.result as NxDomainResult).NxDomain;
          const rowKey = `${props.recordType}:${props.lookupIndex}:nx`;
          return (
            <tr
              data-row-key={rowKey}
              class={`row-nxdomain navigable-row ${props.focusedKey === rowKey ? 'row-focused' : ''}${props.lookupIndex > 0 ? ' lookup-separator' : ''}`}
              onClick={() => props.onRowClick(rowKey)}
            >
              <td data-label="Name">{props.lookup.query.name}</td>
              <td data-label="TTL">-</td>
              <td data-label="Value" class="nxdomain-value">NXDOMAIN</td>
              <td data-label="Server">{server()}</td>
              <td data-label="Time"><TimeCell rt={nx.response_time} maxMs={props.maxResponseTimeMs} /></td>
            </tr>
          );
        }}
      </Show>
      <Show when={isLookupError(props.lookup.result)}>
        {(_) => {
          const rowKey = `${props.recordType}:${props.lookupIndex}:err`;
          return (
            <tr
              data-row-key={rowKey}
              class={`row-error navigable-row ${props.focusedKey === rowKey ? 'row-focused' : ''}${props.lookupIndex > 0 ? ' lookup-separator' : ''}`}
              onClick={() => props.onRowClick(rowKey)}
            >
              <td data-label="Name">{props.lookup.query.name}</td>
              <td data-label="TTL">-</td>
              <td data-label="Value" class="error-value">{formatLookupError(props.lookup.result)}</td>
              <td data-label="Server">{server()}</td>
              <td data-label="Time">-</td>
            </tr>
          );
        }}
      </Show>
    </>
  );
}

/** Format TTL as human-readable duration. */
function formatTTLHuman(ttl: number): string {
  if (ttl < 60) return `${ttl} seconds`;
  if (ttl < 3600) {
    const m = Math.floor(ttl / 60);
    const s = ttl % 60;
    return s > 0 ? `${m}m ${s}s` : `${m} minute${m > 1 ? 's' : ''}`;
  }
  if (ttl < 86400) {
    const h = Math.floor(ttl / 3600);
    const m = Math.floor((ttl % 3600) / 60);
    return m > 0 ? `${h}h ${m}m` : `${h} hour${h > 1 ? 's' : ''}`;
  }
  const d = Math.floor(ttl / 86400);
  const h = Math.floor((ttl % 86400) / 3600);
  return h > 0 ? `${d}d ${h}h` : `${d} day${d > 1 ? 's' : ''}`;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function ResultsTable(props: ResultsTableProps) {
  const groups = createMemo(() => groupByRecordType(props.results));

  // Apply view filters and optional sort on top of the grouped data
  const displayGroups = createMemo(() => {
    let gs = groups();
    if (props.hideNx) {
      gs = gs.filter((g) => g.lookups.some((l) => !isNxDomain(l.result)));
    }
    if (props.devOnly) {
      gs = gs.filter((g) => hasDeviation(g.lookups));
    }
    if (props.sort) {
      gs = [...gs].sort((a, b) => {
        const td = groupTier(a) - groupTier(b);
        if (td !== 0) return td;
        return a.recordType.localeCompare(b.recordType);
      });
    }
    return gs;
  });

  const [focusedKey, setFocusedKey] = createSignal<string | null>(null);
  const [expandedKeys, setExpandedKeys] = createSignal<Set<string>>(new Set());
  let containerRef: HTMLDivElement | undefined;

  // -------------------------------------------------------------------------
  // Row navigation helpers
  // -------------------------------------------------------------------------

  /** Query the DOM for all visible navigable row keys, in document order. */
  function getVisibleRowKeys(): string[] {
    if (!containerRef) return [];
    const rows = containerRef.querySelectorAll<HTMLElement>('[data-row-key]');
    return Array.from(rows).map((r) => r.dataset.rowKey!);
  }

  function toggleExpanded(key: string) {
    setExpandedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function handleRowClick(key: string) {
    setFocusedKey(key);
    if (isExpandableKey(key)) {
      toggleExpanded(key);
    }
  }

  // -------------------------------------------------------------------------
  // Keyboard navigation (j/k/Enter/Escape)
  // -------------------------------------------------------------------------

  function handleKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (props.activeTab !== 'results') return;

    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');
    if (isEditing) return;

    if (e.key === 'j' || e.key === 'k') {
      e.preventDefault();
      const keys = getVisibleRowKeys();
      if (keys.length === 0) return;

      const current = focusedKey();
      const currentIdx = current ? keys.indexOf(current) : -1;

      let nextIdx: number;
      if (e.key === 'j') {
        nextIdx = currentIdx < keys.length - 1 ? currentIdx + 1 : currentIdx;
        if (currentIdx === -1) nextIdx = 0;
      } else {
        if (currentIdx === -1) return; // k with no focus does nothing
        nextIdx = currentIdx > 0 ? currentIdx - 1 : 0;
      }

      const nextKey = keys[nextIdx];
      setFocusedKey(nextKey);
      requestAnimationFrame(() => {
        containerRef
          ?.querySelector(`[data-row-key="${nextKey}"]`)
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
      return;
    }

    if (e.key === 'Enter' && focusedKey()) {
      e.preventDefault();
      if (isExpandableKey(focusedKey()!)) {
        toggleExpanded(focusedKey()!);
      }
      return;
    }

    if (e.key === 'Escape' && focusedKey()) {
      e.preventDefault();
      setFocusedKey(null);
      return;
    }
  }

  // Reset navigation state when results are cleared (new query)
  createEffect(() => {
    if (props.results.length === 0) {
      setFocusedKey(null);
      setExpandedKeys(new Set<string>());
    }
  });

  // Expand all: collect all expandable keys from visible rows
  createEffect(() => {
    const trigger = props.expandAll;
    if (trigger && trigger > 0) {
      requestAnimationFrame(() => {
        const keys = getVisibleRowKeys().filter(isExpandableKey);
        setExpandedKeys(new Set(keys));
      });
    }
  });

  // Collapse all: clear expanded keys
  createEffect(() => {
    const trigger = props.collapseAll;
    if (trigger && trigger > 0) {
      setExpandedKeys(new Set<string>());
    }
  });

  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
  });

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  return (
    <div class="results-container" ref={containerRef}>
      {/* Error banner */}
      <Show when={props.error}>
        <div class="error-banner">
          <span class="error-icon">!</span>
          <span>{props.error}</span>
        </div>
      </Show>

      <Show when={props.activeTab === 'results'}>
        {/* Skeleton loading placeholder */}
        <Show when={props.status === 'loading' && props.results.length === 0}>
          <div class="record-group" role="status" aria-live="polite">
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
                {[0, 1, 2, 3].map(() => (
                  <tr class="skeleton-row">
                    <td><div class="skeleton-bar skeleton-bar--name" /></td>
                    <td><div class="skeleton-bar skeleton-bar--ttl" /></td>
                    <td><div class="skeleton-bar skeleton-bar--value" /></td>
                    <td><div class="skeleton-bar skeleton-bar--server" /></td>
                    <td><div class="skeleton-bar skeleton-bar--time" /></td>
                  </tr>
                ))}
              </tbody>
            </table>
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
        <For each={displayGroups()}>
          {(group) => (
            <RecordGroup
              group={group}
              focusedKey={focusedKey()}
              expandedKeys={expandedKeys()}
              onRowClick={handleRowClick}
              compact={props.compact}
              explain={props.explain}
              ifconfigUrl={props.ifconfigUrl}
              enrichments={props.enrichments}
            />
          )}
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

      <Show when={props.activeTab === 'servers'}>
        <ServerComparison results={props.results} activeTab={props.activeTab} explain={props.explain} sort={props.sort} devOnly={props.devOnly} />
      </Show>

    </div>
  );
}
