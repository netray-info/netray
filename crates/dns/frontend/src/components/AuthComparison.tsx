import { For, Show, createMemo, createSignal, createEffect, onMount, onCleanup } from 'solid-js';
import type { BatchEvent, Lookup, LookupResult } from './ResultsTable';
import { responseTimeMs, responseTimeColor, formatRecordData, getExplanation } from './ResultsTable';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface AuthComparisonProps {
  results: BatchEvent[];
  activeTab: string;
  explain: boolean;
  sort: boolean;
  devOnly: boolean;
  authServers: string[];
}

interface SourceGroup {
  source: string;
  lookups: Lookup[];
}

interface RecordTypeComparison {
  recordType: string;
  sources: SourceGroup[];
  allAgree: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isResponse(result: LookupResult): boolean {
  return 'Response' in result;
}

function isNxDomain(result: LookupResult): boolean {
  return 'NxDomain' in result;
}

function answerFingerprint(lookup: Lookup): string {
  const result = lookup.result;
  if ('Response' in result) {
    const resp = (result as { Response: { records: Array<{ data: Record<string, unknown> }> } }).Response;
    const values = resp.records.map((r) => JSON.stringify(r.data)).sort();
    return `response:${values.join('|')}`;
  }
  if ('NxDomain' in result) return 'nxdomain';
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

function typeColorVar(recordType: string): string {
  const rt = recordType.toLowerCase();
  const known = ['a', 'aaaa', 'mx', 'txt', 'ns', 'soa', 'cname', 'caa', 'srv', 'ptr'];
  if (known.includes(rt)) return `var(--rt-${rt})`;
  return 'var(--rt-default)';
}

const SOURCE_ORDER = ['authoritative', 'recursive'];

function sourceLabel(s: string): string {
  return s === 'authoritative' ? 'Authoritative' : s === 'recursive' ? 'Recursive' : s;
}

function comparisonTier(c: RecordTypeComparison): number {
  if (!c.allAgree) return 0;
  const allNx = c.sources.every((s) => s.lookups.every((l) => isNxDomain(l.result)));
  if (allNx) return 2;
  return 1;
}

// ---------------------------------------------------------------------------
// Build comparison data
// ---------------------------------------------------------------------------

function buildComparisons(batches: BatchEvent[]): RecordTypeComparison[] {
  const byType = new Map<string, Map<string, Lookup[]>>();
  for (const batch of batches) {
    const source = batch.source ?? 'recursive';
    let typeMap = byType.get(batch.record_type);
    if (!typeMap) {
      typeMap = new Map();
      byType.set(batch.record_type, typeMap);
    }
    let arr = typeMap.get(source);
    if (!arr) {
      arr = [];
      typeMap.set(source, arr);
    }
    arr.push(...batch.lookups);
  }

  const comparisons: RecordTypeComparison[] = [];
  for (const [recordType, sourceMap] of byType) {
    const sources: SourceGroup[] = [];
    const sortedKeys = [...sourceMap.keys()].sort(
      (a, b) => SOURCE_ORDER.indexOf(a) - SOURCE_ORDER.indexOf(b)
    );
    for (const source of sortedKeys) {
      sources.push({ source, lookups: sourceMap.get(source)! });
    }

    // Check agreement: compare fingerprints per source
    const perSource = sources.map((s) => {
      const fps = new Set(s.lookups.map(answerFingerprint));
      return [...fps].sort().join('||');
    });
    const allAgree = new Set(perSource).size <= 1;

    comparisons.push({ recordType, sources, allAgree });
  }

  return comparisons;
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function SourceColumn(props: { group: SourceGroup; maxMs: number; recordType: string; explain: boolean }) {
  return (
    <div class="ac-source-column">
      <div class="ac-source-name">{sourceLabel(props.group.source)}</div>
      <For each={props.group.lookups}>
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
            <div class="ac-source-result">
              <div class="ac-result-header">
                <span class={`ac-status ${status.className}`}>{status.text}</span>
                <div class="sc-time-container">
                  <span class="ac-time">{time}</span>
                  <Show when={ms() !== null}>
                    <div
                      class="time-bar"
                      style={{ width: `${barWidth()}%`, 'background-color': barColor() }}
                    />
                  </Show>
                </div>
              </div>
              <Show when={isResponse(lookup.result)}>
                <div class="ac-records">
                  <For each={(lookup.result as { Response: { records: Array<{ data: Record<string, unknown> }> } }).Response.records}>
                    {(record) => {
                      const explanation = () => props.explain ? getExplanation(props.recordType, record.data) : null;
                      return (
                        <div class="ac-record-value">
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
                <div class="ac-records">
                  <div class="ac-record-value ac-nxdomain">NXDOMAIN</div>
                </div>
              </Show>
              <Show when={!isResponse(lookup.result) && !isNxDomain(lookup.result)}>
                <div class="ac-records">
                  <div class="ac-record-value ac-error">
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
    for (const source of props.comparison.sources) {
      for (const lookup of source.lookups) {
        const ms = getResponseTimeMs(lookup);
        if (ms !== null && ms > max) max = ms;
      }
    }
    return max;
  });

  return (
    <div
      class={`ac-type-group${props.isFocused ? ' ac-type-group--focused' : ''}`}
      data-row-key={`ac-${props.index}`}
    >
      <div class="ac-type-header">
        <span class="type-badge" style={{ 'background-color': typeColorVar(props.comparison.recordType) }}>
          {props.comparison.recordType}
        </span>
        <span class={`ac-agreement ${props.comparison.allAgree ? 'agree' : 'diverge'}`}>
          {props.comparison.allAgree ? 'Auth & recursive agree' : 'Auth & recursive diverge'}
        </span>
      </div>
      <div class="ac-source-grid">
        <For each={props.comparison.sources}>
          {(group) => <SourceColumn group={group} maxMs={maxMs()} recordType={props.comparison.recordType} explain={props.explain} />}
        </For>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main export
// ---------------------------------------------------------------------------

export function AuthComparison(props: AuthComparisonProps) {
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
  const [focusedIndex, setFocusedIndex] = createSignal<number | null>(null);
  let containerRef: HTMLDivElement | undefined;

  createEffect(() => {
    if (props.results.length === 0) {
      setFocusedIndex(null);
    }
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (props.activeTab !== 'auth') return;

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
          ?.querySelector(`[data-row-key="ac-${next}"]`)
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
      return;
    }

    if (e.key === 'Escape') {
      if (focusedIndex() !== null) {
        e.preventDefault();
        setFocusedIndex(null);
      }
    }
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    document.removeEventListener('keydown', handleKeyDown);
  });

  return (
    <div class="auth-comparison" ref={containerRef}>
      <Show when={props.authServers.length > 0}>
        <div class="ac-auth-servers">
          <span class="ac-auth-servers-label">Authoritative NS:</span>
          <For each={props.authServers}>
            {(server) => <span class="ac-auth-server-badge">{server}</span>}
          </For>
        </div>
      </Show>
      <Show
        when={comparisons().length > 0}
        fallback={
          <div class="ac-empty">
            <p>No results to compare.</p>
            <p class="hint">Use <code>+auth</code> to compare authoritative vs recursive answers.</p>
          </div>
        }
      >
        <For each={comparisons()}>
          {(comp, i) => <ComparisonRow comparison={comp} index={i()} isFocused={focusedIndex() === i()} explain={props.explain} />}
        </For>
      </Show>
    </div>
  );
}
