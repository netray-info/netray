import { createSignal, createResource, Show, For, ErrorBoundary } from 'solid-js';
import HostInput from './components/HostInput';
import ValidationSummary from './components/ValidationSummary';
import ChainView from './components/ChainView';
import CertDetail from './components/CertDetail';
import TlsParams from './components/TlsParams';
import PortTabs from './components/PortTabs';
import ConsistencyView from './components/ConsistencyView';
import CrossLinks from './components/CrossLinks';
import { inspect, fetchMeta } from './lib/api';
import type { InspectResponse, PortResult } from './lib/types';

export default function App() {
  const [result, setResult] = createSignal<InspectResponse | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [selectedPort, setSelectedPort] = createSignal<number>(443);
  const [theme, setTheme] = createSignal<'light' | 'dark'>(
    window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  );

  const [meta] = createResource(fetchMeta);

  const toggleTheme = () => setTheme(t => t === 'dark' ? 'light' : 'dark');

  const handleInspect = async (input: string) => {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const data = await inspect(input);
      setResult(data);
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
            return (
              <div class="results">
                <ValidationSummary summary={r.summary} />

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

                <Show when={currentPort()}>
                  {(port) => {
                    const p = port();
                    return (
                      <>
                        <Show when={p.consistency}>
                          {(c) => <ConsistencyView consistency={c()} />}
                        </Show>

                        <For each={p.ips}>
                          {(ipResult) => (
                            <div class="ip-section">
                              <div class="ip-section__header">
                                <span class="ip-section__ip mono">{ipResult.ip}</span>
                                <span class="ip-section__version">{ipResult.ip_version}</span>
                                <Show when={ipResult.error}>
                                  {(err) => <span class="ip-section__error">{err().message}</span>}
                                </Show>
                              </div>

                              <Show when={ipResult.tls}>
                                {(tls) => <TlsParams params={tls()} />}
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

                <div class="footer-info">
                  Inspected {allIps().length} IP(s) across {r.ports.length} port(s) in {r.duration_ms}ms
                  <Show when={(r.skipped_ips ?? []).length > 0}>
                    {' '}| {r.skipped_ips?.length} IP(s) skipped (rate limit)
                  </Show>
                </div>
              </div>
            );
          }}
        </Show>
      </main>
    </div>
  );
}
