import { createSignal, onCleanup, Show } from 'solid-js';
import { createTheme } from '@netray-info/common-frontend/theme';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import DomainInput from './components/DomainInput';
import DnsSection from './components/DnsSection';
import TlsSection from './components/TlsSection';
import IpSection from './components/IpSection';
import Summary from './components/Summary';
import { startCheck } from './lib/sse';
import type { CheckState, DnsEvent, TlsEvent, IpEvent, SummaryEvent, DoneEvent } from './lib/types';

export default function App() {
  const themeResult = createTheme('lens_theme', 'system');

  const [checkState, setCheckState] = createSignal<CheckState>('idle');
  const [dns, setDns] = createSignal<DnsEvent | null>(null);
  const [tls, setTls] = createSignal<TlsEvent | null>(null);
  const [ip, setIp] = createSignal<IpEvent | null>(null);
  const [summary, setSummary] = createSignal<SummaryEvent | null>(null);
  const [done, setDone] = createSignal<DoneEvent | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [currentDomain, setCurrentDomain] = createSignal('');

  let cleanup: (() => void) | null = null;

  function clearState() {
    setDns(null);
    setTls(null);
    setIp(null);
    setSummary(null);
    setDone(null);
    setError(null);
  }

  function handleSubmit(domain: string) {
    if (cleanup) { cleanup(); cleanup = null; }
    clearState();
    setCurrentDomain(domain);
    setCheckState('loading');

    const url = new URL(window.location.href);
    url.searchParams.set('d', domain);
    window.history.replaceState(null, '', url.toString());

    cleanup = startCheck(domain, {
      onDns: (data) => setDns(data),
      onTls: (data) => setTls(data),
      onIp: (data) => setIp(data),
      onSummary: (data) => setSummary(data),
      onDone: (data) => { setDone(data); setCheckState('done'); },
      onError: (err) => { setError(err); setCheckState('error'); },
    });
  }

  onCleanup(() => { if (cleanup) cleanup(); });

  const isLoading = () => checkState() === 'loading';
  const hasResults = () => dns() !== null || tls() !== null || ip() !== null;

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
        <nav class="suite-nav" aria-label="Suite navigation">
          <a class="suite-nav__brand" href="https://netray.info">netray.info</a>
          <span class="suite-nav__sep">/</span>
          <a class="suite-nav__link suite-nav__link--current" href="/" aria-current="page">lens</a>
          <span class="suite-nav__sep">·</span>
          <a class="suite-nav__link" href="https://dns.netray.info">dns</a>
          <span class="suite-nav__sep">·</span>
          <a class="suite-nav__link" href="https://tls.netray.info">tls</a>
          <span class="suite-nav__sep">·</span>
          <a class="suite-nav__link" href="https://ip.netray.info">ip</a>
        </nav>

        <header class="header">
          <h1 class="logo">lens</h1>
          <span class="tagline">Domain health at a glance</span>
          <div class="header-actions">
            <ThemeToggle theme={themeResult} class="header-btn" />
          </div>
        </header>

        <main class="main" id="main-content">
          <DomainInput onSubmit={handleSubmit} loading={isLoading()} />

          <Show when={error()}>
            <div class="error-banner" role="alert">{error()}</div>
          </Show>

          <Show when={!hasResults() && !isLoading() && !error()}>
            <div class="welcome">
              <p class="welcome-tagline">
                DNS health, TLS certificate status, and IP reputation — checked together, streamed as they arrive.
              </p>
            </div>
          </Show>

          <Show when={hasResults() || isLoading()}>
            <div class="section-grid">
              <DnsSection
                data={dns()}
                loading={isLoading() && dns() === null}
                error={error() ?? undefined}
              />
              <TlsSection
                data={tls()}
                loading={isLoading() && tls() === null}
                error={error() ?? undefined}
              />
              <IpSection
                data={ip()}
                loading={isLoading() && ip() === null}
                error={error() ?? undefined}
              />
            </div>
          </Show>

          <Show when={summary()}>
            {(s) => <Summary summary={s()} done={done()} />}
          </Show>
        </main>

        <SiteFooter
          aboutText={
            <>
              <em>lens</em> checks DNS health, TLS certificate validity, and IP reputation for any domain — results stream in as each check completes.
              Part of the <a href="https://netray.info">netray.info</a> suite.
            </>
          }
          links={[
            { href: '/docs', label: 'API Docs', external: true },
            { href: 'https://netray.info', label: 'Suite', external: true },
          ]}
        />
      </div>
    </>
  );
}
