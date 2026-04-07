import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createTheme } from '@netray-info/common-frontend/theme';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import Modal from '@netray-info/common-frontend/components/Modal';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import DomainInput from './components/DomainInput';
import DnsSection from './components/DnsSection';
import TlsSection from './components/TlsSection';
import IpSection from './components/IpSection';
import Summary from './components/Summary';
import ValidationChips from './components/ValidationChips';
import { startCheck } from './lib/sse';
import { fetchMeta } from './lib/api';
import { addToHistory } from './lib/history';
import { toJson, toMarkdown } from './lib/export';
import { CHECK_LABELS, CHECK_DESCRIPTIONS } from './lib/checkMeta';
import type {
  CheckState,
  DnsEvent,
  TlsEvent,
  IpEvent,
  SummaryEvent,
  DoneEvent,
  MetaResponse,
} from './lib/types';

export default function App() {
  const themeResult = createTheme('lens_theme', 'system');

  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);
  const [explain, setExplain] = createSignal(false);
  const [checkState, setCheckState] = createSignal<CheckState>('idle');
  const [dns, setDns] = createSignal<DnsEvent | null>(null);
  const [tls, setTls] = createSignal<TlsEvent | null>(null);
  const [ip, setIp] = createSignal<IpEvent | null>(null);
  const [summary, setSummary] = createSignal<SummaryEvent | null>(null);
  const [done, setDone] = createSignal<DoneEvent | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [currentDomain, setCurrentDomain] = createSignal('');
  const [toastMsg, setToastMsg] = createSignal<string | null>(null);
  const [allExpanded, setAllExpanded] = createSignal<boolean | undefined>(undefined);

  let inputEl: HTMLInputElement | undefined;
  let ssCleanup: (() => void) | null = null;

  onMount(() => {
    fetchMeta().then(m => {
      setMeta(m);
      if (m?.site_name) document.title = m.site_name;
    });

    const cleanupShortcuts = createKeyboardShortcuts({
      '/':      (e) => { e.preventDefault(); inputEl?.focus(); },
      '?':      (e) => { e.preventDefault(); setShowHelp(v => !v); },
      'e':      (e) => { e.preventDefault(); setExplain(v => !v); },
      'r':      (e) => {
        const d = currentDomain();
        if (d && checkState() !== 'loading') { e.preventDefault(); handleSubmit(d); }
      },
      'Escape': () => setShowHelp(false),
    });

    onCleanup(cleanupShortcuts);
  });

  function clearState() {
    setDns(null);
    setTls(null);
    setIp(null);
    setSummary(null);
    setDone(null);
    setError(null);
  }

  function handleSubmit(domain: string) {
    if (ssCleanup) { ssCleanup(); ssCleanup = null; }
    clearState();
    setAllExpanded(undefined);
    setCurrentDomain(domain);
    setCheckState('loading');
    addToHistory(domain);

    const url = new URL(window.location.href);
    url.searchParams.set('d', domain);
    window.history.replaceState(null, '', url.toString());

    ssCleanup = startCheck(domain, {
      onDns:     (data) => setDns(data),
      onTls:     (data) => setTls(data),
      onIp:      (data) => setIp(data),
      onSummary: (data) => setSummary(data),
      onDone:    (data) => { setDone(data); setCheckState('done'); },
      onError:   (err)  => { setError(err); setCheckState('error'); },
    });
  }

  function handleCopyMd() {
    const md = toMarkdown(currentDomain(), dns(), tls(), ip(), summary(), done());
    navigator.clipboard.writeText(md).then(() => {
      setToastMsg('Copied!');
      setTimeout(() => setToastMsg(null), 2000);
    });
  }

  function handleDownloadJson() {
    const blob = new Blob(
      [toJson(currentDomain(), dns(), tls(), ip(), summary(), done())],
      { type: 'application/json' },
    );
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${currentDomain()}-lens.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function handleClear() {
    if (ssCleanup) { ssCleanup(); ssCleanup = null; }
    clearState();
    setCurrentDomain('');
    setCheckState('idle');
    window.history.replaceState(null, '', window.location.pathname);
  }

  onCleanup(() => { if (ssCleanup) ssCleanup(); });

  const isLoading  = () => checkState() === 'loading';
  const hasResults = () => dns() !== null || tls() !== null || ip() !== null;

  const allChecks = () => [
    ...(tls()?.checks ?? []),
    ...(dns()?.checks ?? []),
    ...(ip()?.checks ?? []),
  ];

  // Restore domain from URL on load
  const params = new URLSearchParams(window.location.search);
  const initialDomain = params.get('d');
  if (initialDomain) {
    handleSubmit(initialDomain);
  }

  return (
    <>
      <a href="#main-content" class="skip-link">Skip to results</a>
      <div class="app">
        <SuiteNav current="lens" meta={meta()?.ecosystem} />

        <header class="header">
          <h1 class="logo">lens</h1>
          <span class="tagline">Domain health at a glance</span>
          <div class="header-actions">
            <button
              class="header-btn"
              type="button"
              aria-label="Open help"
              onClick={() => setShowHelp(true)}
              title="Help (?)"
            >
              ?
            </button>
            <ThemeToggle theme={themeResult} class="header-btn" />
          </div>
        </header>

        <main class="main" id="main-content">
          <DomainInput
            onSubmit={handleSubmit}
            onClear={handleClear}
            loading={isLoading()}
            value={currentDomain()}
            inputRef={(el) => { inputEl = el; }}
          />

          <Show when={hasResults() || isLoading()}>
            <div class="explain-bar">
              <button
                class={`filter-toggle${explain() ? ' filter-toggle--active' : ''}`}
                type="button"
                aria-pressed={explain()}
                onClick={() => setExplain(v => !v)}
                title="Toggle explanations (e)"
              >explain</button>
              <button
                class="filter-toggle"
                type="button"
                onClick={() => setAllExpanded(v => v === true ? false : true)}
              >{allExpanded() === true ? 'collapse all' : 'expand all'}</button>
            </div>
          </Show>

          <Show when={error()}>
            <div class="error-banner" role="alert">{error()}</div>
          </Show>

          <Show when={!hasResults() && !isLoading() && !error()}>
            <div class="welcome">
              <p class="welcome-tagline">
                TLS certificate status, DNS health, and IP reputation — checked together, streamed as they arrive.
              </p>
            </div>
          </Show>

          <Show when={summary()}>
            {(s) => <Summary summary={s()} done={done()} />}
          </Show>

          <Show when={hasResults() || isLoading()}>
            <div class="chips-row">
              <ValidationChips checks={allChecks()} />
              <Show when={summary() && done()}>
                <div class="export-actions">
                  <button class="export-btn" type="button" onClick={handleCopyMd}>copy MD</button>
                  <button class="export-btn" type="button" onClick={handleDownloadJson}>JSON</button>
                </div>
              </Show>
            </div>
            <div class="section-grid" role="status" aria-live="polite" aria-label="Check results">
              <TlsSection
                data={tls()}
                loading={isLoading() && tls() === null}
                error={error() ?? undefined}
                explain={explain()}
                expanded={allExpanded()}
              />
              <DnsSection
                data={dns()}
                loading={isLoading() && dns() === null}
                error={error() ?? undefined}
                explain={explain()}
                expanded={allExpanded()}
              />
              <IpSection
                data={ip()}
                loading={isLoading() && ip() === null}
                error={error() ?? undefined}
                explain={explain()}
                expanded={allExpanded()}
              />
            </div>
          </Show>
        </main>

        <SiteFooter
          aboutText={
            <>
              <em>lens</em> checks TLS certificate validity, DNS health, and IP reputation for any
              domain — results stream in as each check completes. Part of the{' '}
              <a href="https://netray.info">netray.info</a> suite.
            </>
          }
          links={[
            { href: '/docs',                    label: 'API Docs', external: true },
            { href: 'https://lukas.pustina.de', label: 'Author',   external: true },
          ]}
          version={meta()?.version}
        />

        <Show when={toastMsg()}>
          <div class="toast" role="status" aria-live="polite">{toastMsg()}</div>
        </Show>

        <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
          <div class="help-section">
            <div class="help-section__title">Input</div>
            <code class="help-syntax">example.com</code>
            <p class="help-desc">
              Enter any domain name to check its TLS certificate validity, DNS health, and IP reputation simultaneously.
            </p>
          </div>
          <div class="help-section">
            <div class="help-section__title">Scoring</div>
            <p class="help-desc">
              Each check contributes a weighted score. The overall grade is a weighted average
              across TLS (45%), DNS (35%), and IP (20%).
            </p>
            <Show when={meta()?.profile}>
              {(profile) => (
                <div class="help-scoring">
                  <div class="help-scoring__thresholds">
                    {Object.entries(profile().thresholds)
                      .sort((a, b) => b[1] - a[1])
                      .map(([grade, min]) => (
                        <span class="help-scoring__grade-chip">{grade} ≥ {min}%</span>
                      ))}
                  </div>
                  <div class="help-hard-fail">
                    <p class="help-hard-fail__heading">Hard fail checks — any of these force grade F:</p>
                    <For each={[...profile().hard_fail.tls, ...profile().hard_fail.dns]}>
                      {(name) => (
                        <div class="help-hard-fail__item">
                          <span class="help-hard-fail__label">{CHECK_LABELS[name] ?? name}</span>
                          <Show when={CHECK_DESCRIPTIONS[name]}>
                            <span class="help-hard-fail__desc">{CHECK_DESCRIPTIONS[name]}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </Show>
          </div>
          <div class="help-section">
            <div class="help-section__title">Keyboard shortcuts</div>
            <div class="help-keys">
              <div class="help-key"><kbd>/</kbd><span>Focus input</span></div>
              <div class="help-key"><kbd>Enter</kbd><span>Submit</span></div>
              <div class="help-key"><kbd>r</kbd><span>Re-run last check</span></div>
              <div class="help-key"><kbd>e</kbd><span>Toggle explain mode</span></div>
              <div class="help-key"><kbd>?</kbd><span>Open this help</span></div>
            </div>
          </div>
        </Modal>
      </div>
    </>
  );
}
