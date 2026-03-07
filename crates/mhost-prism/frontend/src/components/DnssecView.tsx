import { createSignal, createEffect, onMount, onCleanup, For, Show } from 'solid-js';

// ---------------------------------------------------------------------------
// Types (mirror the Rust backend structs in dns_dnssec.rs)
// ---------------------------------------------------------------------------

export interface DnsRecord {
  name: string;
  ttl: number;
  record_type: string;
  rdata: string;
}

export interface ChainFinding {
  severity: 'ok' | 'warning' | 'failed';
  message: string;
}

export interface ChainLevel {
  level: number;
  zone: string;
  servers_queried: number;
  dnskey_records: DnsRecord[];
  ds_records: DnsRecord[];
  rrsig_records: DnsRecord[];
  findings: ChainFinding[];
  latency_ms: number;
  is_final: boolean;
}

export interface DnssecDoneStats {
  request_id: string;
  duration_ms: number;
  levels: number;
}

// ---------------------------------------------------------------------------
// Severity helpers
// ---------------------------------------------------------------------------

function severityIcon(severity: string): string {
  switch (severity) {
    case 'ok':      return '\u2713'; // checkmark
    case 'warning': return '\u26A0'; // warning
    case 'failed':  return '\u2717'; // cross
    default:        return '?';
  }
}

function worstSeverity(findings: ChainFinding[]): string {
  if (findings.some((f) => f.severity === 'failed')) return 'failed';
  if (findings.some((f) => f.severity === 'warning')) return 'warning';
  if (findings.some((f) => f.severity === 'ok')) return 'ok';
  return 'ok';
}

// ---------------------------------------------------------------------------
// RecordList — collapsible list of DNS records
// ---------------------------------------------------------------------------

function RecordList(props: { label: string; records: DnsRecord[] }) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <Show when={props.records.length > 0}>
      <div class="dnssec-record-section">
        <button
          class="dnssec-record-toggle"
          onClick={(e) => { e.stopPropagation(); setExpanded((v) => !v); }}
          aria-expanded={expanded()}
        >
          <span class="dnssec-record-toggle-arrow">{expanded() ? '\u25BE' : '\u25B8'}</span>
          <span class="dnssec-record-toggle-label">{props.label}</span>
          <span class="dnssec-record-toggle-count">{props.records.length}</span>
        </button>
        <Show when={expanded()}>
          <div class="dnssec-record-list">
            <For each={props.records}>
              {(rec) => (
                <div class="dnssec-record">
                  <span class="dnssec-record-type">{rec.record_type}</span>
                  <span class="dnssec-record-rdata">{rec.rdata}</span>
                  <span class="dnssec-record-ttl">{rec.ttl}s</span>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}

// ---------------------------------------------------------------------------
// LevelCard — one card per zone level
// ---------------------------------------------------------------------------

interface LevelCardProps {
  level: ChainLevel;
  index: number;
  isFocused: boolean;
  onClick: () => void;
}

function LevelCard(props: LevelCardProps) {
  const lev = props.level;
  const worst = () => worstSeverity(lev.findings);

  const cardClass = () => {
    const parts = ['dnssec-level'];
    parts.push(`dnssec-level--${worst()}`);
    if (lev.is_final) parts.push('dnssec-level--final');
    if (props.isFocused) parts.push('dnssec-level--focused');
    return parts.join(' ');
  };

  const totalRecords = () =>
    lev.dnskey_records.length + lev.ds_records.length + lev.rrsig_records.length;

  return (
    <div
      class={cardClass()}
      data-row-key={`dnssec-${props.index}`}
      tabIndex={0}
      role="button"
      aria-label={`Level ${props.index + 1}: ${lev.zone}`}
      onClick={props.onClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          props.onClick();
        }
      }}
    >
      <div class="dnssec-level-header">
        <span class="dnssec-level-number">{props.index + 1}</span>
        <span class="dnssec-level-zone">{lev.zone}</span>
        <Show when={lev.is_final}>
          <span class="dnssec-level-badge dnssec-level-badge--final">target</span>
        </Show>
        <span class="dnssec-level-latency">{lev.latency_ms.toFixed(0)}ms</span>
      </div>

      {/* Findings */}
      <div class="dnssec-findings">
        <For each={lev.findings}>
          {(finding) => (
            <div class={`dnssec-finding dnssec-finding--${finding.severity}`}>
              <span class="dnssec-finding-icon">{severityIcon(finding.severity)}</span>
              <span class="dnssec-finding-message">{finding.message}</span>
            </div>
          )}
        </For>
      </div>

      {/* Collapsible record details */}
      <Show when={totalRecords() > 0}>
        <div class="dnssec-records">
          <RecordList label="DNSKEY" records={lev.dnskey_records} />
          <RecordList label="DS" records={lev.ds_records} />
          <RecordList label="RRSIG" records={lev.rrsig_records} />
        </div>
      </Show>
    </div>
  );
}

// ---------------------------------------------------------------------------
// DnssecView — the full tab content
// ---------------------------------------------------------------------------

interface DnssecViewProps {
  levels: ChainLevel[];
  doneStats: DnssecDoneStats | null;
  isLoading: boolean;
  activeTab: string;
}

export function DnssecView(props: DnssecViewProps) {
  const [focusedIndex, setFocusedIndex] = createSignal<number | null>(null);
  let containerRef: HTMLDivElement | undefined;

  // Reset state when levels are cleared (new query).
  createEffect(() => {
    if (props.levels.length === 0) {
      setFocusedIndex(null);
    }
  });

  // -------------------------------------------------------------------------
  // Keyboard navigation (j / k)
  // -------------------------------------------------------------------------

  function handleKeyDown(e: KeyboardEvent) {
    if (e.defaultPrevented) return;
    if (props.activeTab !== 'dnssec') return;

    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');
    if (isEditing) return;

    const count = props.levels.length;
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
          ?.querySelector(`[data-row-key="dnssec-${next}"]`)
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
    <div class="dnssec-view" ref={containerRef}>
      <Show when={props.doneStats}>
        {(stats) => (
          <div class="dnssec-summary">
            <span class="dnssec-summary-item">{stats().levels} level{stats().levels !== 1 ? 's' : ''}</span>
            <span class="dnssec-summary-sep">/</span>
            <span class="dnssec-summary-item">{stats().duration_ms}ms</span>
          </div>
        )}
      </Show>
      <div class="dnssec-chain">
        <For each={props.levels}>
          {(level, i) => (
            <>
              <Show when={i() > 0}>
                <div class="dnssec-connector">
                  <div class="dnssec-connector-line" />
                  <span class="dnssec-connector-label">DS {"\u2192"} DNSKEY</span>
                  <div class="dnssec-connector-line" />
                </div>
              </Show>
              <LevelCard
                level={level}
                index={i()}
                isFocused={focusedIndex() === i()}
                onClick={() => setFocusedIndex(i())}
              />
            </>
          )}
        </For>

        <Show when={props.isLoading && props.levels.length === 0}>
          <div class="dnssec-pending" role="status" aria-label="Loading DNSSEC chain">
            <span class="trace-pending-dot" />
            <span class="trace-pending-dot" aria-hidden="true" />
            <span class="trace-pending-dot" aria-hidden="true" />
          </div>
        </Show>
      </div>
    </div>
  );
}
