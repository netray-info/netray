import { createSignal, createResource, Show, For, onMount, onCleanup } from 'solid-js';
import HostInput from './components/HostInput';
import ExportButtons from './components/ExportButtons';
import ValidationSummary from './components/ValidationSummary';
import PortTabs from './components/PortTabs';
import ConsistencyView from './components/ConsistencyView';
import CrossLinks from './components/CrossLinks';
import IpCard from './components/IpCard';
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
  const [showHelp, setShowHelp] = createSignal(false);
  const [lastQuery, setLastQuery] = createSignal('');

  const [meta] = createResource(fetchMeta);

  let inputEl: HTMLInputElement | undefined;
  let modalCloseBtn: HTMLButtonElement | undefined;
  let preModalFocus: HTMLElement | null = null;

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
      if (e.key === '?' && !isInputFocused()) {
        e.preventDefault();
        setShowHelp(v => !v);
        return;
      }

      if (e.key === 'Escape') {
        if (showHelp()) {
          setShowHelp(false);
          e.preventDefault();
          return;
        }
        inputEl?.blur();
        return;
      }

      if (e.key === '/' && !isInputFocused()) {
        e.preventDefault();
        inputEl?.focus();
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key === 'l') {
        e.preventDefault();
        inputEl?.focus();
        return;
      }

      if (e.key === 'r' && !isInputFocused()) {
        const q = lastQuery();
        if (q && !loading()) {
          e.preventDefault();
          handleInspect(q);
        }
        return;
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
    setLastQuery(input);
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

  const openHelp = () => {
    preModalFocus = document.activeElement as HTMLElement | null;
    setShowHelp(true);
    requestAnimationFrame(() => modalCloseBtn?.focus());
  };

  const closeHelp = () => {
    setShowHelp(false);
    preModalFocus?.focus();
    preModalFocus = null;
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
            onClick={openHelp}
            title="Help (?)"
          >?</button>
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
                {/* Statistics first */}
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

                {/* Validation second */}
                <ValidationSummary summary={r.summary} />

                <Show when={r.ports.length > 1}>
                  <PortTabs
                    ports={r.ports.map(p => p.port)}
                    selected={selectedPort()}
                    onSelect={setSelectedPort}
                  />
                </Show>

                <Show when={currentPort()}>
                  {(port) => {
                    const p = port();
                    return (
                      <>
                        <Show when={p.consistency}>
                          {(c) => <ConsistencyView consistency={c()} />}
                        </Show>

                        <For each={p.ips}>
                          {(ipResult, i) => (
                            <IpCard result={ipResult} defaultExpanded={i() === 0} />
                          )}
                        </For>
                      </>
                    );
                  }}
                </Show>

                <Show when={r.dns?.caa}>
                  {(caa) => <CaaView caa={caa()} />}
                </Show>

                <Show when={currentPort()?.tlsa}>
                  {(tlsa) => <TlsaView tlsa={tlsa()} />}
                </Show>

                <Show when={meta()}>
                  <CrossLinks meta={meta()!} hostname={r.hostname} ips={allIps()} />
                </Show>
              </div>
            );
          }}
        </Show>
      </main>

      {/* Help modal */}
      <Show when={showHelp()}>
        <div class="modal-overlay" onClick={closeHelp}>
          <div
            class="modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="help-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="modal__header">
              <h2 id="help-title">Help</h2>
              <button class="modal__close" ref={modalCloseBtn} onClick={closeHelp}>&times;</button>
            </div>

            <div class="help-section">
              <div class="help-section__title">Input syntax</div>
              <code class="help-syntax">hostname[:port[,port...]]</code>
              <p class="help-desc">Enter a hostname to inspect its TLS certificate. Optionally append ports separated by commas.</p>
            </div>

            <div class="help-section">
              <div class="help-section__title">Keyboard shortcuts</div>
              <div class="help-keys">
                <div class="help-key"><kbd>/</kbd><span>Focus input</span></div>
                <div class="help-key"><kbd>Enter</kbd><span>Submit query</span></div>
                <div class="help-key"><kbd>r</kbd><span>Re-run last query</span></div>
                <div class="help-key"><kbd>Escape</kbd><span>Blur input / close help</span></div>
                <div class="help-key"><kbd>?</kbd><span>Toggle help</span></div>
              </div>
            </div>

            <div class="help-section">
              <div class="help-section__title">History</div>
              <p class="help-desc">Previous queries are available via arrow keys when the input is focused.</p>
              <div class="help-keys">
                <div class="help-key"><kbd>&uarr;</kbd><span>Previous query</span></div>
                <div class="help-key"><kbd>&darr;</kbd><span>Next query</span></div>
              </div>
            </div>

            <div class="help-section">
              <div class="help-section__title">What the results mean</div>
              <p class="help-desc"><strong>CAA records</strong> — DNS Certification Authority Authorization. These records declare which CAs are allowed to issue certificates for a domain. If the issuing CA is not listed, the certificate may violate the domain owner's policy.</p>
              <p class="help-desc"><strong>IP consistency</strong> — When a hostname resolves to multiple IPs, this check compares whether all IPs serve the same certificate, TLS version, and cipher suite. Mismatches may indicate misconfigured servers, stale deployments, or CDN inconsistencies.</p>
              <p class="help-desc"><strong>DANE/TLSA</strong> — DNS-based Authentication of Named Entities. TLSA records pin certificates or CAs in DNS, validated via DNSSEC. Provides an alternative trust path independent of the CA system.</p>
            </div>
          </div>
        </div>
      </Show>

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
