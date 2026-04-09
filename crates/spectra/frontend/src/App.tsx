import { createSignal, Show, For, onMount } from 'solid-js';
import UrlInput from './components/UrlInput';
import HeadersView from './components/HeadersView';
import RedirectChain from './components/RedirectChain';
import SecurityAudit from './components/SecurityAudit';
import CspAnalysis from './components/CspAnalysis';
import CorsReport from './components/CorsReport';
import CookieInspector from './components/CookieInspector';
import CachingView from './components/CachingView';
import FingerprintView from './components/FingerprintView';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import type { SuiteNavEcosystem } from '@netray-info/common-frontend/components/SuiteNav';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import Modal from '@netray-info/common-frontend/components/Modal';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import { createTheme } from '@netray-info/common-frontend/theme';
import { inspect, fetchMeta } from './lib/api';
import type { MetaResponse } from './lib/api';
import { addToHistory } from './lib/history';
import type { InspectResponse, CheckStatus } from './lib/types';

const EXAMPLES = [
  'example.com',
  'github.com',
  'cloudflare.com',
];

export default function App() {
  const [result, setResult] = createSignal<InspectResponse | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);
  const theme = createTheme('spectra_theme', 'system');

  // Read URL param for initial query
  const params = new URLSearchParams(window.location.search);
  const initialUrl = params.get('url') ?? '';

  onMount(() => {
    fetchMeta()
      .then(m => {
        setMeta(m);
        if (m?.site_name) document.title = m.site_name;
      })
      .catch(() => {});
  });

  async function handleInspect(url: string) {
    setError(null);
    setResult(null);
    setLoading(true);

    try {
      const data = await inspect(url);
      setResult(data);
      addToHistory(url);

      // Update URL for sharing
      const newUrl = new URL(window.location.href);
      newUrl.searchParams.set('url', url);
      window.history.replaceState({}, '', newUrl.toString());
    } catch (e: any) {
      setError(e.message || 'Inspection failed');
    } finally {
      setLoading(false);
    }
  }

  // Auto-inspect if URL param present
  if (initialUrl) {
    handleInspect(initialUrl);
  }

  function verdictClass(status: CheckStatus): string {
    return `badge badge--${status}`;
  }

  return (
      <div class="app">
      <SuiteNav current={'http' as any} meta={meta()?.ecosystem as SuiteNavEcosystem} />
      <a class="skip-link" href="#results">Skip to results</a>

      <main class="container">
        <div class="header">
          <div>
            <h1 class="logo">spectra</h1>
            <p class="header__tagline">HTTP headers, decoded</p>
          </div>
          <div class="header-actions">
            <ThemeToggle theme={theme} />
            <button class="header-btn" onClick={() => setShowHelp(true)} aria-label="Open help" title="Help (?)">?</button>
          </div>
        </div>

        <UrlInput
          onSubmit={handleInspect}
          loading={loading()}
          initialValue={initialUrl}
        />

        <Show when={error()}>
          <div class="error-banner" role="alert">{error()}</div>
        </Show>

        <Show when={loading()}>
          <div class="loading" role="status" aria-live="polite">
            <div class="spinner" />
            <span>Inspecting...</span>
          </div>
        </Show>

        <div id="results">
          <Show when={result()}>
            {(r) => {
              const data = r();
              return (
                <>
                  {/* Overview bar */}
                  <div class="overview">
                    <div class="overview__item">
                      <span class="overview__label">Status</span>
                      <span class="overview__value">{data.status}</span>
                    </div>
                    <div class="overview__item">
                      <span class="overview__label">HTTP</span>
                      <span class="overview__value">{data.http_version}</span>
                    </div>
                    <Show when={data.compression}>
                      <div class="overview__item">
                        <span class="overview__label">Encoding</span>
                        <span class="overview__value">{data.compression}</span>
                      </div>
                    </Show>
                    <div class="overview__item">
                      <span class="overview__label">Duration</span>
                      <span class="overview__value">{data.duration_ms}ms</span>
                    </div>
                    <div class="overview__item">
                      <span class="overview__label">IP</span>
                      <a href={data.enrichment.detail_url} class="overview__value" target="_blank" rel="noopener">
                        {data.enrichment.ip}
                      </a>
                    </div>
                    <Show when={data.enrichment.org}>
                      <div class="overview__item">
                        <span class="overview__label">Org</span>
                        <span class="overview__value">{data.enrichment.org}</span>
                      </div>
                    </Show>
                    <Show when={data.alt_svc}>
                      <div class="overview__item">
                        <span class="overview__label">Alt-Svc</span>
                        <span class="overview__value mono" style={{ 'font-size': '0.75rem' }}>
                          {data.alt_svc}
                        </span>
                      </div>
                    </Show>
                    <div class="overview__item">
                      <span class="overview__label">Verdict</span>
                      <span class={verdictClass(data.quality.verdict)}>
                        {data.quality.verdict}
                      </span>
                    </div>
                  </div>

                  {/* Quality checks */}
                  <div class="section">
                    <div class="section__title">Quality Assessment</div>
                    <ul class="check-list">
                      <For each={data.quality.checks}>
                        {(check) => (
                          <li class="check-list__item">
                            <span class={`badge badge--${check.status}`}>{check.status}</span>
                            <span class="check-list__name">{check.name}</span>
                            <span class="check-list__message">{check.message ?? ''}</span>
                          </li>
                        )}
                      </For>
                    </ul>
                  </div>

                  {/* Redirects */}
                  <RedirectChain
                    redirects={data.redirects}
                    finalUrl={data.final_url}
                    httpUpgrade={data.http_upgrade}
                  />

                  {/* Security headers */}
                  <SecurityAudit security={data.security} />

                  {/* CSP detail */}
                  <CspAnalysis csp={data.security.csp} />

                  {/* CORS */}
                  <CorsReport cors={data.cors} />

                  {/* Cookies */}
                  <CookieInspector cookies={data.cookies} />

                  {/* Caching + CDN */}
                  <CachingView caching={data.caching} cdn={data.cdn} />

                  {/* Fingerprint + deprecated */}
                  <FingerprintView
                    fingerprint={data.fingerprint}
                    deprecatedHeaders={data.deprecated_headers}
                  />

                  {/* All headers */}
                  <HeadersView headers={data.headers} />
                </>
              );
            }}
          </Show>
        </div>

        <Show when={!result() && !loading() && !error()}>
          <div class="empty-state">
            <div class="empty-state__title">Inspect any URL</div>
            <p>Enter a URL to analyze its HTTP headers, security posture, and server configuration.</p>
            <div class="example-chips">
              <For each={EXAMPLES}>
                {(example) => (
                  <button
                    class="example-chip"
                    onClick={() => handleInspect(`https://${example}`)}
                  >
                    {example}
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>
      </main>

      <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
        <div class="help-section">
          <div class="help-section__title">What is spectra?</div>
          <p class="help-desc">Enter a URL to inspect its HTTP response headers, security posture, cookies, caching, CORS policy, and CSP configuration.</p>
        </div>

        <div class="help-section">
          <div class="help-section__title">Keyboard shortcuts</div>
          <div class="help-keys">
            <div class="help-key"><kbd>/</kbd><span>Focus input</span></div>
            <div class="help-key"><kbd>Enter</kbd><span>Submit URL</span></div>
            <div class="help-key"><kbd>Escape</kbd><span>Close help</span></div>
            <div class="help-key"><kbd>?</kbd><span>Toggle help</span></div>
          </div>
        </div>
      </Modal>

      <SiteFooter
        aboutText={<><em>spectra</em> is an HTTP header inspection service. Part of the <a href="https://netray.info"><strong>netray.info</strong></a> suite.</>}
        links={[
          { label: 'GitHub', href: 'https://github.com/netray-info/spectra' },
          { label: 'API Docs', href: '/docs' },
          { label: 'Author', href: 'https://lukas.pustina.net' },
        ]}
        version={meta()?.version}
      />
      </div>
  );
}
