import { createSignal, createEffect, onMount, onCleanup, For, Show } from 'solid-js';
import type { IpInfo } from '../App';

// ---------------------------------------------------------------------------
// Types (mirror the Rust backend structs in dns_trace.rs)
// ---------------------------------------------------------------------------

export interface DnsRecord {
  name: string;
  ttl: number;
  record_type: string;
  rdata: string;
}

export type ServerOutcome =
  | { type: 'referral' }
  | { type: 'answer' }
  | { type: 'error'; message: string };

export interface ServerResult {
  server_ip: string;
  server_name?: string;
  latency_ms: number;
  outcome: ServerOutcome;
  answer_records?: DnsRecord[];
  referral_ns?: string[];
  authority_zone?: string;
}

export interface ReferralGroup {
  ns_names: string[];
  servers: string[];
  is_majority: boolean;
}

export interface TraceHop {
  level: number;
  zone: string;
  servers_queried: number;
  server_results: ServerResult[];
  referral_groups: ReferralGroup[];
  is_final: boolean;
}

export interface TraceDoneStats {
  request_id: string;
  duration_ms: number;
  hops: number;
  cache_key?: string;
}

// ---------------------------------------------------------------------------
// NsList — truncated list of NS names
// ---------------------------------------------------------------------------

const NS_PREVIEW = 3;

function NsList(props: { names: string[] }) {
  const [expanded, setExpanded] = createSignal(false);
  const visible = () => expanded() ? props.names : props.names.slice(0, NS_PREVIEW);
  const extra   = () => props.names.length - NS_PREVIEW;

  return (
    <span class="trace-ns-list">
      <For each={visible()}>
        {(name, i) => (
          <>
            <span class="trace-ns-name">{name}</span>
            <Show when={i() < visible().length - 1}>
              <span class="trace-ns-sep">, </span>
            </Show>
          </>
        )}
      </For>
      <Show when={!expanded() && extra() > 0}>
        <button
          class="trace-ns-more"
          onClick={(e) => { e.stopPropagation(); setExpanded(true); }}
        >
          +{extra()} more
        </button>
      </Show>
    </span>
  );
}

// ---------------------------------------------------------------------------
// ServerRow — one row in the expanded per-server detail panel
// ---------------------------------------------------------------------------

function isSafeUrl(url: string): boolean {
  return url.startsWith('https://') || url.startsWith('http://');
}

function ServerRow(props: { result: ServerResult; ifconfigUrl?: string | null; enrichments?: Record<string, IpInfo> }) {
  const r = props.result;
  const outcomeType = r.outcome.type;

  const info = () => props.enrichments?.[r.server_ip];

  return (
    <div class={`trace-server-row trace-server-row--${outcomeType}`}>
      <span class="trace-server-ip" title={r.server_name}>
        {props.ifconfigUrl && isSafeUrl(props.ifconfigUrl)
          ? <a class="ip-link" href={`${props.ifconfigUrl}/?ip=${encodeURIComponent(r.server_ip)}`} target="_blank" rel="noopener noreferrer" onClick={(e) => e.stopPropagation()}>{r.server_ip}</a>
          : r.server_ip}
      </span>
      <Show when={r.server_name}>
        <span class="trace-server-name">{r.server_name}</span>
      </Show>
      <Show when={info()?.org}>
        <span class="trace-server-org">({info()!.org})</span>
      </Show>
      <span class="trace-server-latency">{r.latency_ms.toFixed(1)}ms</span>
      <span class={`trace-server-outcome trace-server-outcome--${outcomeType}`}>
        {outcomeType === 'referral' ? 'referral'
          : outcomeType === 'answer' ? 'answer'
          : (r.outcome as { type: 'error'; message: string }).message}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// HopCard — one card per delegation level
// ---------------------------------------------------------------------------

interface HopCardProps {
  hop: TraceHop;
  index: number;
  isFocused: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  onClick: () => void;
  ifconfigUrl?: string | null;
  enrichments?: Record<string, IpInfo>;
}

function HopCard(props: HopCardProps) {
  const hop = props.hop;

  const hasDivergence = () => hop.referral_groups.length > 1;
  const majority      = () => hop.referral_groups.find((g) => g.is_majority);
  const minorities    = () => hop.referral_groups.filter((g) => !g.is_majority);

  const cardClass = () => {
    const parts = ['trace-hop'];
    if (hop.is_final)      parts.push('trace-hop--final');
    if (hasDivergence())   parts.push('trace-hop--diverged');
    if (props.isFocused)   parts.push('trace-hop--focused');
    return parts.join(' ');
  };

  return (
    <div
      class={cardClass()}
      data-row-key={`hop-${props.index}`}
      tabIndex={0}
      role="button"
      aria-label={`Hop ${props.index + 1}: ${hop.zone}${hop.is_final ? ', final answer' : ''}`}
      onClick={props.onClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          props.onClick();
        }
      }}
    >
      <div class="trace-hop-header">
        <span class="trace-hop-number">{props.index + 1}</span>
        <span class="trace-hop-zone">{hop.zone}</span>
        <Show when={hop.is_final}>
          <span class="trace-hop-badge trace-hop-badge--final">answer</span>
        </Show>
        <Show when={hasDivergence()}>
          <span class="trace-hop-badge trace-hop-badge--diverged">diverged</span>
        </Show>
      </div>

      {/* Final hop: show answer records */}
      <Show when={hop.is_final}>
        <div class="trace-answer">
          <For each={hop.server_results.filter((r) => r.outcome.type === 'answer').flatMap((r) => r.answer_records ?? [])}>
            {(rec) => (
              <div class="trace-record">
                <span class="trace-record-type">{rec.record_type}</span>
                <span class="trace-record-rdata">{rec.rdata}</span>
                <span class="trace-record-ttl">{rec.ttl}s</span>
              </div>
            )}
          </For>
          <Show when={hop.server_results.every((r) => !r.answer_records?.length)}>
            <div class="trace-record trace-record--empty">No records returned</div>
          </Show>
        </div>
      </Show>

      {/* Non-final hop: show referral groups */}
      <Show when={!hop.is_final}>
        <Show when={majority()}>
          {(g) => (
            <div class="trace-referral">
              <span class="trace-referral-arrow">→</span>
              <NsList names={g().ns_names} />
            </div>
          )}
        </Show>

        <Show when={hasDivergence()}>
          <div class="trace-divergence">
            <span class="trace-divergence-label">Divergent referrals:</span>
            <div class="trace-divergence-groups">
              <For each={minorities()}>
                {(g) => (
                  <div class="trace-divergence-group">
                    <span class="trace-divergence-servers">{g.servers.join(', ')}</span>
                    <span class="trace-referral-arrow">→</span>
                    <NsList names={g.ns_names} />
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>
      </Show>

      {/* Per-server detail (collapsible) */}
      <div class="trace-hop-footer">
        <span class="trace-hop-stats">
          {hop.servers_queried} server{hop.servers_queried !== 1 ? 's' : ''}
        </span>
        <button
          class="trace-expand-toggle"
          onClick={(e) => { e.stopPropagation(); props.onToggle(); }}
          aria-expanded={props.isExpanded}
        >
          {props.isExpanded ? 'Hide details' : 'Show details'}
        </button>
      </div>

      <Show when={props.isExpanded}>
        <div class="trace-server-list">
          <For each={hop.server_results}>
            {(result) => <ServerRow result={result} ifconfigUrl={props.ifconfigUrl} enrichments={props.enrichments} />}
          </For>
        </div>
      </Show>
    </div>
  );
}

// ---------------------------------------------------------------------------
// TraceView — the full tab content
// ---------------------------------------------------------------------------

interface TraceViewProps {
  hops: TraceHop[];
  doneStats: TraceDoneStats | null;
  isLoading: boolean;
  activeTab: string;
  ifconfigUrl?: string | null;
  enrichments?: Record<string, IpInfo>;
}

export function TraceView(props: TraceViewProps) {
  const [focusedIndex, setFocusedIndex] = createSignal<number | null>(null);
  const [expandedIndices, setExpandedIndices] = createSignal<Set<number>>(new Set());
  let containerRef: HTMLDivElement | undefined;

  function toggleExpanded(index: number) {
    setExpandedIndices((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  // Reset state when hops are cleared (new trace)
  createEffect(() => {
    if (props.hops.length === 0) {
      setFocusedIndex(null);
      setExpandedIndices(new Set<number>());
    }
  });

  // -------------------------------------------------------------------------
  // Keyboard navigation (j / k / Enter / Escape)
  // -------------------------------------------------------------------------

  function handleKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (props.activeTab !== 'trace') return;

    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');
    if (isEditing) return;

    const hopCount = props.hops.length;
    if (hopCount === 0) return;

    if (e.key === 'j' || e.key === 'k') {
      e.preventDefault();
      const current = focusedIndex();
      let next: number;
      if (e.key === 'j') {
        next = current === null ? 0 : Math.min(current + 1, hopCount - 1);
      } else {
        if (current === null) return;
        next = Math.max(current - 1, 0);
      }
      setFocusedIndex(next);
      requestAnimationFrame(() => {
        containerRef
          ?.querySelector(`[data-row-key="hop-${next}"]`)
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
      return;
    }

    if (e.key === 'Enter') {
      const idx = focusedIndex();
      if (idx !== null) {
        e.preventDefault();
        toggleExpanded(idx);
      }
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
    <div class="trace-view" ref={containerRef}>
      <Show when={props.doneStats}>
        {(stats) => (
          <div class="trace-summary">
            <span class="trace-summary-item">{stats().hops} hop{stats().hops !== 1 ? 's' : ''}</span>
            <span class="trace-summary-sep">/</span>
            <span class="trace-summary-item">{stats().duration_ms}ms</span>
          </div>
        )}
      </Show>

      <div class="trace-timeline">
        <For each={props.hops}>
          {(hop, i) => (
            <>
              <Show when={i() > 0}>
                <div class="trace-connector" />
              </Show>
              <HopCard
                hop={hop}
                index={i()}
                isFocused={focusedIndex() === i()}
                isExpanded={expandedIndices().has(i())}
                onToggle={() => toggleExpanded(i())}
                onClick={() => setFocusedIndex(i())}
                ifconfigUrl={props.ifconfigUrl}
                enrichments={props.enrichments}
              />
            </>
          )}
        </For>

        <Show when={props.isLoading && props.hops.length === 0}>
          <div class="trace-pending" role="status" aria-label="Loading trace">
            <span class="trace-pending-dot" />
            <span class="trace-pending-dot" aria-hidden="true" />
            <span class="trace-pending-dot" aria-hidden="true" />
          </div>
        </Show>
      </div>
    </div>
  );
}
