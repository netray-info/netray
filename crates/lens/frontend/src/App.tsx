import { createSignal, onCleanup, onMount, Show } from 'solid-js';
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
            <ValidationChips checks={allChecks()} />
            <div class="section-grid" role="status" aria-live="polite" aria-label="Check results">
              <TlsSection
                data={tls()}
                loading={isLoading() && tls() === null}
                error={error() ?? undefined}
                explain={explain()}
              />
              <DnsSection
                data={dns()}
                loading={isLoading() && dns() === null}
                error={error() ?? undefined}
                explain={explain()}
              />
              <IpSection
                data={ip()}
                loading={isLoading() && ip() === null}
                error={error() ?? undefined}
                explain={explain()}
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

        <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
          <div class="help-section">
            <div class="help-section__title">Input</div>
            <code class="help-syntax">example.com</code>
            <p class="help-desc">
              Enter any domain name to check its TLS certificate validity, DNS health, and IP reputation simultaneously.
            </p>
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
