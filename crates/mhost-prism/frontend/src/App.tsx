import { createSignal, onMount, onCleanup, Show } from 'solid-js';
import { QueryInput } from './components/QueryInput';
import { ResultsTable, parseBatchEvent, type BatchEvent, type DoneStats } from './components/ResultsTable';
import { LintTab, type LintCategory, type CheckDoneStats } from './components/LintTab';
import { TraceView, type TraceHop, type TraceDoneStats } from './components/TraceView';

type Status = 'idle' | 'loading' | 'done' | 'error';
type ActiveTab = 'trace' | 'lint' | 'results' | 'servers' | 'json';
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

// ---------------------------------------------------------------------------
// Check / trace mode helpers
// ---------------------------------------------------------------------------

/** Returns true if the query string contains the +check flag. */
function hasCheckFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+check');
}

/** Returns true if the query string contains the +trace flag. */
function hasTraceFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+trace');
}

/** Extract domain (first token) and @server specs from a query string. */
function extractCheckParams(q: string): { domain: string; servers: string[] } {
  const tokens = q.trim().split(/\s+/);
  const domain = tokens[0] ?? '';
  const servers = tokens
    .slice(1)
    .filter((t) => t.startsWith('@'))
    .map((t) => t.slice(1));
  return { domain, servers };
}

/** Strip the +trace flag, returning the base query for a regular DNS lookup. */
function stripTraceFlag(q: string): string {
  return q.trim().split(/\s+/).filter((t) => t.toLowerCase() !== '+trace').join(' ');
}

/** Extract domain (first token) and record_type from a query string for trace. */
function extractTraceParams(q: string): { domain: string; record_type: string } {
  const tokens = q.trim().split(/\s+/);
  const domain = tokens[0] ?? '';
  // Pick the first token that looks like a record type (uppercase letters, e.g. MX, AAAA)
  const recordTypeToken = tokens.slice(1).find((t) => /^[A-Za-z0-9]+$/.test(t) && !t.startsWith('@') && !t.startsWith('+'));
  const record_type = recordTypeToken?.toUpperCase() ?? 'A';
  return { domain, record_type };
}

// ---------------------------------------------------------------------------
// Fetch-based SSE parser for POST endpoints
// ---------------------------------------------------------------------------

type SSEEventHandler = (eventType: string, data: unknown) => void;

async function readPostStream(
  response: Response,
  onEvent: SSEEventHandler,
  signal: AbortSignal,
): Promise<void> {
  const reader = response.body!.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done || signal.aborted) break;

      buffer += decoder.decode(value, { stream: true });

      // SSE events are separated by double newlines.
      const events = buffer.split(/\r?\n\r?\n/);
      // The last element is either empty or an incomplete event — keep it.
      buffer = events.pop() ?? '';

      for (const eventText of events) {
        const lines = eventText.split(/\r?\n/);
        let eventType = 'message';
        let dataLines: string[] = [];

        for (const line of lines) {
          if (line.startsWith('event:')) {
            eventType = line.slice(6).trim();
          } else if (line.startsWith('data:')) {
            dataLines.push(line.slice(5).trim());
          }
        }

        if (dataLines.length === 0) continue;
        const raw = dataLines.join('\n');

        try {
          const parsed = JSON.parse(raw);
          onEvent(eventType, parsed);
        } catch (e) {
          console.error('Failed to parse SSE event data:', e, raw);
        }
      }
    }
  } finally {
    reader.cancel().catch(() => { /* ignore */ });
  }
}

// ---------------------------------------------------------------------------
// App component
// ---------------------------------------------------------------------------

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

  // Check mode state
  const [isCheckMode, setIsCheckMode] = createSignal(false);
  const [lintCategories, setLintCategories] = createSignal<LintCategory[]>([]);
  const [checkStats, setCheckStats] = createSignal<CheckDoneStats | null>(null);

  // Trace mode state
  const [isTraceMode, setIsTraceMode] = createSignal(false);
  const [traceHops, setTraceHops] = createSignal<TraceHop[]>([]);
  const [traceDoneStats, setTraceDoneStats] = createSignal<TraceDoneStats | null>(null);

  let eventSource: EventSource | null = null;
  let checkAbortController: AbortController | null = null;
  let traceAbortController: AbortController | null = null;
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
  // Connection teardown
  // ---------------------------------------------------------------------------

  function closeEventSource() {
    if (eventSource) {
      eventSource.close();
      eventSource = null;
    }
  }

  function abortCheck() {
    if (checkAbortController) {
      checkAbortController.abort();
      checkAbortController = null;
    }
  }

  function abortTrace() {
    if (traceAbortController) {
      traceAbortController.abort();
      traceAbortController = null;
    }
  }

  function closeConnections() {
    closeEventSource();
    abortCheck();
    abortTrace();
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
  // Submit — check mode (POST /api/check)
  // ---------------------------------------------------------------------------

  async function submitCheck(q: string) {
    closeConnections();

    const { domain, servers } = extractCheckParams(q);
    if (!domain) return;

    setQuery(q);
    setResults([]);
    setStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setError(null);
    setIsCheckMode(true);
    setStatus('loading');
    setActiveTab('lint');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    const controller = new AbortController();
    checkAbortController = controller;

    let response: Response;
    try {
      response = await fetch('/api/check', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Accept': 'text/event-stream',
        },
        body: JSON.stringify({ domain, servers }),
        signal: controller.signal,
      });
    } catch (e: unknown) {
      if (e instanceof Error && e.name === 'AbortError') return;
      setError(e instanceof Error ? e.message : 'Network error');
      setStatus('error');
      return;
    }

    if (!response.ok) {
      try {
        const body = await response.json();
        setError(body?.error?.message ?? `HTTP ${response.status}`);
      } catch {
        setError(`HTTP ${response.status}`);
      }
      setStatus('error');
      return;
    }

    await readPostStream(
      response,
      (eventType, data) => {
        if (eventType === 'batch') {
          try {
            const batch = parseBatchEvent(data as Parameters<typeof parseBatchEvent>[0]);
            setResults((prev) => [...prev, batch]);
          } catch (e) {
            console.error('Failed to parse batch event:', e);
          }
        } else if (eventType === 'lint') {
          try {
            const ev = data as { category: string; results: LintCategory['results'] };
            setLintCategories((prev) => [...prev, { category: ev.category, results: ev.results }]);
          } catch (e) {
            console.error('Failed to parse lint event:', e);
          }
        } else if (eventType === 'done') {
          setCheckStats(data as CheckDoneStats);
          setStatus('done');
        } else if (eventType === 'error') {
          const ev = data as { message?: string; code?: string };
          setError(ev.message ?? ev.code ?? 'Unknown error');
        }
      },
      controller.signal,
    );
  }

  // ---------------------------------------------------------------------------
  // Background query — populates Results/Servers/JSON tabs during a trace
  // ---------------------------------------------------------------------------

  function startBackgroundQuery(q: string) {
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

    es.addEventListener('done', (event) => {
      try {
        setStats(JSON.parse(event.data) as DoneStats);
      } catch (e) {
        console.error('Failed to parse done event:', e);
      }
      closeEventSource();
    });

    es.onerror = () => {
      if (es.readyState === EventSource.CLOSED) closeEventSource();
    };
  }

  // ---------------------------------------------------------------------------
  // Submit — trace mode (POST /api/trace)
  // ---------------------------------------------------------------------------

  async function submitTrace(q: string) {
    closeConnections();

    const { domain, record_type } = extractTraceParams(q);
    if (!domain) return;

    setQuery(q);
    setTraceHops([]);
    setTraceDoneStats(null);
    setError(null);
    setIsTraceMode(true);
    setIsCheckMode(false);
    setResults([]);
    setStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setStatus('loading');
    setActiveTab('trace');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    // Run the base query (without +trace) in the background to populate Results.
    startBackgroundQuery(stripTraceFlag(q));

    const controller = new AbortController();
    traceAbortController = controller;

    let response: Response;
    try {
      response = await fetch('/api/trace', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Accept': 'text/event-stream',
        },
        body: JSON.stringify({ domain, record_type }),
        signal: controller.signal,
      });
    } catch (e: unknown) {
      if (e instanceof Error && e.name === 'AbortError') return;
      setError(e instanceof Error ? e.message : 'Network error');
      setStatus('error');
      return;
    }

    if (!response.ok) {
      try {
        const body = await response.json();
        setError(body?.error?.message ?? `HTTP ${response.status}`);
      } catch {
        setError(`HTTP ${response.status}`);
      }
      setStatus('error');
      return;
    }

    await readPostStream(
      response,
      (eventType, data) => {
        if (eventType === 'hop') {
          try {
            const ev = data as { request_id: string; hop: TraceHop };
            setTraceHops((prev) => [...prev, ev.hop]);
          } catch (e) {
            console.error('Failed to parse hop event:', e);
          }
        } else if (eventType === 'done') {
          setTraceDoneStats(data as TraceDoneStats);
          setStatus('done');
        } else if (eventType === 'error') {
          const ev = data as { message?: string; code?: string };
          setError(ev.message ?? ev.code ?? 'Unknown error');
        }
      },
      controller.signal,
    );
  }

  // ---------------------------------------------------------------------------
  // Submit — combined trace + check mode
  // ---------------------------------------------------------------------------

  function submitTraceAndCheck(q: string) {
    closeConnections();

    const { domain, record_type } = extractTraceParams(q);
    const { domain: checkDomain, servers } = extractCheckParams(q);
    if (!domain) return;

    setQuery(q);
    setTraceHops([]);
    setTraceDoneStats(null);
    setResults([]);
    setStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setError(null);
    setIsTraceMode(true);
    setIsCheckMode(true);
    setStatus('loading');
    setActiveTab('trace');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    let doneCount = 0;
    function onStreamDone() {
      doneCount++;
      if (doneCount >= 2) setStatus('done');
    }

    const checkController = new AbortController();
    checkAbortController = checkController;

    const traceController = new AbortController();
    traceAbortController = traceController;

    // Check stream (provides batch + lint events)
    (async () => {
      let response: Response;
      try {
        response = await fetch('/api/check', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
          body: JSON.stringify({ domain: checkDomain, servers }),
          signal: checkController.signal,
        });
      } catch (e) {
        if (e instanceof Error && e.name === 'AbortError') return;
        setError(e instanceof Error ? e.message : 'Network error');
        onStreamDone();
        return;
      }
      if (!response.ok) {
        try { const body = await response.json(); setError(body?.error?.message ?? `HTTP ${response.status}`); }
        catch { setError(`HTTP ${response.status}`); }
        onStreamDone();
        return;
      }
      await readPostStream(response, (eventType, data) => {
        if (eventType === 'batch') {
          try { setResults((prev) => [...prev, parseBatchEvent(data as Parameters<typeof parseBatchEvent>[0])]); }
          catch (e) { console.error('Failed to parse batch event:', e); }
        } else if (eventType === 'lint') {
          try {
            const ev = data as { category: string; results: LintCategory['results'] };
            setLintCategories((prev) => [...prev, { category: ev.category, results: ev.results }]);
          } catch (e) { console.error('Failed to parse lint event:', e); }
        } else if (eventType === 'done') {
          setCheckStats(data as CheckDoneStats);
          onStreamDone();
        } else if (eventType === 'error') {
          const ev = data as { message?: string; code?: string };
          setError(ev.message ?? ev.code ?? 'Unknown error');
        }
      }, checkController.signal);
    })();

    // Trace stream (provides hop events)
    (async () => {
      let response: Response;
      try {
        response = await fetch('/api/trace', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
          body: JSON.stringify({ domain, record_type }),
          signal: traceController.signal,
        });
      } catch (e) {
        if (e instanceof Error && e.name === 'AbortError') return;
        setError(e instanceof Error ? e.message : 'Network error');
        onStreamDone();
        return;
      }
      if (!response.ok) {
        try { const body = await response.json(); setError(body?.error?.message ?? `HTTP ${response.status}`); }
        catch { setError(`HTTP ${response.status}`); }
        onStreamDone();
        return;
      }
      await readPostStream(response, (eventType, data) => {
        if (eventType === 'hop') {
          try {
            const ev = data as { request_id: string; hop: TraceHop };
            setTraceHops((prev) => [...prev, ev.hop]);
          } catch (e) { console.error('Failed to parse hop event:', e); }
        } else if (eventType === 'done') {
          setTraceDoneStats(data as TraceDoneStats);
          onStreamDone();
        } else if (eventType === 'error') {
          const ev = data as { message?: string; code?: string };
          setError(ev.message ?? ev.code ?? 'Unknown error');
        }
      }, traceController.signal);
    })();
  }

  // ---------------------------------------------------------------------------
  // Submit — query mode (GET /api/query)
  // ---------------------------------------------------------------------------

  function submitQuery(q: string) {
    if (hasCheckFlag(q) && hasTraceFlag(q)) {
      submitTraceAndCheck(q);
      return;
    }
    if (hasCheckFlag(q)) {
      submitCheck(q);
      return;
    }
    if (hasTraceFlag(q)) {
      submitTrace(q);
      return;
    }

    closeConnections();

    setQuery(q);
    setResults([]);
    setError(null);
    setStats(null);
    setIsCheckMode(false);
    setIsTraceMode(false);
    setTraceHops([]);
    setTraceDoneStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setStatus('loading');
    setActiveTab('results');
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

    if (e.key === '?' && !isEditing) {
      e.preventDefault();
      setShowHelp((v) => !v);
      return;
    }

    if (e.key === 'Escape') {
      if (showHelp()) { setShowHelp(false); e.preventDefault(); return; }
      if (showTos())  { setShowTos(false);  e.preventDefault(); return; }
      if (isEditing) {
        const cmContent = (target.closest('.cm-editor') as HTMLElement | null)?.querySelector<HTMLElement>('.cm-content');
        cmContent?.blur();
        e.preventDefault();
        return;
      }
    }

    if (e.key === '/' && !isEditing) {
      e.preventDefault();
      focusEditor?.();
      return;
    }

    if (e.key === 'r' && !isEditing) {
      const q = query();
      if (q) { e.preventDefault(); submitQuery(q); }
      return;
    }
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

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
    closeConnections();
    mediaQuery?.removeEventListener('change', onSystemThemeChange);
    document.removeEventListener('keydown', handleKeyDown);
  });

  // ---------------------------------------------------------------------------
  // Derived display state
  // ---------------------------------------------------------------------------

  const hasContent = () =>
    status() !== 'idle' || results().length > 0 || lintCategories().length > 0 || traceHops().length > 0;
  const isLoading  = () => status() === 'loading';

  // Lint tab badge text: show worst status count when done.
  function lintTabBadge(): string {
    const s = checkStats();
    if (!s) return '';
    if (s.failed > 0)   return ` ✗${s.failed}`;
    if (s.warnings > 0) return ` ⚠${s.warnings}`;
    return ' ✓';
  }

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
          onReady={(api) => { focusEditor = api.focus; }}
        />

        <Show when={hasContent()}>
          <div class="tabs">
            <div class="tabs-left">
              {/* Trace tab — only visible in trace mode */}
              <Show when={isTraceMode()}>
                <button
                  class={`tab${activeTab() === 'trace' ? ' active' : ''}`}
                  onClick={() => setActiveTab('trace')}
                >
                  Trace
                </button>
              </Show>
              {/* Lint tab — only visible in check mode */}
              <Show when={isCheckMode()}>
                <button
                  class={`tab${activeTab() === 'lint' ? ' active' : ''}${
                    checkStats()?.failed ? ' tab--failed' : checkStats()?.warnings ? ' tab--warning' : ''
                  }`}
                  onClick={() => setActiveTab('lint')}
                >
                  Lint{lintTabBadge()}
                </button>
              </Show>
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

            {/* Status bar — query mode */}
            <Show when={!isCheckMode() && !isTraceMode() && status() === 'done' && stats()}>
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
                  <span class="status-badge transport-badge" title="DNS transport protocol">
                    {stats()!.transport!.toUpperCase()}
                  </span>
                </Show>
                <Show when={stats()!.dnssec}>
                  <span class="status-separator">/</span>
                  <span class="status-badge dnssec-badge" title="DNSSEC records were included">
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

            {/* Status bar — loading (check / trace / both) */}
            <Show when={(isCheckMode() || isTraceMode()) && isLoading()}>
              <div class="status-info">
                <span class="status-loading-text">
                  {isCheckMode() && isTraceMode() ? 'Tracing + Checking…'
                    : isCheckMode() ? 'Checking…'
                    : 'Tracing…'}
                </span>
              </div>
            </Show>

            {/* Status bar — trace mode (done) */}
            <Show when={isTraceMode() && status() === 'done' && traceDoneStats()}>
              <div class="status-info">
                <span>{traceDoneStats()!.hops} hop{traceDoneStats()!.hops !== 1 ? 's' : ''}</span>
                <span class="status-separator">/</span>
                <span>{traceDoneStats()!.duration_ms}ms</span>
              </div>
            </Show>
          </div>
        </Show>

        {/* Trace tab content */}
        <Show when={isTraceMode() && activeTab() === 'trace'}>
          <TraceView
            hops={traceHops()}
            doneStats={traceDoneStats()}
            isLoading={isLoading()}
            activeTab={activeTab()}
          />
        </Show>

        {/* Lint tab content */}
        <Show when={isCheckMode() && activeTab() === 'lint'}>
          <LintTab
            categories={lintCategories()}
            doneStats={checkStats()}
            isLoading={isLoading()}
          />
        </Show>

        {/* Results / Servers / JSON — always rendered via ResultsTable */}
        <Show when={activeTab() !== 'lint' && activeTab() !== 'trace'}>
          <ResultsTable
            results={results()}
            stats={stats()}
            status={status()}
            error={error()}
            activeTab={activeTab() as 'results' | 'servers' | 'json'}
          />
        </Show>
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
              <button class="modal-close" onClick={() => setShowHelp(false)}>&times;</button>
            </div>
            <table class="shortcuts-table">
              <tbody>
                <tr><td class="shortcut-key">/</td><td>Focus query input</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Submit query (when input focused)</td></tr>
                <tr><td class="shortcut-key">Tab</td><td>Accept autocomplete suggestion</td></tr>
                <tr><td class="shortcut-key">Escape</td><td>Dismiss autocomplete / blur input</td></tr>
                <tr><td class="shortcut-key">j / k</td><td>Navigate result rows</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Expand/collapse focused row</td></tr>
                <tr><td class="shortcut-key">&uarr; / &darr;</td><td>Browse query history (in input)</td></tr>
                <tr><td class="shortcut-key">r</td><td>Re-run current query</td></tr>
                <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
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
              <button class="modal-close" onClick={() => setShowTos(false)}>&times;</button>
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
