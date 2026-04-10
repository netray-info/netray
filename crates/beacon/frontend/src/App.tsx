import { createSignal, onMount, onCleanup, Show, For, createEffect } from 'solid-js';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import type { SuiteNavEcosystem } from '@netray-info/common-frontend/components/SuiteNav';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import Modal from '@netray-info/common-frontend/components/Modal';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import CrossLink from '@netray-info/common-frontend/components/CrossLink';
import { createTheme } from '@netray-info/common-frontend/theme';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import { storageGet, storageSet } from '@netray-info/common-frontend/storage';
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
import { CATEGORY_LABELS, CATEGORY_ORDER, VERDICT_ORDER } from './lib/types';

const HISTORY_KEY = 'beacon_history';
const MAX_HISTORY = 20;
const EXAMPLE_DOMAINS = ['netray.info', 'gmail.com', 'example.com'];

export default function App() {
  const theme = createTheme('beacon_theme', 'system');
  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);

  // Input state
  const [domain, setDomain] = createSignal('');
  let inputEl: HTMLInputElement | undefined;
  const [selectors, setSelectors] = createSignal<string[]>([]);
  const [selectorInput, setSelectorInput] = createSignal('');
  const [showHistory, setShowHistory] = createSignal(false);

  // Results state
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [categories, setCategories] = createSignal<Map<Category, CheckResult>>(new Map());
  const [summary, setSummary] = createSignal<SummaryEvent | null>(null);
  const [expandAll, setExpandAll] = createSignal(false);
  const [openSections, setOpenSections] = createSignal<Set<Category>>(new Set());
  const [completedCount, setCompletedCount] = createSignal(0);

  let abortRef: AbortController | null = null;

  onMount(() => {
    fetchMeta().then((m) => {
      if (m) setMeta(m);
    });

    // Check URL params
    const params = new URLSearchParams(window.location.search);
    const urlDomain = params.get('domain');
    if (urlDomain) {
      setDomain(urlDomain);
      handleInspect(urlDomain, []);
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
      '/':      (e) => { e.preventDefault(); inputEl?.focus(); },
      'r':      (e) => {
        const d = domain();
        if (d && !loading()) { e.preventDefault(); handleInspect(d, selectors()); }
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

  function getHistory(): string[] {
    return storageGet<string[]>(HISTORY_KEY, []);
  }

  function addToHistory(d: string) {
    const history = getHistory().filter((h) => h !== d);
    history.unshift(d);
    storageSet(HISTORY_KEY, history.slice(0, MAX_HISTORY));
  }

  function handleInspect(d: string, sels: string[]) {
    if (!d.trim()) return;
    abortRef?.abort();

    setError(null);
    setCategories(new Map());
    setSummary(null);
    setCompletedCount(0);
    setLoading(true);

    const newUrl = new URL(window.location.href);
    newUrl.searchParams.set('domain', d);
    window.history.replaceState(null, '', newUrl.toString());

    abortRef = streamInspect(
      d,
      sels,
      (event: SseEvent) => {
        if (event.type === 'category') {
          setCategories((prev) => {
            const next = new Map(prev);
            next.set(event.category, event);
            return next;
          });
          setCompletedCount((c) => c + 1);
        } else if (event.type === 'summary') {
          setSummary(event);
        }
      },
      (msg: string) => { setError(msg); },
      () => { setLoading(false); addToHistory(d); },
    );
  }

  function handleSubmit(e: Event) {
    e.preventDefault();
    handleInspect(domain(), selectors());
  }

  function addSelector() {
    const val = selectorInput().trim();
    if (val && !selectors().includes(val)) {
      setSelectors([...selectors(), val]);
      setSelectorInput('');
    }
  }

  function removeSelector(s: string) {
    setSelectors(selectors().filter((x) => x !== s));
  }

  function toggleSection(cat: Category) {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) { next.delete(cat); } else { next.add(cat); }
      return next;
    });
  }

  const hasResults = () => categories().size > 0 || loading();
  const isDone = () => summary() !== null && !loading();
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
                  placeholder="example.com"
                  value={domain()}
                  onInput={(e) => setDomain(e.currentTarget.value)}
                  onFocus={() => setShowHistory(true)}
                  onBlur={() => setTimeout(() => setShowHistory(false), 200)}
                  role="combobox"
                  aria-label="Domain to inspect"
                  aria-expanded={showHistory() && getHistory().length > 0}
                  aria-autocomplete="list"
                  aria-controls="history-listbox"
                  autocomplete="off"
                  spellcheck={false}
                />
                <Show when={domain()}>
                  <button
                    type="button"
                    class="domain-input__clear"
                    aria-label="Clear"
                    onClick={() => setDomain('')}
                  >×</button>
                </Show>
                <Show when={showHistory() && getHistory().length > 0}>
                  <ul
                    class="history-dropdown"
                    id="history-listbox"
                    role="listbox"
                    onKeyDown={(e) => { if (e.key === 'Escape') setShowHistory(false); }}
                  >
                    <For each={getHistory()}>
                      {(item) => (
                        <li
                          role="option"
                          onMouseDown={() => {
                            setDomain(item);
                            handleInspect(item, selectors());
                          }}
                        >
                          {item}
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </div>
              <button type="submit" class="btn-primary" disabled={loading() || !domain().trim()}>
                {loading() ? 'Inspecting…' : 'Inspect'}
              </button>
            </div>

            <div class="selector-row">
              <span class="selector-label">DKIM selectors:</span>
              <div class="selector-chips">
                <For each={selectors()}>
                  {(s) => (
                    <span class="chip">
                      {s}
                      <button type="button" class="chip__remove" onClick={() => removeSelector(s)}>×</button>
                    </span>
                  )}
                </For>
              </div>
              <input
                type="text"
                class="selector-input"
                placeholder="Add selector"
                aria-label="DKIM selectors (comma-separated)"
                value={selectorInput()}
                onInput={(e) => setSelectorInput(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') { e.preventDefault(); addSelector(); }
                }}
              />
              <button type="button" class="btn-secondary" onClick={addSelector}>Add</button>
            </div>
          </form>

          <Show when={error()}>
            <div class="error-banner" role="alert">{error()}</div>
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
                      class="example-chip"
                      onClick={() => { setDomain(d); handleInspect(d, []); }}
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
                />
              </Show>

              <div class="section-controls">
                <div class="section-controls__left">
                  <Show when={loading()}>
                    <span class="progress-text">{completedCount()} of 12 checks complete</span>
                  </Show>
                </div>
                <div class="section-controls__right">
                  <button
                    class="filter-toggle"
                    onClick={() => setExpandAll(!expandAll())}
                    aria-pressed={expandAll()}
                  >
                    {expandAll() ? 'Collapse all' : 'Expand all'}
                  </button>
                </div>
              </div>
            </Show>

            <div class="category-list">
              <For each={CATEGORY_ORDER}>
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
                        />
                      )}
                    </Show>
                  );
                }}
              </For>
            </div>

            <Show when={isDone()}>
              <div class="cross-links">
                <CrossLink
                  href={`${meta()?.ecosystem?.dns_base_url ?? 'https://dns.netray.info'}/?q=${encodeURIComponent(domain())}`}
                  label="DNS Inspector"
                />
                <CrossLink
                  href={`${meta()?.ecosystem?.tls_base_url ?? 'https://tls.netray.info'}/?domain=${encodeURIComponent(domain())}`}
                  label="TLS Inspector"
                />
                <CrossLink
                  href={`${meta()?.ecosystem?.http_base_url ?? 'https://http.netray.info'}/?url=${encodeURIComponent(`https://${domain()}`)}`}
                  label="HTTP Inspector"
                />
              </div>
            </Show>
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
            <div class="help-section__title">Input</div>
            <code class="help-syntax">example.com</code>
            <p class="help-desc">
              Enter any domain name. Add DKIM selectors below the domain input for custom selector
              testing.
            </p>
          </div>

          <div class="help-section">
            <div class="help-section__title">Keyboard shortcuts</div>
            <table class="shortcuts-table">
              <thead>
                <tr><th>Key</th><th>Action</th></tr>
              </thead>
              <tbody>
                <tr><td class="shortcut-key">/</td><td>Focus domain input</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Submit domain (when input focused)</td></tr>
                <tr><td class="shortcut-key">r</td><td>Re-run last inspection</td></tr>
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

function OverviewCard(props: {
  summary: SummaryEvent;
  mxResult?: CheckResult;
  ipBaseUrl?: string;
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

  return (
    <div class="overview">
      <div class="overview__row">
        <div class="overview__item">
          <span class="overview__label">Verdict</span>
          <span class={`badge badge--${overallVerdict()}`}>{overallVerdict()}</span>
        </div>
        <Show when={(counts().fail ?? 0) > 0}>
          <div class="overview__item">
            <span class="overview__value overview__value--fail">{counts().fail}</span>
            <span class="overview__label">failed</span>
          </div>
        </Show>
        <Show when={(counts().warn ?? 0) > 0}>
          <div class="overview__item">
            <span class="overview__value overview__value--warn">{counts().warn}</span>
            <span class="overview__label">warnings</span>
          </div>
        </Show>
        <Show when={(counts().info ?? 0) > 0}>
          <div class="overview__item">
            <span class="overview__value overview__value--info">{counts().info}</span>
            <span class="overview__label">info</span>
          </div>
        </Show>
        <Show when={(counts().pass ?? 0) > 0}>
          <div class="overview__item">
            <span class="overview__value overview__value--pass">{counts().pass}</span>
            <span class="overview__label">passed</span>
          </div>
        </Show>
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
}) {
  const counts = () => {
    const c: Partial<Record<Verdict, number>> = {};
    for (const sc of props.result.sub_checks) {
      c[sc.verdict] = (c[sc.verdict] ?? 0) + 1;
    }
    return c;
  };

  return (
    <div class="section-card" data-card>
      <button
        class="section-card__header"
        onClick={props.onToggle}
        aria-expanded={props.open}
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
        <span class={`section-card__chevron${props.open ? ' section-card__chevron--open' : ''}`}>
          &#9660;
        </span>
      </button>

      <Show when={props.open}>
        <div class="section-card__body">
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
                    }`}
                  >
                    <span class={`badge badge--${sc.verdict}`}>{sc.verdict}</span>
                    <span class="check-list__name">{sc.name}</span>
                    <span class="check-list__message">{sc.detail}</span>
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
