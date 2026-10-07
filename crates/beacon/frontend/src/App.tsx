import { createSignal, onMount, onCleanup, Show, createEffect } from 'solid-js';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import type { SuiteNavEcosystem } from '@netray-info/common-frontend/components/SuiteNav';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import AppFooter from './components/AppFooter';
import { createTheme } from '@netray-info/common-frontend/theme';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import { addToHistory } from './lib/history';
import { fetchMeta, streamInspect } from './lib/api';
import type { MetaResponse } from './lib/api';
import type {
  Category,
  CheckResult,
  SseEvent,
  SummaryEvent,
} from './lib/types';
import { CATEGORY_ORDER } from './lib/types';
import { parseQuery, formatQuery } from './lib/parse';
import DomainInput from './components/DomainInput';
import SummaryCard from './components/SummaryCard';
import ResultsGrid from './components/ResultsGrid';
import HelpModal from './components/HelpModal';
import IdleState from './components/IdleState';

export default function App() {
  const theme = createTheme('beacon_theme', 'system');
  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);

  // Input state
  const [query, setQuery] = createSignal('');
  const [inspectedDomain, setInspectedDomain] = createSignal('');
  const [inspectedSelectors, setInspectedSelectors] = createSignal<string[]>([]);
  const [lastInspectedQuery, setLastInspectedQuery] = createSignal<string>('');
  let inputEl: HTMLInputElement | undefined;

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
        const last = lastInspectedQuery();
        if (last.trim() && !loading()) { e.preventDefault(); runQuery(last); }
      },
      'j':      navigateCards,
      'k':      navigateCards,
      'Enter':  expandActiveCard,
      'Escape': () => { setShowHelp(false); },
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
          <DomainInput
            query={query}
            onQueryChange={setQuery}
            onSubmit={handleSubmit}
            loading={loading}
            inputRef={(el) => { inputEl = el; }}
            inspectedDomain={inspectedDomain}
            inspectedSelectors={inspectedSelectors}
            onRunQuery={runQuery}
          />

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
            <IdleState onRunQuery={runQuery} onSetQuery={setQuery} />
          </Show>

          <Show when={hasResults()}>
            <Show when={loading() && categories().size === 0}>
              <div class="loading" role="status" aria-live="polite">
                <div class="spinner" />
                <span>Inspecting…</span>
              </div>
            </Show>

            <Show when={summary() !== null}>
              <SummaryCard
                summary={summary()!}
                mxResult={categories().get('mx')}
                ipBaseUrl={meta()?.ecosystem?.ip_base_url}
                domain={inspectedDomain()}
                categories={categories()}
                clientDurationMs={clientDurationMs()}
              />
            </Show>

            <ResultsGrid
              categories={categories()}
              loading={loading()}
              openSections={openSections()}
              onToggleSection={toggleSection}
              showExplanations={showExplanations()}
              domain={inspectedDomain()}
              ecosystem={meta()?.ecosystem}
              mxResult={categories().get('mx')}
              completedCount={completedCount()}
              expandAll={expandAll()}
              onSetExpandAll={setExpandAll}
              onSetShowExplanations={setShowExplanations}
            />
          </Show>
        </main>

        <AppFooter version={meta()?.version} />

        <HelpModal open={showHelp()} onClose={() => setShowHelp(false)} />
    </div>
  );
}
