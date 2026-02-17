import { createSignal, onMount, onCleanup, Show } from 'solid-js';
import { QueryInput } from './components/QueryInput';
import { ResultsTable, parseBatchEvent, type BatchEvent, type DoneStats } from './components/ResultsTable';

type Status = 'idle' | 'loading' | 'done' | 'error';
type ActiveTab = 'results' | 'servers' | 'json';
type Theme = 'dark' | 'light';

const HISTORY_KEY = 'prism_history';
const THEME_KEY = 'prism_theme';
const MAX_HISTORY = 50;

function loadHistory(): string[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  return [];
}

function saveHistory(history: string[]) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.slice(0, MAX_HISTORY)));
  } catch { /* ignore */ }
}

function getSystemTheme(): Theme {
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function getSavedTheme(): Theme | null {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved === 'light' || saved === 'dark') return saved;
  } catch { /* ignore */ }
  return null;
}

export default function App() {
  const [query, setQuery] = createSignal('');
  const [results, setResults] = createSignal<BatchEvent[]>([]);
  const [status, setStatus] = createSignal<Status>('idle');
  const [error, setError] = createSignal<string | null>(null);
  const [stats, setStats] = createSignal<DoneStats | null>(null);
  const [activeTab, setActiveTab] = createSignal<ActiveTab>('results');
  const [history, setHistory] = createSignal<string[]>(loadHistory());
  const [theme, setTheme] = createSignal<Theme>(getSavedTheme() ?? getSystemTheme());
  const [showHelp, setShowHelp] = createSignal(false);
  const [showTos, setShowTos] = createSignal(false);

  let eventSource: EventSource | null = null;
  let focusEditor: (() => void) | undefined;

  // ---------------------------------------------------------------------------
  // Theme
  // ---------------------------------------------------------------------------

  function applyTheme(t: Theme) {
    document.documentElement.setAttribute('data-theme', t);
  }

  function toggleTheme() {
    const next = theme() === 'dark' ? 'light' : 'dark';
    setTheme(next);
    applyTheme(next);
    try { localStorage.setItem(THEME_KEY, next); } catch { /* ignore */ }
  }

  // ---------------------------------------------------------------------------
  // SSE connection
  // ---------------------------------------------------------------------------

  function closeEventSource() {
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
  }

  // ---------------------------------------------------------------------------
  // History
  // ---------------------------------------------------------------------------

  function addToHistory(q: string) {
    setHistory((prev) => {
      const filtered = prev.filter((h) => h !== q);
      const updated = [q, ...filtered].slice(0, MAX_HISTORY);
      saveHistory(updated);
      return updated;
    });
  }

  // ---------------------------------------------------------------------------
  // Submit query
  // ---------------------------------------------------------------------------

  function submitQuery(q: string) {
    closeEventSource();

    setQuery(q);
    setResults([]);
    setError(null);
    setStats(null);
    setStatus('loading');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

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
      if (event instanceof MessageEvent && event.data) {
        try {
          const errorData = JSON.parse(event.data);
          setError(errorData.message ?? errorData.code ?? 'Unknown error');
        } catch {
          setError(event.data);
        }
      }
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

    es.onerror = () => {
      if (status() === 'done') return;
      if (es.readyState === EventSource.CLOSED) {
        if (!error()) setError('Connection closed unexpectedly');
        setStatus('error');
        closeEventSource();
      }
      if (es.readyState === EventSource.CONNECTING && error()) {
        setStatus('error');
        closeEventSource();
      }
    };
  }

  // ---------------------------------------------------------------------------
  // Keyboard shortcuts
  // ---------------------------------------------------------------------------

  function handleKeyDown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');

    // ? — toggle help (always works outside editor)
    if (e.key === '?' && !isEditing) {
      e.preventDefault();
      setShowHelp((v) => !v);
      return;
    }

    // Escape — dismiss modals or blur editor
    if (e.key === 'Escape') {
      if (showHelp()) {
        setShowHelp(false);
        e.preventDefault();
        return;
      }
      if (showTos()) {
        setShowTos(false);
        e.preventDefault();
        return;
      }
      if (isEditing) {
        const cmContent = (target.closest('.cm-editor') as HTMLElement | null)?.querySelector<HTMLElement>('.cm-content');
        cmContent?.blur();
        e.preventDefault();
        return;
      }
    }

    // / — focus query input
    if (e.key === '/' && !isEditing) {
      e.preventDefault();
      focusEditor?.();
      return;
    }
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  // Track system theme changes when no explicit user preference is saved.
  let mediaQuery: MediaQueryList | undefined;
  const onSystemThemeChange = () => {
    if (!getSavedTheme()) {
      const sys = getSystemTheme();
      setTheme(sys);
      applyTheme(sys);
    }
  };

  onMount(() => {
    applyTheme(theme());

    mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQuery.addEventListener('change', onSystemThemeChange);

    const params = new URLSearchParams(window.location.search);
    const q = params.get('q');
    if (q) submitQuery(q);

    document.addEventListener('keydown', handleKeyDown);
  });

  onCleanup(() => {
    closeEventSource();
    mediaQuery?.removeEventListener('change', onSystemThemeChange);
    document.removeEventListener('keydown', handleKeyDown);
  });

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div class="app">
      <header class="header">
        <h1 class="logo">prism</h1>
        <span class="tagline">DNS, refracted</span>
        <div class="header-actions">
          <button
            class="header-btn"
            onClick={toggleTheme}
            title={`Switch to ${theme() === 'dark' ? 'light' : 'dark'} mode`}
          >
            {theme() === 'dark' ? '\u2600' : '\u263E'}
          </button>
          <button
            class="header-btn"
            onClick={() => setShowHelp((v) => !v)}
            title="Keyboard shortcuts (?)"
          >
            ?
          </button>
        </div>
      </header>

      <main>
        <QueryInput
          onSubmit={submitQuery}
          initialValue={query()}
          history={history()}
          onReady={(api) => {
            focusEditor = api.focus;
          }}
        />

        <Show when={status() !== 'idle' || results().length > 0}>
          <div class="tabs">
            <div class="tabs-left">
              <button
                class={`tab ${activeTab() === 'results' ? 'active' : ''}`}
                onClick={() => setActiveTab('results')}
              >
                Results
              </button>
              <button
                class={`tab ${activeTab() === 'servers' ? 'active' : ''}`}
                onClick={() => setActiveTab('servers')}
              >
                Servers
              </button>
              <button
                class={`tab ${activeTab() === 'json' ? 'active' : ''}`}
                onClick={() => setActiveTab('json')}
              >
                JSON
              </button>
            </div>
            <Show when={status() === 'done' && stats()}>
              <div class="status-info">
                <span title="Total DNS queries sent across all servers and record types">
                  {stats()!.total_queries} queries
                </span>
                <span class="status-separator">/</span>
                <span title="Record type batches received from the server">
                  {results().length} batches
                </span>
                <span class="status-separator">/</span>
                <span title="Total wall-clock time for the query">
                  {stats()!.duration_ms}ms
                </span>
                <Show when={stats()!.transport && stats()!.transport !== 'udp'}>
                  <span class="status-separator">/</span>
                  <span class="status-badge transport-badge" title="DNS transport protocol used for this query">
                    {stats()!.transport!.toUpperCase()}
                  </span>
                </Show>
                <Show when={stats()!.dnssec}>
                  <span class="status-separator">/</span>
                  <span class="status-badge dnssec-badge" title="DNSSEC records (DNSKEY, DS) were included in this query">
                    DNSSEC
                  </span>
                </Show>
                <Show when={stats()!.warnings.length > 0}>
                  <span class="status-separator">/</span>
                  <span class="status-warnings" title={stats()!.warnings.join('; ')}>
                    {stats()!.warnings.length} warning{stats()!.warnings.length !== 1 ? 's' : ''}
                  </span>
                </Show>
              </div>
            </Show>
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

      <footer class="footer">
        <button class="footer-link" onClick={() => setShowTos(true)}>Terms of Service</button>
        <span class="footer-sep">&middot;</span>
        <span class="footer-text">Powered by mhost</span>
      </footer>

      {/* Help modal */}
      <Show when={showHelp()}>
        <div class="modal-overlay" onClick={() => setShowHelp(false)}>
          <div class="modal" onClick={(e) => e.stopPropagation()}>
            <div class="modal-header">
              <h2>Keyboard Shortcuts</h2>
              <button class="modal-close" onClick={() => setShowHelp(false)}>
                &times;
              </button>
            </div>
            <table class="shortcuts-table">
              <tbody>
                <tr>
                  <td class="shortcut-key">/</td>
                  <td>Focus query input</td>
                </tr>
                <tr>
                  <td class="shortcut-key">Enter</td>
                  <td>Submit query (when input focused)</td>
                </tr>
                <tr>
                  <td class="shortcut-key">Tab</td>
                  <td>Accept autocomplete suggestion</td>
                </tr>
                <tr>
                  <td class="shortcut-key">Escape</td>
                  <td>Dismiss autocomplete / blur input</td>
                </tr>
                <tr>
                  <td class="shortcut-key">j / k</td>
                  <td>Navigate result rows</td>
                </tr>
                <tr>
                  <td class="shortcut-key">Enter</td>
                  <td>Expand/collapse focused row</td>
                </tr>
                <tr>
                  <td class="shortcut-key">&uarr; / &darr;</td>
                  <td>Browse query history (in input)</td>
                </tr>
                <tr>
                  <td class="shortcut-key">?</td>
                  <td>Toggle this help</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </Show>

      {/* Terms of Service modal */}
      <Show when={showTos()}>
        <div class="modal-overlay" onClick={() => setShowTos(false)}>
          <div class="modal modal-wide" onClick={(e) => e.stopPropagation()}>
            <div class="modal-header">
              <h2>Terms of Service</h2>
              <button class="modal-close" onClick={() => setShowTos(false)}>
                &times;
              </button>
            </div>
            <div class="modal-body">
              <p>
                <strong>prism</strong> is a DNS debugging tool provided for diagnostic and
                educational purposes.
              </p>
              <ul>
                <li>
                  <strong>Fair use:</strong> Queries are rate-limited. Automated bulk querying,
                  scraping, or abuse may result in temporary or permanent blocks.
                </li>
                <li>
                  <strong>No warranty:</strong> DNS results are returned as-is from upstream
                  resolvers. This service makes no guarantee of accuracy, availability, or
                  completeness.
                </li>
                <li>
                  <strong>Privacy:</strong> Query domain names and client IP addresses may be
                  logged for rate-limiting and abuse prevention. Logs are rotated and not shared
                  with third parties. Full DNS response content is not logged.
                </li>
                <li>
                  <strong>No sensitive queries:</strong> Do not query domains that reveal personal
                  or sensitive information. All queries are sent to third-party public DNS
                  resolvers.
                </li>
              </ul>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}
