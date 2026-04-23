import { createSignal, onMount, onCleanup, Show, For, createEffect } from 'solid-js';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import type { SuiteNavEcosystem } from '@netray-info/common-frontend/components/SuiteNav';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import Modal from '@netray-info/common-frontend/components/Modal';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import { createTheme } from '@netray-info/common-frontend/theme';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import { copyToClipboard, downloadFile } from '@netray-info/common-frontend/utils';
import { addToHistory, getHistory } from './lib/history';
import { fetchMeta, streamInspect } from './lib/api';
import type { MetaResponse } from './lib/api';
import type {
  Category,
  CheckResult,
  IpEnrichment,
  SseEvent,
  SubCheck,
  SummaryEvent,
  Verdict,
} from './lib/types';
import {
  CATEGORY_LABELS, CATEGORY_ORDER, VERDICT_ORDER,
  GROUP_ORDER, GROUP_LABELS, GROUP_CATEGORIES,
  CATEGORY_EXPLANATIONS,
  subCheckLabel, subCheckExplanation,
} from './lib/types';
import { parseQuery, formatQuery, MAX_SELECTORS } from './lib/parse';

const EXAMPLE_DOMAINS = ['netray.info', 'gmail.com', 'example.com'];

type Ecosystem = NonNullable<MetaResponse['ecosystem']>;

interface EcoLink {
  href: string;
  label: string;
}

const DKIM_SELECTOR_RE = /selector '([A-Za-z0-9-]{1,63})'/;
const DANE_HOST_RE = /^([A-Za-z0-9.-]+):\s/;

function firstMatch(subChecks: SubCheck[], re: RegExp): string | null {
  for (const sc of subChecks) {
    const m = sc.detail.match(re);
    if (m) return m[1];
  }
  return null;
}

function categoryHeaderLink(
  cat: Category,
  domain: string,
  eco: Ecosystem | undefined,
  result?: CheckResult,
): EcoLink | null {
  if (!domain || !eco) return null;

  const trim = (u: string) => u.replace(/\/+$/, '');
  const dnsBase = eco.dns_base_url ? trim(eco.dns_base_url) : null;
  const tlsBase = eco.tls_base_url ? trim(eco.tls_base_url) : null;

  const dnsQ = (name: string, type: string): EcoLink | null =>
    dnsBase
      ? { href: `${dnsBase}/?q=${encodeURIComponent(`${name} ${type}`)}&ref=beacon`, label: `DNS ↗` }
      : null;
  const tlsH = (host: string): EcoLink | null =>
    tlsBase
      ? { href: `${tlsBase}/?h=${encodeURIComponent(host)}&ref=beacon`, label: `TLS ↗` }
      : null;

  switch (cat) {
    case 'mx':      return dnsQ(domain, 'MX');
    case 'spf':     return dnsQ(domain, 'TXT');
    case 'dmarc':   return dnsQ(`_dmarc.${domain}`, 'TXT');
    case 'dnssec':  return dnsQ(domain, 'DNSKEY');
    case 'tls_rpt': return dnsQ(`_smtp._tls.${domain}`, 'TXT');
    case 'bimi':    return dnsQ(`default._bimi.${domain}`, 'TXT');
    case 'mta_sts': return tlsH(`mta-sts.${domain}`);
    case 'dkim': {
      const sel = result ? firstMatch(result.sub_checks, DKIM_SELECTOR_RE) : null;
      return sel ? dnsQ(`${sel}._domainkey.${domain}`, 'TXT') : null;
    }
    case 'dane': {
      const host = result ? firstMatch(result.sub_checks, DANE_HOST_RE) : null;
      return host ? dnsQ(`_25._tcp.${host}`, 'TLSA') : null;
    }
    default:        return null;
  }
}

export default function App() {
  const theme = createTheme('beacon_theme', 'system');
  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);

  // Input state
  const [query, setQuery] = createSignal('');
  const [inspectedDomain, setInspectedDomain] = createSignal('');
  const [inspectedSelectors, setInspectedSelectors] = createSignal<string[]>([]);
  // Last successfully inspected query (raw), used by the `r` shortcut and Retry button.
  const [lastInspectedQuery, setLastInspectedQuery] = createSignal<string>('');
  let inputEl: HTMLInputElement | undefined;
  const [showHistory, setShowHistory] = createSignal(false);
  const [historyIndex, setHistoryIndex] = createSignal<number>(-1);

  // Results state
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [disconnected, setDisconnected] = createSignal(false);
  const [categories, setCategories] = createSignal<Map<Category, CheckResult>>(new Map());
  const [summary, setSummary] = createSignal<SummaryEvent | null>(null);
  const [expandAll, setExpandAll] = createSignal(false);
  const [showExplanations, setShowExplanations] = createSignal(false);
  const [openSections, setOpenSections] = createSignal<Set<Category>>(new Set());
  const [completedCount, setCompletedCount] = createSignal(0);
  const [clientDurationMs, setClientDurationMs] = createSignal<number | undefined>(undefined);

  let abortRef: AbortController | null = null;
  let inspectStartedAt = 0;

  onMount(() => {
    fetchMeta().then((m) => {
      if (m) setMeta(m);
    });

    // Check URL params: ?q= is the canonical form; ?domain= is a legacy alias
    const params = new URLSearchParams(window.location.search);
    const rawQuery = params.get('q') ?? params.get('domain');
    if (rawQuery) {
      setQuery(rawQuery);
      runQuery(rawQuery);
    }

    function clearCardActive() {
      document.querySelector('[data-card-active]')?.removeAttribute('data-card-active');
    }

    function navigateCards(e: KeyboardEvent) {
      const cards = Array.from(document.querySelectorAll<HTMLElement>('[data-card]'));
      if (cards.length === 0) return;
      e.preventDefault();
      const cur = document.querySelector<HTMLElement>('[data-card-active]');
      let idx = cur ? cards.indexOf(cur) : -1;
      if (idx === -1) {
        idx = e.key === 'j' ? 0 : cards.length - 1;
      } else {
        cur!.removeAttribute('data-card-active');
        idx += e.key === 'j' ? 1 : -1;
      }
      idx = Math.max(0, Math.min(idx, cards.length - 1));
      cards[idx].setAttribute('data-card-active', '');
      cards[idx].scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }

    function expandActiveCard(e: KeyboardEvent) {
      const active = document.querySelector<HTMLElement>('[data-card-active]');
      if (active) {
        e.preventDefault();
        active.querySelector<HTMLElement>('.section-card__header')?.click();
      }
    }

    document.addEventListener('mousedown', clearCardActive);

    const cleanupShortcuts = createKeyboardShortcuts({
      '?':      (e) => { e.preventDefault(); setShowHelp(v => !v); },
      'e':      (e) => { e.preventDefault(); setShowExplanations(v => !v); },
      '/':      (e) => { e.preventDefault(); inputEl?.focus(); },
      'r':      (e) => {
        // Re-run the last successfully inspected query, NOT the current
        // edit-in-progress input value. See F11.
        const last = lastInspectedQuery();
        if (last.trim() && !loading()) { e.preventDefault(); runQuery(last); }
      },
      'j':      navigateCards,
      'k':      navigateCards,
      'Enter':  expandActiveCard,
      'Escape': () => { setShowHelp(false); setShowHistory(false); },
    });

    onCleanup(() => {
      cleanupShortcuts();
      document.removeEventListener('mousedown', clearCardActive);
    });
  });

  onCleanup(() => { abortRef?.abort(); });

  createEffect(() => {
    const expanded = expandAll();
    if (expanded) {
      setOpenSections(new Set<Category>(CATEGORY_ORDER));
    } else {
      setOpenSections(new Set<Category>());
    }
  });

  function handleInspect(d: string, sels: string[]) {
    if (!d.trim()) return;
    abortRef?.abort();

    setError(null);
    setDisconnected(false);
    setCategories(new Map());
    setSummary(null);
    setCompletedCount(0);
    setLoading(true);
    setClientDurationMs(undefined);
    setInspectedDomain(d);
    setInspectedSelectors(sels);
    setLastInspectedQuery(formatQuery(d, sels));
    inspectStartedAt = Date.now();

    const newUrl = new URL(window.location.href);
    newUrl.searchParams.delete('domain');
    newUrl.searchParams.set('q', formatQuery(d, sels));
    window.history.replaceState(null, '', newUrl.toString());

    abortRef = streamInspect(d, sels, {
      onEvent: (event: SseEvent) => {
        if (event.type === 'category') {
          setCategories((prev) => {
            const next = new Map(prev);
            next.set(event.category, event);
            return next;
          });
          setCompletedCount((c) => c + 1);
        } else if (event.type === 'summary') {
          setSummary(event);
          setClientDurationMs(Date.now() - inspectStartedAt);
        }
      },
      onError: (msg, kind) => {
        setError(msg);
        // Preserve partial results and surface a Retry banner when the stream
        // disconnects before the Summary event arrives.
        if (kind === 'disconnect' && summary() === null) {
          setDisconnected(true);
        }
      },
      onDone: () => { setLoading(false); addToHistory(d); },
    });
  }

  function runQuery(raw: string) {
    const parsed = parseQuery(raw);
    if ('error' in parsed) {
      setError(parsed.error);
      return;
    }
    setError(null);
    handleInspect(parsed.domain, parsed.selectors);
  }

  function handleSubmit(e: Event) {
    e.preventDefault();
    runQuery(query());
  }

  function toggleSection(cat: Category) {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) { next.delete(cat); } else { next.add(cat); }
      return next;
    });
  }

  const hasResults = () => categories().size > 0 || loading();
  const isIdle = () => !hasResults() && !loading() && !error();

  return (
    <div class="app">
      <SuiteNav current="email" meta={meta()?.ecosystem as SuiteNavEcosystem} />
      <a class="skip-link" href="#main">Skip to content</a>
        <header class="header">
          <h1 class="logo">beacon</h1>
          <span class="tagline">email security, analyzed</span>
          <div class="header-actions">
            <ThemeToggle theme={theme} class="header-btn" />
            <button
              class="header-btn"
              type="button"
              aria-label="Open help"
              onClick={() => setShowHelp(true)}
              title="Help (?)"
            >?</button>
          </div>
        </header>

        <main class="main" id="main">
          <form class="inspect-form" onSubmit={handleSubmit}>
            <div class="domain-input-row">
              <div class="domain-input-wrapper">
                <input
                  ref={inputEl}
                  type="text"
                  class="domain-input"
                  placeholder="example.com [dkim-selector ...]"
                  value={query()}
                  onInput={(e) => {
                    setQuery(e.currentTarget.value);
                    setHistoryIndex(-1);
                  }}
                  onFocus={() => { setShowHistory(true); setHistoryIndex(-1); }}
                  onBlur={() => setTimeout(() => setShowHistory(false), 200)}
                  onKeyDown={(e) => {
                    const items = getHistory();
                    const open = showHistory() && items.length > 0;
                    if (e.key === 'Escape') {
                      if (showHistory()) { e.preventDefault(); setShowHistory(false); setHistoryIndex(-1); }
                      return;
                    }
                    if (!open) return;
                    if (e.key === 'ArrowDown') {
                      e.preventDefault();
                      setHistoryIndex((i) => (i + 1) % items.length);
                    } else if (e.key === 'ArrowUp') {
                      e.preventDefault();
                      setHistoryIndex((i) => (i <= 0 ? items.length - 1 : i - 1));
                    } else if (e.key === 'Home') {
                      e.preventDefault();
                      setHistoryIndex(0);
                    } else if (e.key === 'End') {
                      e.preventDefault();
                      setHistoryIndex(items.length - 1);
                    } else if (e.key === 'Enter') {
                      const idx = historyIndex();
                      if (idx >= 0 && idx < items.length) {
                        e.preventDefault();
                        const item = items[idx].query;
                        setQuery(item);
                        setShowHistory(false);
                        setHistoryIndex(-1);
                        runQuery(item);
                      }
                    }
                  }}
                  role="combobox"
                  aria-label="Domain and optional DKIM selectors, space-separated"
                  aria-expanded={showHistory() && getHistory().length > 0}
                  aria-autocomplete="list"
                  aria-controls="history-listbox"
                  aria-activedescendant={
                    historyIndex() >= 0 ? `history-option-${historyIndex()}` : undefined
                  }
                  autocomplete="off"
                  spellcheck={false}
                />
                <Show when={query()}>
                  <button
                    type="button"
                    class="domain-input__clear"
                    aria-label="Clear"
                    onClick={() => setQuery('')}
                  >×</button>
                </Show>
                <Show when={showHistory() && getHistory().length > 0}>
                  <ul
                    class="history-dropdown"
                    id="history-listbox"
                    role="listbox"
                  >
                    <For each={getHistory()}>
                      {(entry, i) => {
                        const isActive = () => historyIndex() === i();
                        return (
                          <li
                            id={`history-option-${i()}`}
                            role="option"
                            aria-selected={isActive()}
                            classList={{ 'history-dropdown__item--active': isActive() }}
                            onMouseDown={() => {
                              setQuery(entry.query);
                              setShowHistory(false);
                              setHistoryIndex(-1);
                              runQuery(entry.query);
                            }}
                          >
                            {entry.query}
                          </li>
                        );
                      }}
                    </For>
                  </ul>
                </Show>
              </div>
              <ShareButton
                domain={inspectedDomain}
                selectors={inspectedSelectors}
              />
              <button type="submit" class="btn-primary" disabled={loading() || !query().trim()}>
                {loading() ? 'Inspecting…' : 'Inspect'}
              </button>
            </div>
          </form>

          <Show when={error() && !disconnected()}>
            <div class="error-banner" role="alert">{error()}</div>
          </Show>

          <Show when={disconnected()}>
            <div class="error-banner error-banner--retry" role="alert">
              <span class="error-banner__message">
                {error() ?? 'Connection lost before inspection finished'}
              </span>
              <button
                type="button"
                class="btn-secondary error-banner__retry"
                onClick={() => {
                  const last = lastInspectedQuery();
                  if (last.trim()) runQuery(last);
                }}
              >Retry</button>
            </div>
          </Show>

          <Show when={isIdle()}>
            <div class="empty-state">
              <div class="empty-state__title">Inspect any domain</div>
              <p>
                Check MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and
                DNSBL — DNS-only, no mail servers contacted.
              </p>
              <div class="example-chips">
                <For each={EXAMPLE_DOMAINS}>
                  {(d) => (
                    <button
                      type="button"
                      class="example-chip"
                      onClick={() => { setQuery(d); runQuery(d); }}
                    >
                      {d}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          <Show when={hasResults()}>
            <Show when={loading() && categories().size === 0}>
              <div class="loading" role="status" aria-live="polite">
                <div class="spinner" />
                <span>Inspecting…</span>
              </div>
            </Show>

            <Show when={categories().size > 0}>
              <Show when={summary() !== null}>
                <OverviewCard
                  summary={summary()!}
                  mxResult={categories().get('mx')}
                  ipBaseUrl={meta()?.ecosystem?.ip_base_url}
                  domain={inspectedDomain()}
                  categories={categories()}
                  clientDurationMs={clientDurationMs()}
                />
              </Show>

              <NoMailNotice mxResult={categories().get('mx')} />

              <div class="section-controls">
                <div class="section-controls__left">
                  <Show when={loading()}>
                    <span class="progress-text">{completedCount()} of 12 checks complete</span>
                  </Show>
                  <button
                    type="button"
                    class="filter-toggle"
                    classList={{ 'filter-toggle--active': showExplanations() }}
                    onClick={() => setShowExplanations(v => !v)}
                    aria-pressed={showExplanations()}
                    title="Toggle explanations (e)"
                  >explain</button>
                </div>
                <div class="section-controls__right">
                  <button
                    type="button"
                    class="filter-toggle"
                    onClick={() => setExpandAll(!expandAll())}
                    aria-pressed={expandAll()}
                  >
                    {expandAll() ? 'Collapse all' : 'Expand all'}
                  </button>
                </div>
              </div>
            </Show>

            <div class="category-list" role="list" aria-live="polite" aria-busy={loading()}>
              <For each={GROUP_ORDER}>
                {(group) => (
                  <>
                    <GroupHeading group={group} categories={categories()} />
                    <For each={GROUP_CATEGORIES[group]}>
                      {(cat) => {
                        const result = () => categories().get(cat);
                        return (
                          <Show
                            when={result()}
                            fallback={
                              <Show when={loading()}>
                                <div
                                  class="section-card card--pending"
                                  aria-busy="true"
                                  aria-label={`Loading ${CATEGORY_LABELS[cat]}…`}
                                >
                                  <div class="section-card__header">
                                    <span class="section-card__status section-card__status--skip" />
                                    <span class="section-card__title">{CATEGORY_LABELS[cat]}</span>
                                    <span class="section-card__spacer" />
                                  </div>
                                </div>
                              </Show>
                            }
                          >
                            {(r) => (
                              <CategorySection
                                result={r()}
                                open={openSections().has(cat)}
                                onToggle={() => toggleSection(cat)}
                                showExplanations={showExplanations()}
                                domain={inspectedDomain()}
                                ecosystem={meta()?.ecosystem}
                              />
                            )}
                          </Show>
                        );
                      }}
                    </For>
                  </>
                )}
              </For>
            </div>

          </Show>
        </main>

        <SiteFooter
          aboutText={
            <>
              <em>beacon</em> performs DNS-only email security analysis. Built in{' '}
              <a href="https://www.rust-lang.org" target="_blank" rel="noopener noreferrer">Rust</a> with{' '}
              <a href="https://github.com/tokio-rs/axum" target="_blank" rel="noopener noreferrer">Axum</a> and{' '}
              <a href="https://www.solidjs.com" target="_blank" rel="noopener noreferrer">SolidJS</a>.
              Open to use — rate limiting applies. Part of the{' '}
              <a href="https://netray.info" target="_blank" rel="noopener noreferrer"><strong>netray.info</strong></a> suite.
            </>
          }
          links={[
            { href: '/docs', label: 'API Docs' },
            { href: 'https://lukas.pustina.de', label: 'Author', external: true },
          ]}
          version={meta()?.version}
        />

        <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
          <div class="help-section">
            <div class="help-section__title">About</div>
            <p class="help-desc">
              beacon checks email security posture via DNS only — no mail servers contacted.
              Inspects MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and
              DNSBL.{' '}
              <a href="https://netray.info/guide/email-auth.html" target="_blank" rel="noopener noreferrer">
                Email auth guide ↗
              </a>
            </p>
          </div>

          <div class="help-section">
            <div class="help-section__title">Query syntax</div>
            <code class="help-syntax">domain [dkim-selector ...]</code>
            <p class="help-desc">
              The first token is the domain (must contain a dot). Any additional whitespace-separated
              tokens are treated as DKIM selectors to probe. Up to {MAX_SELECTORS} selectors.
            </p>
            <p class="help-desc">
              Examples:{' '}
              <code class="help-syntax">example.com</code>,{' '}
              <code class="help-syntax">example.com google</code>,{' '}
              <code class="help-syntax">example.com google s1 s2</code>
            </p>
            <p class="help-desc">
              beacon auto-probes Google, Outlook, SES, Proofpoint, Mimecast and{' '}
              <code class="help-syntax">default</code>. Add selectors only for other providers —
              use the value after <code class="help-syntax">s=</code> in a{' '}
              <code class="help-syntax">DKIM-Signature</code> header.
            </p>
          </div>

          <div class="help-section">
            <div class="help-section__title">Keyboard shortcuts</div>
            <table class="shortcuts-table">
              <thead>
                <tr><th>Key</th><th>Action</th></tr>
              </thead>
              <tbody>
                <tr><td class="shortcut-key">/</td><td>Focus query input</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Submit query (when input focused)</td></tr>
                <tr><td class="shortcut-key">r</td><td>Re-run last inspected query (ignores unsaved input)</td></tr>
                <tr><td class="shortcut-key">e</td><td>Toggle explanations</td></tr>
                <tr><td class="shortcut-key">j / k</td><td>Navigate result categories</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Expand / collapse active category</td></tr>
                <tr><td class="shortcut-key">Escape</td><td>Close help / dismiss history</td></tr>
                <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
              </tbody>
            </table>
          </div>
        </Modal>
    </div>
  );
}

function NoMailNotice(props: { mxResult?: CheckResult }) {
  const reason = (): string | null => {
    if (!props.mxResult) return null;
    const names = props.mxResult.sub_checks.map((sc) => sc.name);
    if (names.includes('null_mx')) return 'This domain explicitly declares it does not accept email (null MX).';
    if (names.includes('no_mx')) return 'This domain does not appear to receive email — no MX records found.';
    return null;
  };

  return (
    <Show when={reason()}>
      {(msg) => <div class="notice-banner" role="status">{msg()}</div>}
    </Show>
  );
}

function GroupHeading(props: {
  group: import('./lib/types').Group;
  categories: Map<Category, CheckResult>;
}) {
  const groupCats = () => GROUP_CATEGORIES[props.group];
  const completedResults = () =>
    groupCats().map((c) => props.categories.get(c)).filter(Boolean) as CheckResult[];
  const worstVerdict = (): Verdict | undefined => {
    const results = completedResults();
    if (results.length === 0) return undefined;
    return results.reduce<Verdict>(
      (worst, r) => (VERDICT_ORDER[r.verdict] > VERDICT_ORDER[worst] ? r.verdict : worst),
      'skip',
    );
  };

  return (
    <div class="group-heading">
      <span class="group-heading__title">{GROUP_LABELS[props.group]}</span>
      <Show when={worstVerdict()}>
        {(v) => <span class={`badge badge--${v()} badge--small`}>{v()}</span>}
      </Show>
      <span class="group-heading__divider" />
    </div>
  );
}

function durationClass(ms: number): string {
  if (ms < 500) return 'overview__value overview__value--fast';
  if (ms < 2000) return 'overview__value overview__value--ok';
  return 'overview__value overview__value--slow';
}

function OverviewCard(props: {
  summary: SummaryEvent;
  mxResult?: CheckResult;
  ipBaseUrl?: string;
  domain: string;
  categories: Map<import('./lib/types').Category, CheckResult>;
  clientDurationMs?: number;
}) {
  const overallVerdict = (): Verdict => {
    const vs = Object.values(props.summary.verdicts) as Verdict[];
    return vs.reduce<Verdict>(
      (worst, v) => (VERDICT_ORDER[v] > VERDICT_ORDER[worst] ? v : worst),
      'skip',
    );
  };

  const counts = () => {
    const c: Partial<Record<Verdict, number>> = {};
    for (const v of Object.values(props.summary.verdicts) as Verdict[]) {
      c[v] = (c[v] ?? 0) + 1;
    }
    return c;
  };

  const firstEnrichment = () => props.mxResult?.enrichment?.[0];

  // F8: grade is primary, verdict is subheading. Backend sends Grade as
  // uppercase letter (A-F) today; G3 will introduce "skipped" (lowercase).
  const gradeLabel = (): string => {
    const g = props.summary.grade as string;
    return g === 'skipped' ? 'Skipped' : g;
  };
  const gradeModifier = (): string => {
    const g = props.summary.grade as string;
    return g === 'skipped' ? 'skipped' : g.toLowerCase();
  };

  return (
    <div class="overview">
      <div class="overview__row overview__row--grade">
        <div class="overview__item overview__item--grade">
          <span class="overview__label">Grade</span>
          <span
            class={`overview__grade overview__grade--${gradeModifier()}`}
            aria-label={`Grade ${gradeLabel()}`}
          >{gradeLabel()}</span>
        </div>
        <div class="overview__item overview__item--verdict">
          <span class="overview__label">Verdict</span>
          <span class={`badge badge--${overallVerdict()}`}>{overallVerdict()}</span>
        </div>
        <Show when={(counts().fail ?? 0) > 0}>
          <span class="badge badge--fail">{counts().fail} failed</span>
        </Show>
        <Show when={(counts().warn ?? 0) > 0}>
          <span class="badge badge--warn">{counts().warn} warnings</span>
        </Show>
        <Show when={(counts().info ?? 0) > 0}>
          <span class="badge badge--info">{counts().info} info</span>
        </Show>
        <Show when={(counts().pass ?? 0) > 0}>
          <span class="badge badge--pass">{counts().pass} passed</span>
        </Show>
        {(() => {
          const ms = props.summary.duration_ms ?? props.clientDurationMs;
          return ms !== undefined ? (
            <div class="overview__item">
              <span class="overview__label">Duration</span>
              <span class={durationClass(ms)}>{ms}ms</span>
            </div>
          ) : null;
        })()}
        <ExportButtons domain={props.domain} summary={props.summary} categories={props.categories} />
      </div>

      <Show when={firstEnrichment()}>
        {(e) => (
          <div class="overview__row overview__row--enrichment">
            <div class="overview__item">
              <span class="overview__label">MX</span>
              <span class="overview__value">{e().ip}</span>
            </div>
            <Show when={e().org}>
              <div class="overview__item">
                <span class="overview__label">Hosted by</span>
                <span class="overview__value">
                  {e().org}
                  <Show when={e().ip_type}>
                    {' '}
                    <span class="overview__value--qualifier">({e().ip_type})</span>
                  </Show>
                </span>
              </div>
            </Show>
            <a
              href={`${props.ipBaseUrl ?? 'https://ip.netray.info'}/${encodeURIComponent(e().ip)}`}
              class="overview__ip-link"
              target="_blank"
              rel="noopener noreferrer"
            >IP ↗</a>
          </div>
        )}
      </Show>
    </div>
  );
}

function CategorySection(props: {
  result: CheckResult;
  open: boolean;
  onToggle: () => void;
  showExplanations: boolean;
  domain: string;
  ecosystem?: Ecosystem;
}) {
  const headerLink = () =>
    categoryHeaderLink(props.result.category, props.domain, props.ecosystem, props.result);
  const explanation = () => CATEGORY_EXPLANATIONS[props.result.category];
  const counts = () => {
    const c: Partial<Record<Verdict, number>> = {};
    for (const sc of props.result.sub_checks) {
      c[sc.verdict] = (c[sc.verdict] ?? 0) + 1;
    }
    return c;
  };

  const headerId = `card-header-${props.result.category}`;
  const cardId = `card-body-${props.result.category}`;

  return (
    <div class="section-card" data-card role="listitem">
      <button
        id={headerId}
        type="button"
        class="section-card__header"
        onClick={props.onToggle}
        aria-expanded={props.open}
        aria-controls={cardId}
      >
        <span class={`section-card__status section-card__status--${props.result.verdict}`} />
        <span class="section-card__title">{CATEGORY_LABELS[props.result.category]}</span>
        <span class="section-card__badges">
          <Show when={(counts().fail ?? 0) > 0}>
            <span class="badge badge--fail">{counts().fail} failed</span>
          </Show>
          <Show when={(counts().warn ?? 0) > 0}>
            <span class="badge badge--warn">{counts().warn} warnings</span>
          </Show>
          <Show when={(counts().info ?? 0) > 0}>
            <span class="badge badge--info">{counts().info} info</span>
          </Show>
          <Show when={(counts().pass ?? 0) > 0}>
            <span class="badge badge--pass">{counts().pass} passed</span>
          </Show>
        </span>
        <Show when={!props.open && props.result.detail}>
          <span class="section-card__summary">{props.result.detail}</span>
        </Show>
        <span class="section-card__spacer" />
        <Show when={headerLink()}>
          {(l) => (
            <a
              class="section-card__link"
              href={l().href}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
            >{l().label}</a>
          )}
        </Show>
        <span class={`section-card__chevron${props.open ? ' section-card__chevron--open' : ''}`}>
          &#9660;
        </span>
      </button>

      <Show when={props.open}>
        <div
          id={cardId}
          class="section-card__body"
          role="region"
          aria-labelledby={headerId}
        >
          <Show when={props.showExplanations && explanation()}>
            {(e) => (
              <div class="explain-card">
                {e().summary}
                <Show when={e().guideUrl}>
                  {' '}<a href={e().guideUrl} target="_blank" rel="noopener noreferrer" class="explain-card__guide-link">Learn more ↗</a>
                </Show>
              </div>
            )}
          </Show>
          <Show when={props.result.detail}>
            <div class="section-card__detail">{props.result.detail}</div>
          </Show>
          <Show
            when={props.result.sub_checks.length > 0}
            fallback={<p class="section-empty">No sub-checks</p>}
          >
            <ul class="check-list">
              <For each={props.result.sub_checks}>
                {(sc: SubCheck) => (
                  <li
                    class={`check-list__item${
                      sc.verdict === 'fail' ? ' check-row--fail' :
                      sc.verdict === 'warn' ? ' check-row--warn' : ''
                    }${props.showExplanations ? ' check-list__item--explainable' : ''}`}
                  >
                    <span class={`badge badge--${sc.verdict}`}>{sc.verdict}</span>
                    <span class="check-list__name">{subCheckLabel(sc.name)}</span>
                    <span class="check-list__message">{sc.detail}</span>
                    <Show when={props.showExplanations && subCheckExplanation(sc.name)}>
                      <span class="check-explain">{subCheckExplanation(sc.name)}</span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>

          <Show when={props.result.enrichment && props.result.enrichment.length > 0}>
            <div class="enrichment-list">
              <For each={props.result.enrichment!}>
                {(e: IpEnrichment) => (
                  <span class="enrichment-item">
                    <span class="enrichment-item__ip">{e.ip}</span>
                    <Show when={e.asn}><span class="badge badge--skip">AS{e.asn}</span></Show>
                    <Show when={e.org}><span class="badge badge--skip">{e.org}</span></Show>
                    <Show when={e.ip_type}><span class="badge badge--skip">{e.ip_type}</span></Show>
                  </span>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function ShareButton(props: {
  domain: () => string;
  selectors: () => string[];
}) {
  const [status, setStatus] = createSignal<'idle' | 'copied'>('idle');

  const buildShareUrl = (): string => {
    const d = props.domain();
    if (!d) return window.location.href;
    const base = `${window.location.origin}${window.location.pathname}`;
    const params = new URLSearchParams();
    params.set('q', formatQuery(d, props.selectors()));
    return `${base}?${params.toString()}`;
  };

  const disabled = () => !props.domain();

  const handleClick = async () => {
    if (disabled()) return;
    await copyToClipboard(buildShareUrl());
    setStatus('copied');
    setTimeout(() => setStatus('idle'), 2000);
  };

  return (
    <span class="share-btn-wrapper">
      <button
        type="button"
        class="share-btn"
        aria-label="Copy shareable link"
        title={status() === 'copied' ? 'Copied!' : 'Copy shareable link'}
        disabled={disabled()}
        onClick={handleClick}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
          <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
        </svg>
      </button>
      <Show when={status() === 'copied'}>
        <span class="share-btn__toast" role="status" aria-live="polite">Copied!</span>
      </Show>
    </span>
  );
}

function ExportButtons(props: {
  domain: string;
  summary: SummaryEvent;
  categories: Map<Category, CheckResult>;
}) {
  const [copyStatus, setCopyStatus] = createSignal<'idle' | 'success' | 'error'>('idle');

  const downloadJson = () => {
    const data = {
      domain: props.domain,
      duration_ms: props.summary.duration_ms,
      verdicts: props.summary.verdicts,
      categories: Object.fromEntries(
        Array.from(props.categories.entries()).map(([k, v]) => [k, v]),
      ),
    };
    downloadFile(JSON.stringify(data, null, 2), `beacon-${props.domain}.json`, 'application/json');
  };

  const copyMarkdown = async () => {
    const lines: string[] = [
      `# Email Security: ${props.domain}`,
      '',
      `**Verdict**: ${Object.values(props.summary.verdicts).reduce((w, v) =>
        VERDICT_ORDER[v as Verdict] > VERDICT_ORDER[w as Verdict] ? v : w,
      )}`,
    ];
    if (props.summary.duration_ms !== undefined) {
      lines.push(`**Duration**: ${props.summary.duration_ms}ms`);
    }
    for (const group of GROUP_ORDER) {
      lines.push(`## ${GROUP_LABELS[group]}`, '');
      for (const cat of GROUP_CATEGORIES[group]) {
        const result = props.categories.get(cat);
        if (!result) continue;
        lines.push(`### ${CATEGORY_LABELS[cat]} — ${result.verdict}`);
        if (result.detail) lines.push(result.detail, '');
        for (const sc of result.sub_checks) {
          lines.push(`- **${sc.verdict.toUpperCase()}** ${subCheckLabel(sc.name)}${sc.detail ? ` — ${sc.detail}` : ''}`);
        }
        lines.push('');
      }
    }
    lines.push(`_Inspected via [beacon](https://email.netray.info)_`);

    const ok = await copyToClipboard(lines.join('\n'));
    setCopyStatus(ok ? 'success' : 'error');
    setTimeout(() => setCopyStatus('idle'), 2000);
  };

  return (
    <div class="export-buttons">
      <button type="button" class="export-buttons__btn" onClick={copyMarkdown} aria-label="Copy as Markdown">
        {copyStatus() === 'success' ? 'copied!' : copyStatus() === 'error' ? 'failed' : 'copy MD'}
      </button>
      <button type="button" class="export-buttons__btn" onClick={downloadJson} aria-label="Download as JSON">
        JSON
      </button>
    </div>
  );
}
