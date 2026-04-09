import { createSignal, Show, For } from 'solid-js';
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
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import { createTheme } from '@netray-info/common-frontend/theme';
import { inspect } from './lib/api';
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
  const themeResult = createTheme('spectra_theme', 'system');

  // Read URL param for initial query
  const params = new URLSearchParams(window.location.search);
  const initialUrl = params.get('url') ?? '';

  async function handleInspect(url: string) {
    setError(null);
    setResult(null);
    setLoading(true);

    try {
      const data = await inspect(url);
      setResult(data);

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
    <div>
      <SuiteNav />

      <div class="container">
        <div class="header">
          <div class="header__left">
            <div>
              <div class="header__title">spectra</div>
              <div class="header__tagline">HTTP headers, decoded</div>
            </div>
          </div>
          <div class="header__actions">
            <ThemeToggle />
          </div>
        </div>

        <UrlInput
          onSubmit={handleInspect}
          loading={loading()}
          initialValue={initialUrl}
        />

        <Show when={error()}>
          <div class="error-banner">{error()}</div>
        </Show>

        <Show when={loading()}>
          <div class="loading">
            <div class="loading__spinner" />
            <span>Inspecting...</span>
          </div>
        </Show>

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
      </div>

      <SiteFooter />
    </div>
  );
}
