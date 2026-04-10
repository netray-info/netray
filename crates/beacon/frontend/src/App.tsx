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
  Grade,
  IpEnrichment,
  SseEvent,
  SubCheck,
  SummaryEvent,
  Verdict,
} from './lib/types';
import { CATEGORY_LABELS, CATEGORY_ORDER } from './lib/types';

const HISTORY_KEY = 'beacon_history';
const MAX_HISTORY = 20;

const GRADE_COLORS: Record<Grade, string> = {
  A: 'var(--color-pass)',
  B: 'var(--color-info)',
  C: 'var(--color-warn)',
  D: 'var(--color-warn)',
  F: 'var(--color-fail)',
};

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
      if (m?.site_name) document.title = m.site_name;
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

  onCleanup(() => {
    abortRef?.abort();
  });

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

    // Update URL
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
      (msg: string) => {
        setError(msg);
      },
      () => {
        setLoading(false);
        addToHistory(d);
      },
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
      if (next.has(cat)) {
        next.delete(cat);
      } else {
        next.add(cat);
      }
      return next;
    });
  }

  const hasResults = () => categories().size > 0 || summary() !== null || loading();
  const isDone = () => summary() !== null && !loading();
  const isIdle = () => !hasResults() && !loading() && !error();

  function verdictCounts(): Record<Verdict, number> {
    const counts: Record<Verdict, number> = { skip: 0, pass: 0, info: 0, warn: 0, fail: 0 };
    for (const r of categories().values()) {
      counts[r.verdict]++;
    }
    return counts;
  }

  return (
    <>
      <SuiteNav current="email" meta={meta()?.ecosystem as SuiteNavEcosystem} />

      <div class="app">
      <header class="header">
        <h1 class="logo">beacon</h1>
        <span class="tagline">email security, graded</span>
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

      <main class="main">
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
                  class="clear-btn"
                  aria-label="Clear"
                  onClick={() => setDomain('')}
                >
                  x
                </button>
              </Show>
              <Show when={showHistory() && getHistory().length > 0}>
                <ul
                  class="history-dropdown"
                  id="history-listbox"
                  role="listbox"
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') setShowHistory(false);
                  }}
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
              {loading() ? 'Inspecting...' : 'Inspect'}
            </button>
          </div>

          <div class="selector-row">
            <span class="selector-label">DKIM selectors:</span>
            <div class="selector-chips">
              <For each={selectors()}>
                {(s) => (
                  <span class="chip">
                    {s}
                    <button type="button" class="chip__remove" onClick={() => removeSelector(s)}>
                      x
                    </button>
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
                if (e.key === 'Enter') {
                  e.preventDefault();
                  addSelector();
                }
              }}
            />
            <button type="button" class="btn-secondary" onClick={addSelector}>
              Add
            </button>
          </div>
        </form>

        <Show when={isIdle()}>
          <div class="idle-state">
            <p class="idle-state__text">
              Enter a domain to inspect its email security posture. Checks MX, SPF, DKIM, DMARC,
              MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and DNSBL records.
            </p>
            <div class="mode-cards">
              <For each={EXAMPLE_DOMAINS}>
                {(d) => (
                  <button
                    class="mode-card"
                    onClick={() => {
                      setDomain(d);
                      handleInspect(d, []);
                    }}
                  >
                    {d}
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>

        <Show when={error()}>
          <div class="error-alert" role="alert">
            {error()}
          </div>
        </Show>

        <Show when={hasResults()}>
          <div class="results">
            <Show when={summary()}>
              {(s) => (
                <div class="grade-display" style={{ '--grade-color': GRADE_COLORS[s().grade] }}>
                  <span class="grade-display__letter">{s().grade}</span>
                  <span class="grade-display__label">Overall Grade</span>
                </div>
              )}
            </Show>

            <Show when={categories().size > 0 && !loading()}>
              <div class="verdict-strip">
                {(['pass', 'warn', 'fail', 'info'] as Verdict[]).map((v) => (
                  <span class={`verdict-strip__chip badge badge--${v}`}>
                    {verdictCounts()[v]} {v}
                  </span>
                ))}
              </div>
              <div class="results__controls">
                <button class="btn-secondary btn-small" onClick={() => setExpandAll(!expandAll())}>
                  {expandAll() ? 'Collapse All' : 'Expand All'}
                </button>
              </div>
            </Show>

            <Show when={loading()}>
              <div class="loading-indicator" role="status" aria-live="polite">
                {completedCount()} of 12 checks complete
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
                          <div class="section-card card--pending" aria-busy="true" aria-label={`Loading ${CATEGORY_LABELS[cat]}...`}>
                            <div class="section-card__header">
                              <span class="skeleton skeleton-line" style={{ width: '3rem', height: '1.25rem', 'flex-shrink': '0', margin: 0 }} />
                              <span class="section-card__title">{CATEGORY_LABELS[cat]}</span>
                              <span class="skeleton skeleton-line" style={{ width: '40%', height: '0.875rem', 'margin-left': 'auto', 'margin-top': 0, 'margin-bottom': 0 }} />
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
          </div>
        </Show>
      </main>

      <SiteFooter
        aboutText={
          <>
            <em>beacon</em> performs DNS-only email security posture analysis. Built in{' '}
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

      {/* Help modal */}
      <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
        <div class="help-section">
          <div class="help-section__title">About</div>
          <p class="help-desc">
            beacon checks email security posture via DNS only — no mail servers contacted.
            Inspects MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, and DNSBL
            and assigns an A–F grade.{' '}
            <a href="https://netray.info/guide/email-auth.html" target="_blank" rel="noopener noreferrer">Email auth guide ↗</a>
          </p>
        </div>

        <div class="help-section">
          <div class="help-section__title">Input</div>
          <code class="help-syntax">example.com</code>
          <p class="help-desc">Enter any domain name. DKIM selectors can be added below the domain input for custom selector testing.</p>
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
    </>
  );
}

function CategorySection(props: {
  result: CheckResult;
  open: boolean;
  onToggle: () => void;
}) {
  return (
    <div class="section-card" data-card>
      <button
        class="section-card__header"
        onClick={props.onToggle}
        aria-expanded={props.open}
      >
        <span class={`badge badge--${props.result.verdict}`}>{props.result.verdict}</span>
        <span class="section-card__title">{CATEGORY_LABELS[props.result.category]}</span>
        <span class="section-card__detail">{props.result.detail}</span>
        <span class={`section-card__chevron${props.open ? ' section-card__chevron--open' : ''}`}>
          &#9660;
        </span>
      </button>
      <Show when={props.open}>
        <div class="section-card__body">
          <For each={props.result.sub_checks}>
            {(sc: SubCheck) => (
              <div class="sub-check">
                <span class={`badge badge--${sc.verdict} badge--small`}>{sc.verdict}</span>
                <span class="sub-check__name">{sc.name}</span>
                <span class="sub-check__detail">{sc.detail}</span>
              </div>
            )}
          </For>
          <Show when={props.result.sub_checks.length === 0}>
            <p class="section-card__empty">No sub-checks</p>
          </Show>
          <Show when={props.result.enrichment && props.result.enrichment.length > 0}>
            <div class="enrichment-badges">
              <For each={props.result.enrichment!}>
                {(e: IpEnrichment) => (
                  <span class="enrichment-badge">
                    <span class="enrichment-badge__ip">{e.ip}</span>
                    <Show when={e.asn}><span class="enrichment-badge__tag">AS{e.asn}</span></Show>
                    <Show when={e.org}><span class="enrichment-badge__tag">{e.org}</span></Show>
                    <Show when={e.ip_type}><span class="enrichment-badge__tag">{e.ip_type}</span></Show>
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
