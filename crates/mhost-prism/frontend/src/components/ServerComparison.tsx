import { For, Show, createMemo, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import type { BatchEvent, Lookup, LookupResult } from './ResultsTable';
import { responseTimeMs, responseTimeColor, formatRecordData, getExplanation } from './ResultsTable';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ServerComparisonProps {
  results: BatchEvent[];
  activeTab: string;
  explain: boolean;
  sort: boolean;
  devOnly: boolean;
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

/**
 * Median response time in ms for a given formatted server name across all
 * record type groups. Returns null when no successful latency data exists.
 */
function medianLatencyMs(server: string, batches: BatchEvent[]): number | null {
  const times: number[] = [];
  for (const batch of batches) {
    for (const lookup of batch.lookups) {
      if (formatServer(lookup.name_server) !== server) continue;
      const ms = getResponseTimeMs(lookup);
      if (ms !== null) times.push(ms);
    }
  }
  if (times.length === 0) return null;
  times.sort((a, b) => a - b);
  return times[Math.floor(times.length / 2)];
}

/** CSS custom property name for a record type color. */
function typeColorVar(recordType: string): string {
  const rt = recordType.toLowerCase();
  const known = ['a', 'aaaa', 'mx', 'txt', 'ns', 'soa', 'cname', 'caa', 'srv', 'ptr'];
  if (known.includes(rt)) return `var(--rt-${rt})`;
  return 'var(--rt-default)';
}

/** Sort tier: divergences first (0), then normal records (1), then all-NXDOMAIN (2). */
function comparisonTier(c: RecordTypeComparison): number {
  if (!c.allAgree) return 0;
  const allNx = c.servers.every((s) => s.lookups.every((l) => isNxDomain(l.result)));
  if (allNx) return 2;
  return 1;
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

// ---------------------------------------------------------------------------
// Latency summary row (above per-record-type results)
// ---------------------------------------------------------------------------

function LatencySummaryRow(props: { serverNames: string[]; batches: BatchEvent[] }) {
  const maxMs = createMemo(() => {
    let max = 0;
    for (const name of props.serverNames) {
      const m = medianLatencyMs(name, props.batches);
      if (m !== null && m > max) max = m;
    }
    return max;
  });

  return (
    <div class="sc-latency-summary">
      <div class="sc-latency-summary-label">Median latency</div>
      <div class="sc-latency-summary-cells" style={{ 'grid-template-columns': `repeat(${props.serverNames.length}, 1fr)` }}>
        <For each={props.serverNames}>
          {(name) => {
            const ms = () => medianLatencyMs(name, props.batches);
            const barWidth = () => {
              const m = ms();
              if (m === null || maxMs() <= 0) return 0;
              return Math.max(2, (m / maxMs()) * 100);
            };
            const barColor = () => {
              const m = ms();
              return m !== null ? responseTimeColor(m) : 'transparent';
            };
            return (
              <div class="sc-latency-cell">
                <Show when={ms() !== null} fallback={<span class="sc-latency-na">-</span>}>
                  <div class="sc-time-container">
                    <span class="sc-time">{Math.round(ms()!)}ms</span>
                    <div
                      class="time-bar"
                      style={{ width: `${barWidth()}%`, 'background-color': barColor() }}
                    />
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
}

function ServerColumn(props: { server: ServerGroup; maxMs: number; recordType: string; explain: boolean }) {
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
                    {(record) => {
                      const explanation = () => props.explain ? getExplanation(props.recordType, record.data) : null;
                      return (
                        <div class="sc-record-value">
                          {formatRecordData(record.data)}
                          <Show when={explanation()}>
                            <div class="record-explanation">{explanation()}</div>
                          </Show>
                        </div>
                      );
                    }}
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

function ComparisonRow(props: { comparison: RecordTypeComparison; index: number; isFocused: boolean; explain: boolean }) {
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
    <div
      class={`sc-type-group${props.isFocused ? ' sc-type-group--focused' : ''}`}
      data-row-key={`sc-${props.index}`}
    >
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
          {(server) => <ServerColumn server={server} maxMs={maxMs()} recordType={props.comparison.recordType} explain={props.explain} />}
        </For>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export function ServerComparison(props: ServerComparisonProps) {
  const comparisons = createMemo(() => {
    let comps = buildComparisons(props.results);
    if (props.devOnly) comps = comps.filter((c) => !c.allAgree);
    if (props.sort) {
      comps = [...comps].sort((a, b) => {
        const ta = comparisonTier(a);
        const tb = comparisonTier(b);
        if (ta !== tb) return ta - tb;
        return a.recordType.localeCompare(b.recordType);
      });
    }
    return comps;
  });

  /** Unique server names across all results, in insertion order. */
  const serverNames = createMemo(() => {
    const seen = new Set<string>();
    const names: string[] = [];
    for (const batch of props.results) {
      for (const lookup of batch.lookups) {
        const name = formatServer(lookup.name_server);
        if (!seen.has(name)) {
          seen.add(name);
          names.push(name);
        }
      }
    }
    return names;
  });
  const [focusedIndex, setFocusedIndex] = createSignal<number | null>(null);
  let containerRef: HTMLDivElement | undefined;

  // Reset state when results are cleared (new query).
  createEffect(() => {
    if (props.results.length === 0) {
      setFocusedIndex(null);
    }
  });

  // -------------------------------------------------------------------------
  // Keyboard navigation (j / k)
  // -------------------------------------------------------------------------

  function handleKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (props.activeTab !== 'servers') return;

    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');
    if (isEditing) return;

    const count = comparisons().length;
    if (count === 0) return;

    if (e.key === 'j' || e.key === 'k') {
      e.preventDefault();
      const current = focusedIndex();
      let next: number;
      if (e.key === 'j') {
        next = current === null ? 0 : Math.min(current + 1, count - 1);
      } else {
        if (current === null) return;
        next = Math.max(current - 1, 0);
      }
      setFocusedIndex(next);
      requestAnimationFrame(() => {
        containerRef
          ?.querySelector(`[data-row-key="sc-${next}"]`)
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
      return;
    }

    if (e.key === 'Escape') {
      if (focusedIndex() !== null) {
        e.preventDefault();
        setFocusedIndex(null);
      }
      return;
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
  });

  return (
    <div class="server-comparison" ref={containerRef}>
      <Show
        when={comparisons().length > 0}
        fallback={
          <div class="sc-empty">
            <p>No results to compare.</p>
            <p class="hint">Query multiple servers (e.g., <code>@cloudflare @google</code>) to see a comparison.</p>
          </div>
        }
      >
        <Show when={serverNames().length > 1}>
          <LatencySummaryRow serverNames={serverNames()} batches={props.results} />
        </Show>
        <For each={comparisons()}>
          {(comp, i) => <ComparisonRow comparison={comp} index={i()} isFocused={focusedIndex() === i()} explain={props.explain} />}
        </For>
      </Show>
    </div>
  );
}
