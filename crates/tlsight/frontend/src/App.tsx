import { createSignal, createResource, Show, For, onMount, onCleanup } from 'solid-js';
import HostInput from './components/HostInput';
import QueryHistory from './components/QueryHistory';
import ExportButtons from './components/ExportButtons';
import ValidationSummary from './components/ValidationSummary';
import ChainView from './components/ChainView';
import CertDetail from './components/CertDetail';
import TlsParams from './components/TlsParams';
import PortTabs from './components/PortTabs';
import ConsistencyView from './components/ConsistencyView';
import CrossLinks from './components/CrossLinks';
import CtView from './components/CtView';
import { CaaView, TlsaView } from './components/DnsInfo';
import { inspect, fetchMeta } from './lib/api';
import { addToHistory } from './lib/history';
import type { InspectResponse, PortResult } from './lib/types';

type Theme = 'dark' | 'light' | 'system';
const THEME_KEY = 'tlsight_theme';

function getSystemTheme(): 'dark' | 'light' {
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function getSavedTheme(): Theme | null {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === 'light' || saved === 'dark' || saved === 'system') return saved;
  } catch { /* ignore */ }
  return null;
}

function resolveTheme(t: Theme): 'dark' | 'light' {
  return t === 'system' ? getSystemTheme() : t;
}

const EXAMPLES: { title: string; desc: string; queries: string[] }[] = [
  {
    title: 'Inspect',
    desc: 'Full TLS inspection — certificate chain, validation, OCSP, CT, DNS cross-checks. The single view for "is this domain\'s TLS correct?"',
    queries: ['example.com'],
  },
  {
    title: 'Multi-port',
    desc: 'Scan multiple TLS ports in one shot — compare HTTPS, SMTPS, IMAPS, POP3S certificates side by side.',
    queries: ['example.com:443,465,993'],
  },
  {
    title: 'Deep dive',
    desc: 'Inspect a specific port with full CAA and DANE cross-validation. See if the issuing CA is authorized and TLSA records match.',
    queries: ['example.com:8443'],
  },
];

export default function App() {
  const [result, setResult] = createSignal<InspectResponse | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [selectedPort, setSelectedPort] = createSignal<number>(443);
  const [theme, setTheme] = createSignal<Theme>(getSavedTheme() ?? 'system');

  const [meta] = createResource(fetchMeta);

  let inputEl: HTMLInputElement | undefined;

  function applyTheme(t: Theme) {
    document.documentElement.setAttribute('data-theme', resolveTheme(t));
  }

  function toggleTheme() {
    const next: Theme = theme() === 'system' ? 'dark' : theme() === 'dark' ? 'light' : 'system';
    setTheme(next);
    applyTheme(next);
    try { localStorage.setItem(THEME_KEY, next); } catch { /* ignore */ }
  }

  const themeIcon = () =>
    theme() === 'system' ? '\u25D0' : theme() === 'dark' ? '\u263E' : '\u2600';

  const themeTitle = () =>
    theme() === 'system' ? 'Theme: System'
    : theme() === 'dark' ? 'Theme: Dark'
    : 'Theme: Light';

  function isInputFocused() {
    return document.activeElement?.tagName === 'INPUT' || document.activeElement?.tagName === 'TEXTAREA';
  }

  let mediaQuery: MediaQueryList | undefined;
  const onSystemThemeChange = () => {
    if (theme() === 'system') applyTheme('system');
  };

  onMount(() => {
    applyTheme(theme());
    mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', onSystemThemeChange);

    const handler = (e: KeyboardEvent) => {
      if (e.key === '/' && !isInputFocused()) {
        e.preventDefault();
        inputEl?.focus();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === 'l') {
        e.preventDefault();
        inputEl?.focus();
      }
      if (e.key === 'Escape') {
        inputEl?.blur();
      }
    };
    document.addEventListener('keydown', handler);
    onCleanup(() => {
      document.removeEventListener('keydown', handler);
      mediaQuery?.removeEventListener('change', onSystemThemeChange);
    });
  });

  const handleInspect = async (input: string) => {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const data = await inspect(input);
      setResult(data);
      addToHistory(input);
      if (data.ports.length > 0) {
        setSelectedPort(data.ports[0].port);
      }
      const url = new URL(window.location.href);
      url.searchParams.set('h', input);
      window.history.replaceState(null, '', url.toString());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const currentPort = (): PortResult | undefined => {
    const r = result();
    if (!r) return undefined;
    return r.ports.find(p => p.port === selectedPort()) ?? r.ports[0];
  };

  const allIps = (): string[] => {
    const r = result();
    if (!r) return [];
    const ips = new Set<string>();
    for (const port of r.ports) {
      for (const ip of port.ips) {
        ips.add(ip.ip);
      }
    }
    return [...ips];
  };

  // Check URL for initial query
  const params = new URLSearchParams(window.location.search);
  const initialQuery = params.get('h');
  if (initialQuery) {
    handleInspect(initialQuery);
  }

  return (
    <div class="app">
      <a href="#main-content" class="skip-link">Skip to results</a>

      <header class="header">
        <h1 class="logo">tlsight</h1>
        <span class="tagline">TLS, illuminated</span>
        <div class="header-actions">
          <button
            class="header-btn"
            onClick={toggleTheme}
            title={themeTitle()}
          >
            {themeIcon()}
          </button>
        </div>
      </header>

      <main class="main">
        <HostInput onSubmit={handleInspect} loading={loading()} inputRef={el => (inputEl = el)} />
        <QueryHistory onSelect={handleInspect} />

        <Show when={error()}>
          <div class="error-banner">{error()}</div>
        </Show>

        <Show when={loading()}>
          <div class="loading-indicator">
            <div class="spinner" />
            Inspecting...
          </div>
        </Show>

        {/* Empty state — shown before any query */}
        <Show when={!result() && !loading() && !error()}>
          <div class="welcome">
            <p class="welcome-tagline">
              Full certificate chain inspection, TLS configuration analysis, and DNS cross-validation — in one view.
            </p>
            <div class="welcome-cards">
              <For each={EXAMPLES}>
                {(ex) => (
                  <div class="welcome-card">
                    <div class="welcome-card-title">{ex.title}</div>
                    <p class="welcome-card-desc">{ex.desc}</p>
                    <For each={ex.queries}>
                      {(q) => (
                        <button
                          class="welcome-example"
                          onClick={() => handleInspect(q)}
                          title="Click to run"
                        >
                          {q}
                        </button>
                      )}
                    </For>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>

        <Show when={result()}>
          {(res) => {
            const r = res();
            return (
              <div id="main-content" class="results">
                <ValidationSummary summary={r.summary} />

                {/* Results toolbar */}
                <div class="results-toolbar">
                  <div class="results-summary">
                    <span class="results-summary-item">{allIps().length} IP{allIps().length !== 1 ? 's' : ''}</span>
                    <span class="results-summary-sep">/</span>
                    <span class="results-summary-item">{r.ports.length} port{r.ports.length !== 1 ? 's' : ''}</span>
                    <span class="results-summary-sep">/</span>
                    <span class="results-summary-item">{r.duration_ms}ms</span>
                    <Show when={(r.skipped_ips ?? []).length > 0}>
                      <span class="results-summary-sep">/</span>
                      <span class="results-summary-item" style={{ color: 'var(--warn)' }}>{r.skipped_ips?.length} skipped</span>
                    </Show>
                  </div>
                  <ExportButtons result={r} />
                </div>

                <Show when={r.warnings?.length}>
                  <div class="warnings">
                    <For each={r.warnings}>
                      {(w) => <div class="warning-banner">{w}</div>}
                    </For>
                  </div>
                </Show>

                <Show when={r.ports.length > 1}>
                  <PortTabs
                    ports={r.ports.map(p => p.port)}
                    selected={selectedPort()}
                    onSelect={setSelectedPort}
                  />
                </Show>

                <Show when={r.dns?.caa}>
                  {(caa) => <CaaView caa={caa()} />}
                </Show>

                <Show when={currentPort()}>
                  {(port) => {
                    const p = port();
                    return (
                      <>
                        <Show when={p.consistency}>
                          {(c) => <ConsistencyView consistency={c()} />}
                        </Show>

                        <Show when={p.tlsa}>
                          {(tlsa) => <TlsaView tlsa={tlsa()} />}
                        </Show>

                        <For each={p.ips}>
                          {(ipResult) => (
                            <div class="ip-section">
                              <div class="ip-section__header">
                                <span class="ip-section__ip">{ipResult.ip}</span>
                                <span class="ip-section__version">{ipResult.ip_version}</span>
                                <Show when={ipResult.error}>
                                  {(err) => <span class="ip-section__error">{err().message}</span>}
                                </Show>
                              </div>

                              <Show when={ipResult.tls}>
                                {(tls) => <TlsParams params={tls()} />}
                              </Show>

                              <Show when={ipResult.ct}>
                                {(ct) => <CtView ct={ct()} />}
                              </Show>

                              <Show when={ipResult.chain}>
                                {(chain) => (
                                  <>
                                    <ChainView chain={chain()} />
                                    <div class="cert-details">
                                      <For each={chain()}>
                                        {(cert) => <CertDetail cert={cert} />}
                                      </For>
                                    </div>
                                  </>
                                )}
                              </Show>
                            </div>
                          )}
                        </For>
                      </>
                    );
                  }}
                </Show>

                <Show when={meta()}>
                  <CrossLinks meta={meta()!} hostname={r.hostname} ips={allIps()} />
                </Show>
              </div>
            );
          }}
        </Show>
      </main>

      <footer class="footer">
        <a class="footer-link" href="/docs" target="_blank" rel="noopener noreferrer">API Docs</a>
        <span class="footer-sep">&middot;</span>
        <Show when={meta()?.version}>
          <span class="footer-text">v{meta()!.version}</span>
        </Show>
      </footer>
    </div>
  );
}
