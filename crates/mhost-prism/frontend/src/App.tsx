import { createSignal, onMount, onCleanup, Show } from 'solid-js';
import { QueryInput } from './components/QueryInput';
import { ResultsTable, parseBatchEvent, type BatchEvent, type DoneStats } from './components/ResultsTable';

type Status = 'idle' | 'loading' | 'done' | 'error';
type ActiveTab = 'results' | 'json';

export default function App() {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<BatchEvent[]>([]);
  const [status, setStatus] = createSignal<Status>('idle');
  const [error, setError] = createSignal<string | null>(null);
  const [stats, setStats] = createSignal<DoneStats | null>(null);
  const [activeTab, setActiveTab] = createSignal<ActiveTab>('results');

  let eventSource: EventSource | null = null;

  /** Close any active SSE connection. */
  function closeEventSource() {
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
  }

  /** Submit a query: connect to SSE and stream results. */
  function submitQuery(q: string) {
    // Close previous connection
    closeEventSource();

    // Reset state
    setQuery(q);
    setResults([]);
    setError(null);
    setStats(null);
    setStatus('loading');

    // Update URL
    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    // Open SSE connection
    const sseUrl = `/api/query?q=${encodeURIComponent(q)}`;
    const es = new EventSource(sseUrl);
    eventSource = es;

    es.addEventListener('batch', (event) => {
      try {
        const batch = parseBatchEvent(JSON.parse(event.data));
        setResults((prev) => [...prev, batch]);
      } catch (e) {
        console.error('Failed to parse batch event:', e);
      }
    });

    es.addEventListener('error', (event) => {
      // SSE error event — could be a connection error or a server-sent error
      if (event instanceof MessageEvent && event.data) {
        try {
          const errorData = JSON.parse(event.data);
          setError(errorData.message ?? errorData.code ?? 'Unknown error');
        } catch {
          setError(event.data);
        }
      }
      // Note: we do NOT close here or set status to error yet.
      // The 'done' event is always sent after errors, so we wait for it.
      // If the connection itself drops, the onerror handler below fires.
    });

    es.addEventListener('done', (event) => {
      try {
        const doneData: DoneStats = JSON.parse(event.data);
        setStats(doneData);
      } catch (e) {
        console.error('Failed to parse done event:', e);
      }
      setStatus('done');
      closeEventSource();
    });

    // EventSource onerror fires on connection failure or when the server closes
    es.onerror = () => {
      // If we already received 'done', this fires because we closed — ignore.
      if (status() === 'done') return;

      // If readyState is CLOSED, the server closed the connection unexpectedly
      if (es.readyState === EventSource.CLOSED) {
        if (!error()) {
          setError('Connection closed unexpectedly');
        }
        setStatus('error');
        closeEventSource();
      }
      // If readyState is CONNECTING, EventSource is auto-reconnecting.
      // We let it retry a few times, but if we already have an error, give up.
      if (es.readyState === EventSource.CONNECTING && error()) {
        setStatus('error');
        closeEventSource();
      }
    };
  }

  // On mount: check URL for initial query
  onMount(() => {
    const params = new URLSearchParams(window.location.search);
    const q = params.get('q');
    if (q) {
      submitQuery(q);
    }
  });

  // Cleanup on unmount
  onCleanup(() => {
    closeEventSource();
  });

  return (
    <div class="app">
      <header class="header">
        <h1 class="logo">prism</h1>
        <span class="tagline">DNS, refracted</span>
      </header>

      <main>
        <QueryInput onSubmit={submitQuery} initialValue={query()} />

        <Show when={status() !== 'idle' || results().length > 0}>
          <div class="tabs">
            <button
              class={`tab ${activeTab() === 'results' ? 'active' : ''}`}
              onClick={() => setActiveTab('results')}
            >
              Results
            </button>
            <button
              class={`tab ${activeTab() === 'json' ? 'active' : ''}`}
              onClick={() => setActiveTab('json')}
            >
              JSON
            </button>
          </div>
        </Show>

        <ResultsTable
          results={results()}
          stats={stats()}
          status={status()}
          error={error()}
          activeTab={activeTab()}
        />
      </main>
    </div>
  );
}
