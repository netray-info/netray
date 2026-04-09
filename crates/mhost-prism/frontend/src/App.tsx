import { createSignal, createEffect, onMount, onCleanup, Show } from 'solid-js';
import { QueryInput } from './components/QueryInput';
import { ResultsTable, parseBatchEvent, groupByRecordType, lookupsAgree, hasDeviation, type BatchEvent, type DoneStats } from './components/ResultsTable';
import { LintTab, type LintCategory, type CheckDoneStats } from './components/LintTab';
import { TraceView, type TraceHop, type TraceDoneStats } from './components/TraceView';
import { DnssecView, type ChainLevel, type DnssecDoneStats } from './components/DnssecView';
import { TransportComparison } from './components/TransportComparison';
import { AuthComparison } from './components/AuthComparison';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import { DnsCrossLinks } from './components/DnsCrossLinks';
import { toMarkdown, toCsv, toJson, downloadFile, copyToClipboard, type MarkdownContext } from './lib/export';
import { createTheme } from '@netray-info/common-frontend/theme';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import { storageGet, storageSet } from '@netray-info/common-frontend/storage';
import Modal from '@netray-info/common-frontend/components/Modal';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';

type Status = 'idle' | 'loading' | 'done' | 'error';
type ActiveTab = 'dnssec' | 'trace' | 'lint' | 'results' | 'servers' | 'transport' | 'auth';

export interface CloudInfo {
  provider?: string;
  region?: string;
  service?: string;
}

export interface IpInfo {
  asn?: number;
  org?: string;
  ip_type?: string;   // "type" in JSON, renamed by serde
  cloud?: CloudInfo;
  is_tor?: boolean;
  is_vpn?: boolean;
  is_datacenter?: boolean;
  is_spamhaus?: boolean;
  is_c2?: boolean;
}

const HISTORY_KEY = 'prism_history';
const VIEW_PREFS_KEY = 'prism_view_prefs';
const MAX_HISTORY = 50;

interface ViewPrefs { hideNx: boolean; compact: boolean; devOnly: boolean; sort: boolean; explain: boolean; }

function loadViewPrefs(): ViewPrefs {
  const p = storageGet<Partial<ViewPrefs>>(VIEW_PREFS_KEY, {});
  return { hideNx: Boolean(p.hideNx ?? true), compact: Boolean(p.compact ?? true), devOnly: Boolean(p.devOnly ?? false), sort: Boolean(p.sort ?? true), explain: Boolean(p.explain ?? false) };
}

function saveViewPrefs(prefs: ViewPrefs) {
  storageSet(VIEW_PREFS_KEY, prefs);
}

function loadHistory(): string[] {
  return storageGet<string[]>(HISTORY_KEY, []);
}

function saveHistory(history: string[]) {
  storageSet(HISTORY_KEY, history.slice(0, MAX_HISTORY));
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

/** Returns true if the query string contains the +dnssec flag. */
function hasDnssecFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+dnssec');
}

/** Returns true if the query string contains the +compare flag. */
function hasCompareFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+compare');
}

/** Returns true if the query string contains the +auth flag. */
function hasAuthFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+auth');
}

/** Returns true if the query string contains the +short flag. */
function hasShortFlag(q: string): boolean {
  return q.trim().toLowerCase().split(/\s+/).some((t) => t === '+short');
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

/** Strip routing flags (+dnssec, +trace, +check, +compare, +auth) for background query. */
function stripRoutingFlags(q: string): string {
  return q.trim().split(/\s+/).filter((t) => {
    const lower = t.toLowerCase();
    return lower !== '+dnssec' && lower !== '+trace' && lower !== '+check' && lower !== '+compare' && lower !== '+auth';
  }).join(' ');
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
  const themeResult = createTheme('prism_theme', 'system');
  const [showHelp, setShowHelp] = createSignal(false);


  // View options
  const vp = loadViewPrefs();
  const [hideNx, setHideNx] = createSignal(vp.hideNx);
  const [compact, setCompact] = createSignal(vp.compact);
  const [devOnly, setDevOnly] = createSignal(vp.devOnly);
  const [sortView, setSortView] = createSignal(vp.sort);
  const [explain, setExplain] = createSignal(vp.explain);

  function currentViewPrefs(): ViewPrefs {
    return { hideNx: hideNx(), compact: compact(), devOnly: devOnly(), sort: sortView(), explain: explain() };
  }
  function toggleHideNx() { const n = !hideNx(); setHideNx(n); saveViewPrefs({ ...currentViewPrefs(), hideNx: n }); }
  function toggleCompact() { const n = !compact(); setCompact(n); saveViewPrefs({ ...currentViewPrefs(), compact: n }); }
  function toggleDevOnly() { const n = !devOnly(); setDevOnly(n); saveViewPrefs({ ...currentViewPrefs(), devOnly: n }); }
  function toggleSort()    { const n = !sortView(); setSortView(n); saveViewPrefs({ ...currentViewPrefs(), sort: n }); }
  function toggleExplain() { const n = !explain(); setExplain(n); saveViewPrefs({ ...currentViewPrefs(), explain: n }); }

  // Expand/collapse all triggers (increment to trigger effect)
  const [expandAllTrigger, setExpandAllTrigger] = createSignal(0);
  const [collapseAllTrigger, setCollapseAllTrigger] = createSignal(0);
  const [allExpanded, setAllExpanded] = createSignal(false);
  function toggleExpandAll() {
    if (allExpanded()) {
      setCollapseAllTrigger((n) => n + 1);
      setAllExpanded(false);
    } else {
      setExpandAllTrigger((n) => n + 1);
      setAllExpanded(true);
    }
  }

  // Completed record types (for streaming progress)
  const [completedTypes, setCompletedTypes] = createSignal<string[]>([]);
  const [copied, setCopied] = createSignal(false);

  // Stream timeout feedback
  const [streamTimedOut, setStreamTimedOut] = createSignal(false);
  let streamTimeoutId: ReturnType<typeof setTimeout> | null = null;

  function startStreamTimeout() {
    clearStreamTimeout();
    streamTimeoutId = setTimeout(() => {
      closeConnections();
      setStreamTimedOut(true);
      if (status() === 'loading') setStatus('done');
    }, 30_000);
  }

  function clearStreamTimeout() {
    if (streamTimeoutId !== null) {
      clearTimeout(streamTimeoutId);
      streamTimeoutId = null;
    }
  }

  // Check mode state
  const [isCheckMode, setIsCheckMode] = createSignal(false);
  const [lintCategories, setLintCategories] = createSignal<LintCategory[]>([]);
  const [checkStats, setCheckStats] = createSignal<CheckDoneStats | null>(null);

  // Trace mode state
  const [isTraceMode, setIsTraceMode] = createSignal(false);
  const [traceHops, setTraceHops] = createSignal<TraceHop[]>([]);
  const [traceDoneStats, setTraceDoneStats] = createSignal<TraceDoneStats | null>(null);

  // DNSSEC mode state
  const [isDnssecMode, setIsDnssecMode] = createSignal(false);
  const [dnssecLevels, setDnssecLevels] = createSignal<ChainLevel[]>([]);
  const [dnssecDoneStats, setDnssecDoneStats] = createSignal<DnssecDoneStats | null>(null);

  // Compare mode state
  const [isCompareMode, setIsCompareMode] = createSignal(false);
  const [compareResults, setCompareResults] = createSignal<BatchEvent[]>([]);

  // Auth mode state
  const [isAuthMode, setIsAuthMode] = createSignal(false);
  const [authResults, setAuthResults] = createSignal<BatchEvent[]>([]);
  const [authServers, setAuthServers] = createSignal<string[]>([]);

  // Short mode state
  const [isShortMode, setIsShortMode] = createSignal(false);

  // Site metadata
  const [siteName, setSiteName] = createSignal('prism');
  const [siteVersion, setSiteVersion] = createSignal<string | null>(null);

  // IP enrichment
  const [ifconfigUrl, setIfconfigUrl] = createSignal<string | null>(null);
  const [enrichments, setEnrichments] = createSignal<Record<string, IpInfo>>({});

  // TLS inspector cross-link
  const [tlsUrl, setTlsUrl] = createSignal<string | null>(null);

  // Permalink state
  const [cacheKey, setCacheKey] = createSignal<string | null>(null);
  const [shareMessage, setShareMessage] = createSignal<string | null>(null);

  // Reactive document title — updated whenever mode or site name changes.
  createEffect(() => {
    const base = siteName();
    const mode = isCheckMode() ? 'Check'
      : isTraceMode() ? 'Trace'
      : isDnssecMode() ? 'DNSSEC'
      : isCompareMode() ? 'Compare'
      : isAuthMode() ? 'Auth'
      : null;
    document.title = mode ? `${base} • ${mode}` : base;
  });

  let eventSource: EventSource | null = null;
  let checkAbortController: AbortController | null = null;
  let traceAbortController: AbortController | null = null;
  let dnssecAbortController: AbortController | null = null;
  let compareAbortController: AbortController | null = null;
  let authAbortController: AbortController | null = null;
  let focusEditor: (() => void) | undefined;
  let clearEditor: (() => void) | undefined;
  let setEditorValue: ((v: string) => void) | undefined;
  function fillQuery(q: string) {
    setEditorValue?.(q);
    submitCombined(q);
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

  function abortDnssec() {
    if (dnssecAbortController) {
      dnssecAbortController.abort();
      dnssecAbortController = null;
    }
  }

  function abortCompare() {
    if (compareAbortController) {
      compareAbortController.abort();
      compareAbortController = null;
    }
  }

  function abortAuth() {
    if (authAbortController) {
      authAbortController.abort();
      authAbortController = null;
    }
  }

  function closeConnections() {
    clearStreamTimeout();
    closeEventSource();
    abortCheck();
    abortTrace();
    abortDnssec();
    abortCompare();
    abortAuth();
  }

  function cancelQuery() {
    closeConnections();
    setStatus('done');
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
  // Reset
  // ---------------------------------------------------------------------------

  function resetAll() {
    closeConnections();
    setQuery('');
    setResults([]);
    setStatus('idle');
    setError(null);
    setStats(null);
    setActiveTab('results');
    setIsCheckMode(false);
    setIsTraceMode(false);
    setIsDnssecMode(false);
    setIsCompareMode(false);
    setIsShortMode(false);
    setCompareResults([]);
    setIsAuthMode(false);
    setAuthResults([]);
    setAuthServers([]);
    setTraceHops([]);
    setTraceDoneStats(null);
    setDnssecLevels([]);
    setDnssecDoneStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setCompletedTypes([]);
    setEnrichments({});
    setCacheKey(null);
    setShareMessage(null);
    setStreamTimedOut(false);
    clearEditor?.();
    focusEditor?.();
    const url = new URL(window.location.href);
    url.searchParams.delete('q');
    url.searchParams.delete('r');
    window.history.replaceState(null, '', url.toString());
  }

  // ---------------------------------------------------------------------------
  // Shareable permalinks
  // ---------------------------------------------------------------------------

  function copyShareLink() {
    const key = cacheKey();
    if (!key) return;
    const url = `${window.location.origin}/?r=${key}`;
    navigator.clipboard.writeText(url).then(() => {
      setShareMessage('Copied!');
      setTimeout(() => setShareMessage(null), 2000);
    }).catch(() => {
      setShareMessage('Copy failed');
      setTimeout(() => setShareMessage(null), 2000);
    });
  }

  async function loadCachedResult(key: string) {
    setStatus('loading');
    setCacheKey(key);

    let response: Response;
    try {
      response = await fetch(`/api/results/${encodeURIComponent(key)}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Network error');
      setStatus('error');
      return;
    }

    if (!response.ok) {
      setError(response.status === 404 ? 'Shared result not found or expired' : `HTTP ${response.status}`);
      setStatus('error');
      return;
    }

    const cached = await response.json() as {
      query: string;
      mode: string;
      events: Array<{ event_type: string; data: unknown }>;
    };

    // Populate the query input.
    setQuery(cached.query);
    setEditorValue?.(cached.query);

    // Set mode flags based on cached mode.
    if (cached.mode === 'check') {
      setIsCheckMode(true);
      setActiveTab('lint');
    } else if (cached.mode === 'trace') {
      setIsTraceMode(true);
      setActiveTab('trace');
    } else if (cached.mode === 'compare') {
      setIsCompareMode(true);
      setActiveTab('transport');
    } else if (cached.mode === 'auth') {
      setIsAuthMode(true);
      setActiveTab('auth');
    } else {
      setActiveTab('results');
    }

    // Replay events to populate UI state.
    for (const ev of cached.events) {
      if (ev.event_type === 'batch') {
        try {
          const batch = parseBatchEvent(ev.data as Parameters<typeof parseBatchEvent>[0]);
          if (cached.mode === 'compare') {
            setCompareResults((prev) => [...prev, batch]);
          } else if (cached.mode === 'auth') {
            setAuthResults((prev) => [...prev, batch]);
          }
          setResults((prev) => [...prev, batch]);
        } catch { /* skip malformed */ }
      } else if (ev.event_type === 'lint') {
        try {
          const lint = ev.data as { category: string; results: LintCategory['results'] };
          setLintCategories((prev) => [...prev, { category: lint.category, results: lint.results }]);
        } catch { /* skip */ }
      } else if (ev.event_type === 'hop') {
        try {
          const hop = ev.data as { request_id: string; hop: TraceHop };
          setTraceHops((prev) => [...prev, hop.hop]);
        } catch { /* skip */ }
      } else if (ev.event_type === 'enrichment') {
        handleEnrichmentEvent(ev.data);
      } else if (ev.event_type === 'done') {
        const done = ev.data as Record<string, unknown>;
        if (cached.mode === 'check') {
          setCheckStats(done as unknown as CheckDoneStats);
        } else if (cached.mode === 'trace') {
          setTraceDoneStats(done as unknown as TraceDoneStats);
        } else if (cached.mode === 'auth') {
          const authDone = done as Record<string, unknown>;
          if (Array.isArray(authDone.auth_servers)) setAuthServers(authDone.auth_servers as string[]);
          setStats(done as unknown as DoneStats);
        } else {
          setStats(done as unknown as DoneStats);
        }
      }
    }

    setStatus('done');
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
        setCompletedTypes((prev) => prev.includes(batch.record_type) ? prev : [...prev, batch.record_type]);
      } catch (e) {
        console.error('Failed to parse batch event:', e);
      }
    });

    es.addEventListener('enrichment', (event) => {
      try {
        handleEnrichmentEvent(JSON.parse(event.data));
      } catch { /* ignore */ }
    });

    es.addEventListener('done', (event) => {
      try {
        const done = JSON.parse(event.data) as DoneStats;
        setStats(done);
        if (done.cache_key) setCacheKey(done.cache_key);
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
  // Submit — combined mode (any combination of +check, +trace, +dnssec)
  // ---------------------------------------------------------------------------

  function handleEnrichmentEvent(data: unknown) {
    const ev = data as { enrichments?: Record<string, IpInfo> };
    if (ev.enrichments) {
      setEnrichments((prev) => ({ ...prev, ...ev.enrichments }));
    }
  }

  function submitCombined(q: string) {
    closeConnections();

    const wantCheck = hasCheckFlag(q);
    const wantTrace = hasTraceFlag(q);
    const wantDnssec = hasDnssecFlag(q);
    const wantCompare = hasCompareFlag(q);
    const wantAuth = hasAuthFlag(q);
    const wantShort = hasShortFlag(q);

    const { domain, record_type } = extractTraceParams(q);
    const { domain: checkDomain, servers } = extractCheckParams(q);
    if (!domain) return;

    setQuery(q);
    setTraceHops([]);
    setTraceDoneStats(null);
    setDnssecLevels([]);
    setDnssecDoneStats(null);
    setResults([]);
    setStats(null);
    setLintCategories([]);
    setAllExpanded(false);
    setCheckStats(null);
    setCompareResults([]);
    setAuthResults([]);
    setAuthServers([]);
    setError(null);
    setIsTraceMode(wantTrace);
    setIsCheckMode(wantCheck);
    setIsDnssecMode(wantDnssec);
    setIsCompareMode(wantCompare);
    setIsAuthMode(wantAuth);
    setIsShortMode(wantShort);
    setEnrichments({});
    setCacheKey(null);
    setStreamTimedOut(false);
    setStatus('loading');
    // Pick the first active tab: auth > transport > dnssec > trace > lint
    setActiveTab(wantAuth ? 'auth' : wantCompare ? 'transport' : wantDnssec ? 'dnssec' : wantTrace ? 'trace' : 'lint');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    startStreamTimeout();

    // Count how many streams we launch; status → done when all finish.
    let streamCount = 0;
    if (wantCheck) streamCount++;
    if (wantTrace) streamCount++;
    if (wantDnssec) streamCount++;
    if (wantCompare) streamCount++;
    if (wantAuth) streamCount++;

    let doneCount = 0;
    function onStreamDone() {
      doneCount++;
      if (doneCount >= streamCount) {
        clearStreamTimeout();
        setStatus('done');
      }
    }

    // Background query to populate Results tab (strip all routing flags).
    if (wantCheck) {
      // Check already provides batch events — no separate background query needed.
    } else if (!wantCompare && !wantAuth) {
      // Compare and auth provide their own batch events — no separate background query needed.
      startBackgroundQuery(stripRoutingFlags(q));
    }

    // Check stream (provides batch + lint events)
    if (wantCheck) {
      const checkController = new AbortController();
      checkAbortController = checkController;

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
            try {
              const batch = parseBatchEvent(data as Parameters<typeof parseBatchEvent>[0]);
              setResults((prev) => [...prev, batch]);
              setCompletedTypes((prev) => prev.includes(batch.record_type) ? prev : [...prev, batch.record_type]);
            }
            catch (e) { console.error('Failed to parse batch event:', e); }
          } else if (eventType === 'lint') {
            try {
              const ev = data as { category: string; results: LintCategory['results'] };
              setLintCategories((prev) => [...prev, { category: ev.category, results: ev.results }]);
            } catch (e) { console.error('Failed to parse lint event:', e); }
          } else if (eventType === 'done') {
            const checkDone = data as CheckDoneStats;
            setCheckStats(checkDone);
            if (checkDone.cache_key) setCacheKey(checkDone.cache_key);
            // Populate query-level stats so the Results tab summary works.
            setStats({
              total_queries: checkDone.total_checks,
              duration_ms: checkDone.duration_ms,
              warnings: [],
            });
            onStreamDone();
          } else if (eventType === 'enrichment') {
            handleEnrichmentEvent(data);
          } else if (eventType === 'error') {
            const ev = data as { message?: string; code?: string };
            setError(ev.message ?? ev.code ?? 'Unknown error');
          }
        }, checkController.signal);
      })();
    }

    // Trace stream (provides hop events)
    if (wantTrace) {
      const traceController = new AbortController();
      traceAbortController = traceController;

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
          } else if (eventType === 'enrichment') {
            handleEnrichmentEvent(data);
          } else if (eventType === 'done') {
            const done = data as TraceDoneStats;
            setTraceDoneStats(done);
            if (done.cache_key) setCacheKey(done.cache_key);
            onStreamDone();
          } else if (eventType === 'error') {
            const ev = data as { message?: string; code?: string };
            setError(ev.message ?? ev.code ?? 'Unknown error');
          }
        }, traceController.signal);
      })();
    }

    // DNSSEC stream (provides chain events)
    if (wantDnssec) {
      const controller = new AbortController();
      dnssecAbortController = controller;

      (async () => {
        let response: Response;
        try {
          response = await fetch('/api/dnssec', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
            body: JSON.stringify({ domain }),
            signal: controller.signal,
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
          if (eventType === 'chain') {
            try {
              const ev = data as { request_id: string; level: ChainLevel };
              setDnssecLevels((prev) => [...prev, ev.level]);
            } catch (e) { console.error('Failed to parse chain event:', e); }
          } else if (eventType === 'done') {
            const done = data as DnssecDoneStats & { cache_key?: string };
            setDnssecDoneStats(done);
            if (done.cache_key) setCacheKey(done.cache_key);
            onStreamDone();
          } else if (eventType === 'error') {
            const ev = data as { message?: string; code?: string };
            setError(ev.message ?? ev.code ?? 'Unknown error');
          }
        }, controller.signal);
      })();
    }

    // Compare stream (provides batch events with transport field)
    if (wantCompare) {
      const controller = new AbortController();
      compareAbortController = controller;

      // Build the POST body: domain, record types, servers (same extraction as check).
      const tokens = q.trim().split(/\s+/);
      const compareDomain = tokens[0] ?? '';
      const recordTypes = tokens.slice(1).filter((t) => /^[A-Za-z0-9]+$/.test(t) && !t.startsWith('@') && !t.startsWith('+')).map((t) => t.toUpperCase());
      const compareServers = tokens.filter((t) => t.startsWith('@')).map((t) => t.slice(1));
      const compareBody: Record<string, unknown> = { domain: compareDomain };
      if (recordTypes.length > 0) compareBody.record_types = recordTypes;
      if (compareServers.length > 0) compareBody.servers = compareServers;

      (async () => {
        let response: Response;
        try {
          response = await fetch('/api/compare', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
            body: JSON.stringify(compareBody),
            signal: controller.signal,
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
            try {
              const batch = parseBatchEvent(data as Parameters<typeof parseBatchEvent>[0]);
              setCompareResults((prev) => [...prev, batch]);
              // Also populate Results tab for the general view.
              setResults((prev) => [...prev, batch]);
              setCompletedTypes((prev) => prev.includes(batch.record_type) ? prev : [...prev, batch.record_type]);
            } catch (e) { console.error('Failed to parse batch event:', e); }
          } else if (eventType === 'enrichment') {
            handleEnrichmentEvent(data);
          } else if (eventType === 'done') {
            const done = data as DoneStats & { transports?: string[]; cache_key?: string };
            setStats({
              total_queries: done.total_queries,
              duration_ms: done.duration_ms,
              warnings: done.warnings ?? [],
            });
            if (done.cache_key) setCacheKey(done.cache_key);
            onStreamDone();
          } else if (eventType === 'error') {
            const ev = data as { message?: string; code?: string };
            setError(ev.message ?? ev.code ?? 'Unknown error');
          }
        }, controller.signal);
      })();
    }

    // Auth stream (provides batch events with source field)
    if (wantAuth) {
      const controller = new AbortController();
      authAbortController = controller;

      const tokens = q.trim().split(/\s+/);
      const authDomain = tokens[0] ?? '';
      const recordTypes = tokens.slice(1).filter((t) => /^[A-Za-z0-9]+$/.test(t) && !t.startsWith('@') && !t.startsWith('+')).map((t) => t.toUpperCase());
      const authServersSpec = tokens.filter((t) => t.startsWith('@')).map((t) => t.slice(1));
      const authBody: Record<string, unknown> = { domain: authDomain };
      if (recordTypes.length > 0) authBody.record_types = recordTypes;
      if (authServersSpec.length > 0) authBody.servers = authServersSpec;

      (async () => {
        let response: Response;
        try {
          response = await fetch('/api/authcompare', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', 'Accept': 'text/event-stream' },
            body: JSON.stringify(authBody),
            signal: controller.signal,
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
            try {
              const batch = parseBatchEvent(data as Parameters<typeof parseBatchEvent>[0]);
              setAuthResults((prev) => [...prev, batch]);
              setResults((prev) => [...prev, batch]);
              setCompletedTypes((prev) => prev.includes(batch.record_type) ? prev : [...prev, batch.record_type]);
            } catch (e) { console.error('Failed to parse batch event:', e); }
          } else if (eventType === 'enrichment') {
            handleEnrichmentEvent(data);
          } else if (eventType === 'done') {
            const done = data as DoneStats & { auth_servers?: string[]; cache_key?: string };
            if (done.auth_servers) setAuthServers(done.auth_servers);
            setStats({
              total_queries: done.total_queries,
              duration_ms: done.duration_ms,
              warnings: done.warnings ?? [],
            });
            if (done.cache_key) setCacheKey(done.cache_key);
            onStreamDone();
          } else if (eventType === 'error') {
            const ev = data as { message?: string; code?: string };
            setError(ev.message ?? ev.code ?? 'Unknown error');
          }
        }, controller.signal);
      })();
    }
  }

  // ---------------------------------------------------------------------------
  // Submit — query mode (GET /api/query)
  // ---------------------------------------------------------------------------

  function submitQuery(q: string) {
    const wantCheck = hasCheckFlag(q);
    const wantTrace = hasTraceFlag(q);
    const wantDnssec = hasDnssecFlag(q);
    const wantCompare = hasCompareFlag(q);
    const wantAuth = hasAuthFlag(q);

    // Any combination of routing flags → unified combined handler.
    if (wantCheck || wantTrace || wantDnssec || wantCompare || wantAuth) {
      submitCombined(q);
      return;
    }

    closeConnections();

    setQuery(q);
    setResults([]);
    setError(null);
    setStats(null);
    setIsCheckMode(false);
    setIsTraceMode(false);
    setIsDnssecMode(false);
    setIsCompareMode(false);
    setCompareResults([]);
    setIsAuthMode(false);
    setAuthResults([]);
    setAuthServers([]);
    setTraceHops([]);
    setTraceDoneStats(null);
    setDnssecLevels([]);
    setDnssecDoneStats(null);
    setLintCategories([]);
    setCheckStats(null);
    setEnrichments({});
    setCacheKey(null);
    setStreamTimedOut(false);
    setStatus('loading');
    setActiveTab('results');
    addToHistory(q);

    const url = new URL(window.location.href);
    url.searchParams.set('q', q);
    window.history.pushState(null, '', url.toString());

    startStreamTimeout();

    const sseUrl = `/api/query?q=${encodeURIComponent(q)}`;
    const es = new EventSource(sseUrl);
    eventSource = es;

    es.addEventListener('batch', (event) => {
      try {
        const batch = parseBatchEvent(JSON.parse(event.data));
        setResults((prev) => [...prev, batch]);
        setCompletedTypes((prev) => prev.includes(batch.record_type) ? prev : [...prev, batch.record_type]);
      } catch (e) {
        console.error('Failed to parse batch event:', e);
      }
    });

    es.addEventListener('enrichment', (event) => {
      try {
        handleEnrichmentEvent(JSON.parse(event.data));
      } catch (e) {
        console.error('Failed to parse enrichment event:', e);
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
        if (doneData.cache_key) setCacheKey(doneData.cache_key);
      } catch (e) {
        console.error('Failed to parse done event:', e);
      }
      clearStreamTimeout();
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

  // Escape needs special handling (works inside editors too)
  function handleEscape(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    if (showHelp()) { setShowHelp(false); e.preventDefault(); return; }
    const target = e.target as HTMLElement;
    const isEditing =
      target.tagName === 'INPUT' ||
      target.tagName === 'TEXTAREA' ||
      target.isContentEditable ||
      !!target.closest('.cm-editor');
    if (isEditing) {
      const cmContent = (target.closest('.cm-editor') as HTMLElement | null)?.querySelector<HTMLElement>('.cm-content');
      cmContent?.blur();
      e.preventDefault();
    }
  }

  function navigateTab(e: KeyboardEvent) {
    if (!hasContent()) return;
    e.preventDefault();
    const visibleTabs: ActiveTab[] = [];
    if (isAuthMode()) visibleTabs.push('auth');
    if (isCompareMode()) visibleTabs.push('transport');
    if (isDnssecMode()) visibleTabs.push('dnssec');
    if (isTraceMode()) visibleTabs.push('trace');
    if (isCheckMode()) visibleTabs.push('lint');
    visibleTabs.push('results', 'servers');
    const idx = visibleTabs.indexOf(activeTab());
    if (idx === -1) return;
    const next = e.key === 'l'
      ? Math.min(idx + 1, visibleTabs.length - 1)
      : Math.max(idx - 1, 0);
    setActiveTab(visibleTabs[next]);
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  onMount(() => {
    // Fetch client config (site name, version, ifconfig URL for IP links).
    fetch('/api/config')
      .then((r) => r.json())
      .then((cfg: { site_name?: string; version?: string; ifconfig_url?: string; tls_url?: string }) => {
        if (cfg.site_name) {
          setSiteName(cfg.site_name);
        }
        if (cfg.version) setSiteVersion(cfg.version);
        if (cfg.ifconfig_url) setIfconfigUrl(cfg.ifconfig_url);
        if (cfg.tls_url) setTlsUrl(cfg.tls_url);
      })
      .catch(() => { /* non-critical */ });

    const params = new URLSearchParams(window.location.search);
    const r = params.get('r');
    if (r) {
      loadCachedResult(r);
    } else {
      const q = params.get('q');
      if (q) submitQuery(q);
    }

    document.addEventListener('keydown', handleEscape);

    const cleanupShortcuts = createKeyboardShortcuts({
      '?': (e) => { e.preventDefault(); setShowHelp((v) => !v); },
      '/': (e) => { e.preventDefault(); focusEditor?.(); },
      'r': (e) => { const q = query(); if (q && status() !== 'loading') { e.preventDefault(); submitQuery(q); } },
      'h': navigateTab,
      'l': navigateTab,
    });

    onCleanup(() => {
      cleanupShortcuts();
      document.removeEventListener('keydown', handleEscape);
    });
  });

  onCleanup(() => {
    closeConnections();
  });

  // ---------------------------------------------------------------------------
  // Derived display state
  // ---------------------------------------------------------------------------

  const hasContent = () =>
    status() !== 'idle' || results().length > 0 || lintCategories().length > 0 || traceHops().length > 0 || dnssecLevels().length > 0 || compareResults().length > 0 || authResults().length > 0;
  const isLoading  = () => status() === 'loading';

  // Agreement/divergence counts for the results summary
  const agreementCounts = () => {
    const groups = groupByRecordType(results());
    let agree = 0;
    let diverge = 0;
    for (const g of groups) {
      if (hasDeviation(g.lookups)) diverge++;
      else if (lookupsAgree(g.lookups)) agree++;
    }
    return { agree, diverge };
  };

  // Lint tab badge: show worst status count when done.
  function lintTabBadge() {
    const s = checkStats();
    if (!s) return null;
    if (s.failed > 0)   return <> <span class="tab-badge">{'\u2718'}{s.failed}</span></>;
    if (s.warnings > 0) return <> <span class="tab-badge">{'\u26A0'}{s.warnings}</span></>;
    return <> <span class="tab-badge">{'\u2713'}</span></>;
  }

  // DNSSEC tab badge + issue counts from chain findings.
  function dnssecIssueCounts(): { warnings: number; errors: number } {
    let warnings = 0;
    let errors = 0;
    for (const level of dnssecLevels()) {
      for (const f of level.findings) {
        if (f.severity === 'warning') warnings++;
        else if (f.severity === 'failed') errors++;
      }
    }
    return { warnings, errors };
  }

  function dnssecTabBadge() {
    if (!dnssecDoneStats()) return null;
    const { warnings, errors } = dnssecIssueCounts();
    if (errors > 0)   return <> <span class="tab-badge">{'\u2718'}{errors}</span></>;
    if (warnings > 0) return <> <span class="tab-badge">{'\u26A0'}{warnings}</span></>;
    return <> <span class="tab-badge">{'\u2713'}</span></>;
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <>
      <div class="app">
      <SuiteNav current="dns" />
      <a href="#main-content" class="skip-link">Skip to results</a>
      <header class="header">
        <h1 class="logo">{siteName()}</h1>
        <span class="tagline">DNS, refracted</span>
        <div class="header-actions">
          <ThemeToggle theme={themeResult} class="header-btn" />
          <button
            class="header-btn"
            onClick={() => setShowHelp((v) => !v)}
            aria-label="Open help"
            title="Help (?)"
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
          disabled={status() === 'loading'}
          onReset={status() !== 'idle' ? resetAll : undefined}
          onReady={(api) => { focusEditor = api.focus; clearEditor = api.clear; setEditorValue = api.setValue; }}
          shareLabel={status() === 'done' && cacheKey() ? (shareMessage() ?? 'Share') : undefined}
          onShare={copyShareLink}
        />

        {/* Empty state — shown on landing before any query */}
        <Show when={!hasContent()}>
          <div class="welcome">
            <p class="welcome-tagline">
              Multi-resolver DNS queries, health checks, and delegation traces — right in your browser.
            </p>
            <div class="mode-cards welcome-cards">
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">Query</div>
                <p class="mode-card__desc welcome-card-desc">
                  Fan out to multiple resolvers at once. See who answers differently, who's faster, and where results diverge.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('example.com A AAAA @cloudflare @google')}
                  title="Click to run"
                >
                  example.com A AAAA @cloudflare @google
                </button>
              </div>
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">Check</div>
                <p class="mode-card__desc welcome-card-desc">
                  Full domain health audit in one shot — 15 record types plus DMARC lint. Surfaces missing SPF, broken DMARC, DNSSEC mismatches, and more.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('example.com +check')}
                  title="Click to run"
                >
                  example.com +check
                </button>
              </div>
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">Trace</div>
                <p class="mode-card__desc welcome-card-desc">
                  Walk the delegation chain from root servers to authoritative, hop by hop. Find broken delegations, stale glue, and split-brain DNS.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('example.com A +trace')}
                  title="Click to run"
                >
                  example.com A +trace
                </button>
              </div>
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">DNSSEC</div>
                <p class="mode-card__desc welcome-card-desc">
                  Validate the DNSSEC chain of trust from root to authoritative. Checks DNSKEY, DS, and RRSIG records at each delegation level.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('dnssec-deployment.org +dnssec')}
                  title="Click to run"
                >
                  dnssec-deployment.org +dnssec
                </button>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('dnssec-failed.org +dnssec')}
                  title="Click to run"
                >
                  dnssec-failed.org +dnssec
                </button>
              </div>
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">Compare</div>
                <p class="mode-card__desc welcome-card-desc">
                  Query across all four transports — UDP, TCP, DoT, DoH — in parallel. Detect middlebox interference, protocol-specific filtering, or transport disagreements.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('example.com A +compare')}
                  title="Click to run"
                >
                  example.com A +compare
                </button>
              </div>
              <div class="mode-card welcome-card">
                <div class="mode-card__title welcome-card-title">Auth</div>
                <p class="mode-card__desc welcome-card-desc">
                  Compare authoritative nameserver answers against recursive resolver answers. Reveal caching staleness, NXDOMAIN hijacking, or split-horizon inconsistencies.
                </p>
                <button
                  class="mode-card__example welcome-example"
                  onClick={() => fillQuery('example.com A +auth')}
                  title="Click to run"
                >
                  example.com A +auth
                </button>
              </div>
            </div>
          </div>
        </Show>

        <Show when={hasContent()}>
          {/* Row 1: Tabs */}
          <div class="tabs">
            <div class="tabs-left" role="tablist">
              <Show when={isAuthMode()}>
                <button role="tab" aria-selected={activeTab() === 'auth'} aria-controls="panel-auth" class={`tab${activeTab() === 'auth' ? ' active' : ''}`} onClick={() => setActiveTab('auth')}>Auth</button>
              </Show>
              <Show when={isCompareMode()}>
                <button role="tab" aria-selected={activeTab() === 'transport'} aria-controls="panel-transport" class={`tab${activeTab() === 'transport' ? ' active' : ''}`} onClick={() => setActiveTab('transport')}>Transport</button>
              </Show>
              <Show when={isDnssecMode()}>
                <button role="tab" aria-selected={activeTab() === 'dnssec'} aria-controls="panel-dnssec" class={`tab${activeTab() === 'dnssec' ? ' active' : ''}${dnssecIssueCounts().errors ? ' tab--failed' : dnssecIssueCounts().warnings ? ' tab--warning' : ''}`} onClick={() => setActiveTab('dnssec')}>DNSSEC{dnssecTabBadge()}</button>
              </Show>
              <Show when={isTraceMode()}>
                <button role="tab" aria-selected={activeTab() === 'trace'} aria-controls="panel-trace" class={`tab${activeTab() === 'trace' ? ' active' : ''}`} onClick={() => setActiveTab('trace')}>Trace</button>
              </Show>
              <Show when={isCheckMode()}>
                <button role="tab" aria-selected={activeTab() === 'lint'} aria-controls="panel-lint" class={`tab${activeTab() === 'lint' ? ' active' : ''}${checkStats()?.failed ? ' tab--failed' : checkStats()?.warnings ? ' tab--warning' : ''}`} onClick={() => setActiveTab('lint')}>Lint{lintTabBadge()}</button>
              </Show>
              <button role="tab" aria-selected={activeTab() === 'results'} aria-controls="main-content" class={`tab ${activeTab() === 'results' ? 'active' : ''}`} onClick={() => setActiveTab('results')}>Results</button>
              <button role="tab" aria-selected={activeTab() === 'servers'} aria-controls="main-content" class={`tab ${activeTab() === 'servers' ? 'active' : ''}`} onClick={() => setActiveTab('servers')}>Servers</button>

            </div>
          </div>

          {/* Row 2: Summary + actions (or loading status) */}
          <Show when={isLoading()}>
            <div class="toolbar-row">
              <div class="status-info">
                <span class="status-loading-text">
                  {isCheckMode() && isTraceMode() ? 'Tracing + Checking…'
                    : isAuthMode() ? 'Comparing auth vs recursive…'
                    : isCompareMode() ? 'Comparing transports…'
                    : isDnssecMode() ? 'Validating DNSSEC…'
                    : isCheckMode() ? 'Checking…'
                    : isTraceMode() ? 'Tracing…'
                    : completedTypes().length > 0
                      ? `Querying… ${completedTypes().join(', ')} done`
                      : 'Querying…'}
                </span>
                <button class="cancel-btn" onClick={cancelQuery} title="Cancel query">cancel</button>
              </div>
            </div>
          </Show>
          <Show when={status() === 'done' && results().length > 0}>
            <div class="toolbar-row">
              <div class="results-summary">
                <span class="results-summary-item">{stats()!.total_queries} queries</span>
                <span class="results-summary-sep">/</span>
                <span class="results-summary-item">{results().length} batches</span>
                <span class="results-summary-sep">/</span>
                <span class="results-summary-item">{stats()!.duration_ms}ms</span>
                <Show when={tlsUrl()}>
                  {(tls) => (
                    <>
                      <span class="results-summary-sep">/</span>
                      <a
                        class="eco-link"
                        href={`${tls()}/?h=${encodeURIComponent(extractTraceParams(query()).domain)}&ref=prism`}
                        target="_blank"
                        rel="noopener noreferrer"
                      >TLS ↗</a>
                    </>
                  )}
                </Show>
                <Show when={stats()!.transport && stats()!.transport !== 'udp'}>
                  <span class="results-summary-sep">/</span>
                  <span class="results-summary-item status-badge transport-badge">{stats()!.transport!.toUpperCase()}</span>
                </Show>
                <Show when={stats()!.dnssec}>
                  <span class="results-summary-sep">/</span>
                  <span class="results-summary-item status-badge dnssec-badge">DNSSEC</span>
                </Show>
                <Show when={stats()!.warnings.length > 0}>
                  <span class="results-summary-sep">/</span>
                  <span class="results-summary-item status-warnings" title={stats()!.warnings.join('; ')}>{stats()!.warnings.length} warning{stats()!.warnings.length !== 1 ? 's' : ''}</span>
                </Show>
                <Show when={agreementCounts().agree > 0}>
                  <span class="results-summary-sep">/</span>
                  <span class="results-summary-item agree-badge" aria-label="All servers agree">{agreementCounts().agree} agree</span>
                </Show>
                <Show when={agreementCounts().diverge > 0}>
                  <span class="results-summary-sep">/</span>
                  <span class="results-summary-item deviation-badge" aria-label="Results diverge">{agreementCounts().diverge} diverge</span>
                </Show>
              </div>
              <div class="toolbar-actions">
                <button
                  class="export-btn"
                  onClick={() => {
                    const ctx: MarkdownContext = { query: query(), stats: stats(), agreeCounts: agreementCounts() };
                    copyToClipboard(toMarkdown(results(), ctx)).then((ok) => {
                      if (ok) { setCopied(true); setTimeout(() => setCopied(false), 1500); }
                    });
                  }}
                  title="Copy results as Markdown table"
                >{copied() ? 'Copied' : 'Copy MD'}</button>
                <button class="export-btn" onClick={() => downloadFile(toCsv(results()), 'dns-results.csv', 'text/csv')} title="Download results as CSV">CSV</button>
                <button class="export-btn" onClick={() => downloadFile(toJson(results(), stats()), 'dns-results.json', 'application/json')} title="Download results as JSON">JSON</button>
              </div>
            </div>
          </Show>

          {/* Row 3: View options — results tab */}
          <Show when={activeTab() === 'results' && results().length > 0}>
            <div class="view-options">
              <button class={`view-btn${hideNx() ? ' active' : ''}`} onClick={toggleHideNx} title="Hide groups where all servers returned NXDOMAIN">hide NX</button>
              <button class={`view-btn${compact() ? ' active' : ''}`} onClick={toggleCompact} title="Collapse groups where all servers agree">compact</button>
              <button class={`view-btn${devOnly() ? ' active' : ''}`} onClick={toggleDevOnly} title="Show only groups where servers diverge">deviations</button>
              <button class={`view-btn${sortView() ? ' active' : ''}`} onClick={toggleSort} title="Sort: deviations first, then records, then NXDOMAIN">sort</button>
              <button class={`view-btn${explain() ? ' active' : ''}`} onClick={toggleExplain} title="Show explanations for record fields">explain</button>
              <span class="view-options-spacer" />
              <button class={`view-btn${allExpanded() ? ' active' : ''}`} onClick={toggleExpandAll} title={allExpanded() ? 'Collapse all record rows' : 'Expand all record rows'}>{allExpanded() ? 'collapse all' : 'expand all'}</button>
            </div>
          </Show>
          {/* Row 3: View options — servers tab */}
          <Show when={activeTab() === 'servers' && results().length > 0}>
            <div class="view-options">
              <button class={`view-btn${devOnly() ? ' active' : ''}`} onClick={toggleDevOnly} title="Show only groups where servers diverge">deviations</button>
              <button class={`view-btn${sortView() ? ' active' : ''}`} onClick={toggleSort} title="Sort: deviations first">sort</button>
              <button class={`view-btn${explain() ? ' active' : ''}`} onClick={toggleExplain} title="Show explanations for record fields">explain</button>
            </div>
          </Show>
          {/* Row 3: View options — auth tab */}
          <Show when={activeTab() === 'auth' && authResults().length > 0}>
            <div class="view-options">
              <button class={`view-btn${devOnly() ? ' active' : ''}`} onClick={toggleDevOnly} title="Show only record types where auth and recursive diverge">deviations</button>
              <button class={`view-btn${sortView() ? ' active' : ''}`} onClick={toggleSort} title="Sort: divergences first">sort</button>
              <button class={`view-btn${explain() ? ' active' : ''}`} onClick={toggleExplain} title="Show explanations for record fields">explain</button>
            </div>
          </Show>
          {/* Row 3: View options — transport tab */}
          <Show when={activeTab() === 'transport' && compareResults().length > 0}>
            <div class="view-options">
              <button class={`view-btn${devOnly() ? ' active' : ''}`} onClick={toggleDevOnly} title="Show only record types where transports diverge">deviations</button>
              <button class={`view-btn${sortView() ? ' active' : ''}`} onClick={toggleSort} title="Sort: divergences first">sort</button>
              <button class={`view-btn${explain() ? ' active' : ''}`} onClick={toggleExplain} title="Show explanations for record fields">explain</button>
            </div>
          </Show>
        </Show>

        {/*
          Tab panes use CSS display toggling instead of <Show> unmount/remount.
          Unmounting one pane before mounting another leaves a frame with no
          content, which causes a layout-height collapse that Safari repaints
          as a visible page shift. Keeping all panes in the DOM and toggling
          display avoids that intermediate empty state entirely.
        */}

        {/* Auth tab pane — mounted once auth mode starts */}
        <Show when={isAuthMode()}>
          <div id="panel-auth" role="tabpanel" style={{ display: activeTab() === 'auth' ? 'block' : 'none' }}>
            <AuthComparison
              results={authResults()}
              activeTab={activeTab()}
              explain={explain()}
              sort={sortView()}
              devOnly={devOnly()}
              authServers={authServers()}
            />
          </div>
        </Show>

        {/* Transport tab pane — mounted once compare mode starts */}
        <Show when={isCompareMode()}>
          <div id="panel-transport" role="tabpanel" style={{ display: activeTab() === 'transport' ? 'block' : 'none' }}>
            <TransportComparison
              results={compareResults()}
              activeTab={activeTab()}
              explain={explain()}
              sort={sortView()}
              devOnly={devOnly()}
            />
          </div>
        </Show>

        {/* DNSSEC tab pane — mounted once dnssec mode starts */}
        <Show when={isDnssecMode()}>
          <div id="panel-dnssec" role="tabpanel" style={{ display: activeTab() === 'dnssec' ? 'block' : 'none' }}>
            <DnssecView
              levels={dnssecLevels()}
              doneStats={dnssecDoneStats()}
              isLoading={isLoading()}
              activeTab={activeTab()}
            />
          </div>
        </Show>

        {/* Trace tab pane — mounted once trace mode starts */}
        <Show when={isTraceMode()}>
          <div id="panel-trace" role="tabpanel" style={{ display: activeTab() === 'trace' ? 'block' : 'none' }}>
            <TraceView
              hops={traceHops()}
              doneStats={traceDoneStats()}
              isLoading={isLoading()}
              activeTab={activeTab()}
              ifconfigUrl={ifconfigUrl()}
              enrichments={enrichments()}
            />
          </div>
        </Show>

        {/* Lint tab pane — mounted once check mode starts */}
        <Show when={isCheckMode()}>
          <div id="panel-lint" role="tabpanel" style={{ display: activeTab() === 'lint' ? 'block' : 'none' }}>
            <LintTab
              categories={lintCategories()}
              doneStats={checkStats()}
              isLoading={isLoading()}
            />
          </div>
        </Show>

        {/* Results / Servers / JSON — always mounted, hidden when another pane is active */}
        <Show when={hasContent()}>
          <div id="main-content" style={{ display: activeTab() !== 'lint' && activeTab() !== 'trace' && activeTab() !== 'dnssec' && activeTab() !== 'transport' && activeTab() !== 'auth' ? 'block' : 'none' }}>
            <ResultsTable
              results={results()}
              stats={stats()}
              status={status()}
              error={error()}
              activeTab={activeTab() as 'results' | 'servers'}
              hideNx={hideNx()}
              compact={compact()}
              devOnly={devOnly()}
              sort={sortView()}
              explain={explain()}
              short={isShortMode()}
              expandAll={expandAllTrigger()}
              collapseAll={collapseAllTrigger()}
              ifconfigUrl={ifconfigUrl()}
              enrichments={enrichments()}
            />
          </div>
        </Show>

        <Show when={status() === 'done' && query()}>
          <DnsCrossLinks domain={query()} tlsUrl={tlsUrl()} />
        </Show>

        <Show when={streamTimedOut()}>
          <p class="stream-timeout-msg">Stream timed out — showing partial results</p>
        </Show>
      </main>

      <SiteFooter
        aboutText={<>
          <em>{siteName()}</em> is a multi-server DNS debugging and inspection service.
          Fan-out queries across resolvers with streaming results, DNSSEC validation, delegation tracing, and transport comparison.
          Built in <a href="https://www.rust-lang.org/" target="_blank" rel="noopener noreferrer">Rust</a>{" "}
          with <a href="https://github.com/tokio-rs/axum" target="_blank" rel="noopener noreferrer">Axum</a>{" "}
          and <a href="https://www.solidjs.com/" target="_blank" rel="noopener noreferrer">SolidJS</a>,{" "}
          powered by <a href="https://github.com/lukaspustina/mhost" target="_blank" rel="noopener noreferrer">mhost</a>.
          Open to use — rate limiting applies.
        </>}
        links={[
          { href: 'https://github.com/lukaspustina/mhost-prism', label: 'GitHub', external: true },
          { href: '/docs', label: 'API Docs' },
          { href: 'https://lukas.pustina.de', label: 'Author', external: true },
        ]}
        version={siteVersion() ?? undefined}
      />

      {/* Help modal */}
      <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
        <div class="help-section">
          <div class="help-section-title">Query syntax</div>
          <code class="help-syntax">domain [TYPE...] [@server...] [+flag...]</code>
          <p class="help-syntax-desc">Tokens are space-separated. Order within each group doesn't matter.</p>
        </div>

        <div class="help-section">
          <div class="help-section-title">Predefined servers</div>
          <table class="help-ref-table">
            <tbody>
              <tr><td class="help-token">@cloudflare</td><td>1.1.1.1 / 1.0.0.1</td></tr>
              <tr><td class="help-token">@google</td><td>8.8.8.8 / 8.8.4.4</td></tr>
              <tr><td class="help-token">@quad9</td><td>9.9.9.9</td></tr>
              <tr><td class="help-token">@mullvad</td><td>Mullvad DNS</td></tr>
              <tr><td class="help-token">@wikimedia</td><td>Wikimedia DNS</td></tr>
              <tr><td class="help-token">@dns4eu</td><td>DNS4EU</td></tr>
              <tr><td class="help-token">@system</td><td>/etc/resolv.conf</td></tr>
              <tr><td class="help-token">@1.2.3.4</td><td>Custom IP (if enabled by operator)</td></tr>
            </tbody>
          </table>
        </div>

        <div class="help-section">
          <div class="help-section-title">Flags</div>
          <table class="help-ref-table">
            <tbody>
              <tr><td class="help-token">+udp</td><td>UDP transport (default)</td></tr>
              <tr><td class="help-token">+tcp</td><td>TCP transport</td></tr>
              <tr><td class="help-token">+tls</td><td>DNS-over-TLS</td></tr>
              <tr><td class="help-token">+https</td><td>DNS-over-HTTPS</td></tr>
              <tr><td class="help-token">+dnssec</td><td>DNSSEC chain-of-trust validation</td></tr>
              <tr><td class="help-token">+check</td><td>Domain health check (15 types + DMARC lint)</td></tr>
              <tr><td class="help-token">+trace</td><td>Delegation trace (root → authoritative)</td></tr>
              <tr><td class="help-token">+compare</td><td>Transport comparison (UDP/TCP/TLS/HTTPS)</td></tr>
              <tr><td class="help-token">+auth</td><td>Authoritative vs recursive comparison</td></tr>
            </tbody>
          </table>
        </div>

        <div class="help-section">
          <div class="help-section-title">Record types</div>
          <p class="help-types">A &nbsp;AAAA &nbsp;MX &nbsp;TXT &nbsp;NS &nbsp;SOA &nbsp;CAA &nbsp;CNAME &nbsp;DNSKEY &nbsp;DS &nbsp;HTTPS &nbsp;SVCB &nbsp;SRV &nbsp;SSHFP &nbsp;TLSA &nbsp;NAPTR &nbsp;PTR &nbsp;HINFO &nbsp;OPENPGPKEY</p>
        </div>

        <div class="help-section">
          <div class="help-section-title">Keyboard shortcuts</div>
          <table class="shortcuts-table">
            <thead>
              <tr><th>Key</th><th>Action</th></tr>
            </thead>
            <tbody>
              <tr><td class="shortcut-key">/</td><td>Focus query input</td></tr>
              <tr><td class="shortcut-key">Enter</td><td>Submit query (when input focused)</td></tr>
              <tr><td class="shortcut-key">Tab</td><td>Accept autocomplete suggestion</td></tr>
              <tr><td class="shortcut-key">Escape</td><td>Dismiss autocomplete / blur input</td></tr>
              <tr><td class="shortcut-key">j / k</td><td>Navigate result rows</td></tr>
              <tr><td class="shortcut-key">h / l</td><td>Previous / next tab</td></tr>
              <tr><td class="shortcut-key">&uarr; / &darr;</td><td>Browse query history (in input)</td></tr>
              <tr><td class="shortcut-key">r</td><td>Re-run current query</td></tr>
              <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
            </tbody>
          </table>
        </div>
      </Modal>

    </div>
    </>
  );
}
