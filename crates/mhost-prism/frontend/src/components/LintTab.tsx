import { For, Show } from 'solid-js';

// ---------------------------------------------------------------------------
// Types (mirror the Rust backend structs)
// ---------------------------------------------------------------------------

// Serde's external tagging of CheckResult:
//   Ok(String)      → { "Ok": "message" }
//   Warning(String) → { "Warning": "message" }
//   Failed(String)  → { "Failed": "message" }
//   NotFound()      → { "NotFound": [] }
export type CheckResult =
  | { Ok: string }
  | { Warning: string }
  | { Failed: string }
  | { NotFound: [] };

export interface LintCategory {
  category: string;
  results: CheckResult[];
}

export interface CheckDoneStats {
  request_id: string;
  duration_ms: number;
  total_checks: number;
  passed: number;
  warnings: number;
  failed: number;
  not_found: number;
  cache_key?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const CATEGORY_LABELS: Record<string, string> = {
  caa:            'CAA',
  cname_apex:     'CNAME at Apex',
  dnssec:         'DNSSEC',
  https_svcb:     'HTTPS / SVCB',
  mx:             'MX',
  ns:             'NS',
  spf:            'SPF',
  ttl:            'TTL Consistency',
  dmarc:          'DMARC',
  infrastructure: 'Infrastructure',
};

function categoryLabel(category: string): string {
  return CATEGORY_LABELS[category] ?? category.toUpperCase();
}

type CardStatus = 'failed' | 'warning' | 'ok' | 'not_found' | 'pending';

function worstStatus(results: CheckResult[]): CardStatus {
  if (results.length === 0) return 'pending';
  let hasWarning = false;
  let allNotFound = true;
  for (const r of results) {
    if ('Failed' in r) return 'failed';
    if ('Warning' in r) hasWarning = true;
    if (!('NotFound' in r)) allNotFound = false;
  }
  if (hasWarning) return 'warning';
  if (allNotFound) return 'not_found';
  return 'ok';
}

function statusIcon(s: CardStatus): string {
  switch (s) {
    case 'failed':    return '✘';
    case 'warning':   return '⚠';
    case 'ok':        return '✓';
    case 'not_found': return '—';
    case 'pending':   return '·';
  }
}

function resultMessage(r: CheckResult): string {
  if ('Ok' in r)      return r.Ok;
  if ('Warning' in r) return r.Warning;
  if ('Failed' in r)  return r.Failed;
  return 'No data found';
}

function resultKind(r: CheckResult): 'ok' | 'warning' | 'failed' | 'not_found' {
  if ('Ok' in r)      return 'ok';
  if ('Warning' in r) return 'warning';
  if ('Failed' in r)  return 'failed';
  return 'not_found';
}

// ---------------------------------------------------------------------------
// Contextual remediation hints
// ---------------------------------------------------------------------------

function getHint(category: string, results: CheckResult[]): string | null {
  for (const r of results) {
    const kind = resultKind(r);
    const msg = resultMessage(r).toLowerCase();

    if (category === 'spf') {
      if (kind === 'failed' && msg.includes('too many'))
        return 'Reduce SPF includes or flatten with ip4:/ip6: mechanisms.';
      if (kind === 'failed' || kind === 'not_found')
        return 'Add a TXT record: v=spf1 -all (or with your mail providers).';
    }
    if (category === 'dmarc') {
      if (kind === 'failed' || kind === 'not_found')
        return 'Add _dmarc.domain TXT "v=DMARC1; p=reject; rua=mailto:dmarc@example.com".';
      if (kind === 'warning' && msg.includes('p=none'))
        return 'Consider upgrading to p=quarantine or p=reject.';
    }
    if (category === 'dnssec' && kind === 'failed')
      return 'Contact your DNS provider to enable DNSSEC signing.';
    if (category === 'caa' && kind === 'not_found')
      return 'Add CAA records to restrict certificate issuance (e.g., 0 issue "letsencrypt.org").';
    if (category === 'cname_apex' && kind === 'failed')
      return 'CNAME at zone apex violates RFC 1034; use ALIAS/ANAME or A/AAAA records.';
    if (category === 'ns' && kind === 'warning' && msg.includes('single'))
      return 'Add a secondary nameserver for redundancy.';
    if (category === 'https_svcb' && kind === 'not_found')
      return 'Consider adding HTTPS/SVCB records for faster TLS connection setup.';
    if (category === 'ttl' && kind === 'warning')
      return 'Inconsistent TTLs across record types; align TTLs for predictable caching.';
    if (category === 'infrastructure') {
      if (kind === 'failed' && msg.includes('spamhaus'))
        return 'This IP is listed on Spamhaus blocklists — deliverability may be affected.';
      if (kind === 'failed' && msg.includes('command-and-control'))
        return 'This IP is flagged as C2 infrastructure — investigate immediately.';
      if (kind === 'warning' && msg.includes('residential'))
        return 'Residential IPs are unusual for production DNS/mail infrastructure.';
      if (kind === 'warning' && msg.includes('tor'))
        return 'Tor exit nodes are unusual for DNS infrastructure.';
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// LintCard — one card per lint category
// ---------------------------------------------------------------------------

function LintCard(props: { category: LintCategory }) {
  const status = () => worstStatus(props.category.results);
  const label  = () => categoryLabel(props.category.category);
  const hint   = () => getHint(props.category.category, props.category.results);

  return (
    <div class={`lint-card lint-card--${status()}`}>
      <div class="lint-card-header">
        <span class={`lint-status-icon lint-status-icon--${status()}`}>
          {statusIcon(status())}
        </span>
        <span class="lint-card-label">{label()}</span>
      </div>
      <ul class="lint-results-list">
        <For each={props.category.results}>
          {(result) => (
            <li class={`lint-result lint-result--${resultKind(result)}`}>
              {resultMessage(result)}
            </li>
          )}
        </For>
      </ul>
      <Show when={hint()}>
        <div class="lint-hint">{hint()}</div>
      </Show>
    </div>
  );
}

// ---------------------------------------------------------------------------
// LintTab — the full tab content
// ---------------------------------------------------------------------------

interface LintTabProps {
  categories: LintCategory[];
  doneStats: CheckDoneStats | null;
  isLoading: boolean;
}

export function LintTab(props: LintTabProps) {
  // All 9 known categories in display order; missing ones shown as pending
  const ORDERED_CATEGORIES = [
    'dmarc', 'spf', 'dnssec', 'caa', 'ns', 'mx',
    'cname_apex', 'https_svcb', 'ttl', 'infrastructure',
  ];

  const received = () => new Set(props.categories.map((c) => c.category));

  // Build full list: received categories in order, then pending placeholders
  const displayCategories = (): Array<LintCategory | string> => {
    const result: Array<LintCategory | string> = [];
    for (const key of ORDERED_CATEGORIES) {
      const found = props.categories.find((c) => c.category === key);
      if (found) {
        result.push(found);
      } else if (props.isLoading) {
        result.push(key); // pending placeholder
      }
    }
    return result;
  };

  return (
    <div class="lint-tab">
      <Show when={props.doneStats}>
        {(stats) => (
          <div class="lint-summary">
            <span class="lint-summary-item lint-summary-total">
              {stats().total_checks} checks
            </span>
            <Show when={stats().passed > 0}>
              <span class="lint-summary-sep">/</span>
              <span class="lint-summary-item lint-summary-passed">
                ✓ {stats().passed} passed
              </span>
            </Show>
            <Show when={stats().warnings > 0}>
              <span class="lint-summary-sep">/</span>
              <span class="lint-summary-item lint-summary-warning">
                ⚠ {stats().warnings} {stats().warnings === 1 ? 'warning' : 'warnings'}
              </span>
            </Show>
            <Show when={stats().failed > 0}>
              <span class="lint-summary-sep">/</span>
              <span class="lint-summary-item lint-summary-failed">
                ✘ {stats().failed} failed
              </span>
            </Show>
            <span class="lint-summary-sep">/</span>
            <span class="lint-summary-item lint-summary-duration">
              {stats().duration_ms}ms
            </span>
          </div>
        )}
      </Show>

      <div class="lint-grid">
        <For each={displayCategories()}>
          {(item) => {
            if (typeof item === 'string') {
              // Pending placeholder card
              return (
                <div class="lint-card lint-card--pending">
                  <div class="lint-card-header">
                    <span class="lint-status-icon lint-status-icon--pending">·</span>
                    <span class="lint-card-label">{categoryLabel(item)}</span>
                  </div>
                  <div class="lint-card-pending-text">Checking…</div>
                </div>
              );
            }
            return <LintCard category={item} />;
          }}
        </For>
      </div>
    </div>
  );
}
