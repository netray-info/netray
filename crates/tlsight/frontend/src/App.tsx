import { createSignal, Show } from 'solid-js';
import HostInput from './components/HostInput';
import ValidationSummary from './components/ValidationSummary';
import ChainView from './components/ChainView';
import CertDetail from './components/CertDetail';
import TlsParams from './components/TlsParams';
import { inspect } from './lib/api';
import type { InspectResponse } from './lib/types';

export default function App() {
  const [result, setResult] = createSignal<InspectResponse | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [theme, setTheme] = createSignal<'light' | 'dark'>(
    window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  );

  const toggleTheme = () => setTheme(t => t === 'dark' ? 'light' : 'dark');

  const handleInspect = async (input: string) => {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const data = await inspect(input);
      setResult(data);
      // Update URL for shareability
      const url = new URL(window.location.href);
      url.searchParams.set('h', input);
      window.history.replaceState(null, '', url.toString());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  // Check URL for initial query
  const params = new URLSearchParams(window.location.search);
  const initialQuery = params.get('h');
  if (initialQuery) {
    handleInspect(initialQuery);
  }

  return (
    <div class="app" data-theme={theme()}>
      <header class="header">
        <h1 class="title">tlsight</h1>
        <button class="theme-toggle" onClick={toggleTheme} title="Toggle theme">
          {theme() === 'dark' ? 'Light' : 'Dark'}
        </button>
      </header>

      <main class="main">
        <HostInput onSubmit={handleInspect} loading={loading()} />

        <Show when={error()}>
          <div class="error-banner">{error()}</div>
        </Show>

        <Show when={result()}>
          {(res) => {
            const r = res();
            const firstPort = r.ports[0];
            const firstIp = firstPort?.ips[0];
            return (
              <div class="results">
                <ValidationSummary summary={r.summary} />
                <Show when={firstIp?.chain}>
                  {(chain) => <ChainView chain={chain()} />}
                </Show>
                <Show when={firstIp?.chain}>
                  {(chain) => (
                    <div class="cert-details">
                      {chain().map(cert => <CertDetail cert={cert} />)}
                    </div>
                  )}
                </Show>
                <Show when={firstIp?.tls}>
                  {(tls) => <TlsParams params={tls()} />}
                </Show>
                <div class="footer-info">
                  Inspected {firstPort?.ips.length ?? 0} IP(s) on port {firstPort?.port ?? 443} in {r.duration_ms}ms
                </div>
              </div>
            );
          }}
        </Show>
      </main>
    </div>
  );
}
