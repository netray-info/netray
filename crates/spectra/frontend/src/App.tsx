import { createSignal, createEffect, Show, For, onMount, onCleanup } from 'solid-js';
import UrlInput from './components/UrlInput';
import ExportButtons from './components/ExportButtons';
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
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import { inspect, fetchMeta } from './lib/api';
import type { MetaResponse } from './lib/api';
import { addToHistory } from './lib/history';
import type { InspectResponse, CheckStatus } from './lib/types';

const EXAMPLES = [
  'netray.info',
  'example.com',
  'github.com',
  'cloudflare.com',
];

const STATUS_ICON: Record<CheckStatus, string> = {
  pass: '\u2713',
  warn: '\u26A0',
  fail: '\u2717',
  skip: '\u2014',
};

const SECURITY_CHECK_NAMES = new Set([
  'hsts', 'csp', 'x_frame_options', 'x_content_type_options',
  'referrer_policy', 'permissions_policy',
]);

export default function App() {
  const [result, setResult] = createSignal<InspectResponse | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);
  const [lastQuery, setLastQuery] = createSignal('');
  const theme = createTheme('spectra_theme', 'system');

  let inputEl: HTMLInputElement | undefined;

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

    // Ctrl+L / Cmd+L — not filtered by createKeyboardShortcuts
    const ctrlLHandler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'l') {
        e.preventDefault();
        inputEl?.focus();
      }
    };
    document.addEventListener('keydown', ctrlLHandler);

    function clearCardActive() {
      document.querySelector('[data-card-active]')?.removeAttribute('data-card-active');
    }

    function navigateCards(e: KeyboardEvent) {
      const cards = Array.from(document.querySelectorAll<HTMLElement>('[data-card]'));
      if (cards.length === 0) return;
      e.preventDefault();
      const cur = document.querySelector<HTMLElement>('[data-card-active]');
      let idx = cur ? cards.indexOf(cur) : -1;
      if (idx === -1) {
        idx = e.key === 'j' ? 0 : cards.length - 1;
      } else {
        cur!.removeAttribute('data-card-active');
        idx += e.key === 'j' ? 1 : -1;
      }
      idx = Math.max(0, Math.min(idx, cards.length - 1));
      cards[idx].setAttribute('data-card-active', '');
      cards[idx].scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }

    function expandActiveCard(e: KeyboardEvent) {
      const active = document.querySelector<HTMLElement>('[data-card-active]');
      if (active) {
        e.preventDefault();
        active.querySelector<HTMLElement>('.section-card__header')?.click();
      }
    }

    document.addEventListener('mousedown', clearCardActive);

    const cleanupShortcuts = createKeyboardShortcuts({
      '?': (e) => { e.preventDefault(); setShowHelp(v => !v); },
      '/': (e) => { e.preventDefault(); inputEl?.focus(); },
      'e': (e) => { e.preventDefault(); setShowExplanations(v => !v); },
      'r': (e) => { const q = lastQuery(); if (q && !loading()) { e.preventDefault(); handleInspect(q); } },
      'j': navigateCards,
      'k': navigateCards,
      'Enter': expandActiveCard,
      'Escape': (e) => { e.preventDefault(); inputEl?.blur(); setShowHelp(false); },
    });

    onCleanup(() => {
      cleanupShortcuts();
      document.removeEventListener('keydown', ctrlLHandler);
      document.removeEventListener('mousedown', clearCardActive);
    });
  });

  async function handleInspect(url: string) {
    setError(null);
    setResult(null);
    setLoading(true);
    setLastQuery(url);

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

  function handleClear() {
    setResult(null);
    setError(null);
    setLastQuery('');
    window.history.replaceState({}, '', window.location.pathname);
  }

  // Auto-inspect if URL param present
  if (initialUrl) {
    handleInspect(initialUrl);
  }

  const STATUS_ORDER: Record<CheckStatus, number> = { pass: 0, skip: 1, warn: 2, fail: 3 };

  function worstStatus(statuses: CheckStatus[]): CheckStatus {
    let worst: CheckStatus = 'pass';
    for (const s of statuses) {
      if (STATUS_ORDER[s] > STATUS_ORDER[worst]) worst = s;
    }
    return worst;
  }

  function securityWorst(sec: InspectResponse['security']): CheckStatus {
    return worstStatus([
      sec.hsts.status, sec.x_frame_options.status,
      sec.x_content_type_options.status, sec.referrer_policy.status,
      sec.permissions_policy.status, sec.coop.status, sec.coep.status, sec.corp.status,
    ]);
  }

  function verdictClass(status: CheckStatus): string {
    return `badge badge--${status}`;
  }

  function durationClass(ms: number): string {
    if (ms < 500) return 'overview__value overview__value--fast';
    if (ms < 2000) return 'overview__value overview__value--ok';
    return 'overview__value overview__value--slow';
  }

  // Explain toggle
  const [showExplanations, setShowExplanations] = createSignal(false);

  function countChips(checks: Array<{ status: CheckStatus }>) {
    const counts: Partial<Record<CheckStatus, number>> = {};
    for (const c of checks) counts[c.status] = (counts[c.status] ?? 0) + 1;
    return counts;
  }

  function securityStatuses(sec: InspectResponse['security']): Array<{ status: CheckStatus }> {
    return [
      sec.hsts, sec.x_frame_options, sec.x_content_type_options,
      sec.referrer_policy, sec.permissions_policy, sec.coop, sec.coep, sec.corp,
    ];
  }

  // Expand / collapse all
  const [allExpanded, setAllExpanded] = createSignal(false);
  const [openQuality, setOpenQuality] = createSignal(false);
  const [openRedirects, setOpenRedirects] = createSignal(false);
  const [openSecurity, setOpenSecurity] = createSignal(false);
  const [openCsp, setOpenCsp] = createSignal(false);
  const [openCors, setOpenCors] = createSignal(false);
  const [openCookies, setOpenCookies] = createSignal(false);
  const [openCaching, setOpenCaching] = createSignal(false);
  const [openFingerprint, setOpenFingerprint] = createSignal(false);
  const [openHeaders, setOpenHeaders] = createSignal(false);

  createEffect(() => {
    const expanded = allExpanded();
    setOpenQuality(expanded);
    setOpenRedirects(expanded);
    setOpenSecurity(expanded);
    setOpenCsp(expanded);
    setOpenCors(expanded);
    setOpenCookies(expanded);
    setOpenCaching(expanded);
    setOpenFingerprint(expanded);
    setOpenHeaders(expanded);
  });

  return (
      <div class="app">
      <SuiteNav current="http" meta={meta()?.ecosystem as SuiteNavEcosystem} />
      <a class="skip-link" href="#results">Skip to results</a>

      <main class="container">
        <header class="header">
          <h1 class="logo">spectra</h1>
          <span class="tagline">HTTP, exposed</span>
          <div class="header-actions">
            <ThemeToggle theme={theme} class="header-btn" />
            <button class="header-btn" onClick={() => setShowHelp(true)} aria-label="Open help" title="Help (?)">?</button>
          </div>
        </header>

        <UrlInput
          onSubmit={handleInspect}
          onClear={handleClear}
          loading={loading()}
          value={lastQuery() || initialUrl}
          inputRef={el => (inputEl = el)}
          showCopyLink={!!result()}
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
              const hasRedirects = data.redirects.length > 0 || !!data.http_upgrade;
              const hasCsp = Object.keys(data.security.csp.directives).length > 0 || data.security.csp.issues.length > 0;
              const hasCookies = data.cookies.length > 0;

              return (
                <>
                  {/* Overview bar */}
                  <div class="overview">
                    {/* Row 1: response facts */}
                    <div class="overview__row">
                      <div class="overview__item">
                        <span class="overview__label">Verdict</span>
                        <span class={verdictClass(data.quality.verdict)}>
                          {data.quality.verdict}
                        </span>
                      </div>
                      <div class="overview__item">
                        <span class="overview__label">Status</span>
                        <span class="overview__value">{data.status}</span>
                      </div>
                      <div class="overview__item">
                        <span class="overview__label">HTTP</span>
                        <span class="overview__value">{data.http_version}</span>
                      </div>
                      <div class="overview__item">
                        <span class="overview__label">Duration</span>
                        <span class={durationClass(data.duration_ms)} title={data.duration_ms < 500 ? 'Fast' : data.duration_ms < 2000 ? 'Acceptable' : 'Slow'}>{data.duration_ms}ms</span>
                      </div>
                    </div>
                    {/* Row 2: IP + enrichment */}
                    <div class="overview__row overview__row--enrichment">
                      <span class="overview__row-label">Server</span>
                      <div class="overview__item">
                        <span class="overview__label">IP</span>
                        <span class="overview__value">{data.enrichment.ip}</span>
                        <Show when={data.enrichment.threat}>
                          <span class="badge badge--fail">{data.enrichment.threat}</span>
                        </Show>
                      </div>
                      <Show when={data.enrichment.org}>
                        <div class="overview__item">
                          <span class="overview__label" title="Server hosting provider">Org</span>
                          <span class="overview__value">{data.enrichment.org}</span>
                        </div>
                      </Show>
                      <Show when={data.enrichment.ip_type}>
                        <div class="overview__item">
                          <span class="overview__label" title="IP address classification">Category</span>
                          <span class="overview__value">
                            {data.enrichment.ip_type!.charAt(0).toUpperCase() + data.enrichment.ip_type!.slice(1)}
                          </span>
                        </div>
                      </Show>
                      <Show when={data.enrichment.role}>
                        <div class="overview__item">
                          <span class="overview__label">Role</span>
                          <span class="overview__value">{data.enrichment.role}</span>
                        </div>
                      </Show>
                      <a
                        href={data.enrichment.detail_url}
                        class="overview__ip-link"
                        target="_blank"
                        rel="noopener"
                        title="View IP details"
                      >IP ↗</a>
                    </div>
                  </div>

                  {/* Controls bar */}
                  <div class="section-controls">
                    <div class="section-controls__left">
                      <button
                        class="filter-toggle"
                        classList={{ 'filter-toggle--active': showExplanations() }}
                        onClick={() => setShowExplanations(!showExplanations())}
                        aria-pressed={showExplanations()}
                        title="Toggle explanations (e)"
                      >
                        explain
                      </button>
                    </div>
                    <div class="section-controls__right">
                      <ExportButtons result={data} />
                      <button
                        class="filter-toggle"
                        onClick={() => setAllExpanded(!allExpanded())}
                      >
                        {allExpanded() ? 'Collapse all' : 'Expand all'}
                      </button>
                    </div>
                  </div>

                  {/* Quality Assessment */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenQuality(!openQuality())} aria-expanded={openQuality()} aria-controls="section-quality-body">
                      <span class={`section-card__status section-card__status--${data.quality.verdict}`} />
                      <span class="section-card__title">Quality Assessment</span>
                      <span class="section-card__badges">
                        {(() => {
                          const counts = countChips(data.quality.checks);
                          return (
                            <>
                              <Show when={(counts.fail ?? 0) > 0}><span class="badge badge--fail">{counts.fail} failed</span></Show>
                              <Show when={(counts.warn ?? 0) > 0}><span class="badge badge--warn">{counts.warn} warnings</span></Show>
                              <Show when={(counts.skip ?? 0) > 0}><span class="badge badge--skip">{counts.skip} skipped</span></Show>
                              <Show when={(counts.pass ?? 0) > 0}><span class="badge badge--pass">{counts.pass} passed</span></Show>
                            </>
                          );
                        })()}
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openQuality() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>

                    {/* Always-visible chip row — outside collapse gate */}
                    <div class="check-chips">
                      {(() => {
                        const chipChecks = data.quality.checks
                          .filter(c => c.status !== 'skip')
                          .sort((a, b) => STATUS_ORDER[b.status] - STATUS_ORDER[a.status]);
                        return (
                          <For each={chipChecks}>
                            {(check) => (
                              <span
                                class={`filter-toggle check--${check.status}`}
                                title={showExplanations() && check.explanation ? check.explanation : undefined}
                              >
                                <span aria-hidden="true">{STATUS_ICON[check.status]}</span>
                                {' '}{check.label}
                              </span>
                            )}
                          </For>
                        );
                      })()}
                    </div>

                    {/* Expandable detail — non-security checks only */}
                    <Show when={openQuality()}>
                      <div class="section-card__body" id="section-quality-body">
                        <ul class="check-list">
                          {(() => {
                            const detailChecks = data.quality.checks.filter(c => !SECURITY_CHECK_NAMES.has(c.name));
                            return (
                              <For each={detailChecks}>
                                {(check) => (
                                  <li class={`check-list__item${check.status === 'fail' ? ' check-row--fail' : check.status === 'warn' ? ' check-row--warn' : ''}${showExplanations() ? ' check-list__item--explainable' : ''}`}>
                                    <span class={`badge badge--${check.status}`}>{check.status}</span>
                                    <span class="check-list__name">{check.label}</span>
                                    <span class="check-list__message">{check.message ?? ''}</span>
                                    <Show when={showExplanations() && check.explanation}>
                                      <span class="check-explain">{check.explanation}</span>
                                    </Show>
                                  </li>
                                )}
                              </For>
                            );
                          })()}
                        </ul>
                      </div>
                    </Show>
                  </div>

                  {/* Redirects */}
                  <Show when={hasRedirects}>
                    <div class="section-card" data-card>
                      <button class="section-card__header" onClick={() => setOpenRedirects(!openRedirects())} aria-expanded={openRedirects()} aria-controls="section-redirects-body">
                        <span class="section-card__status section-card__status--skip" />
                        <span class="section-card__title">Redirects</span>
                        <span class="section-card__badges">
                          <span class="badge badge--skip">{data.redirects.length} hop{data.redirects.length !== 1 ? 's' : ''}</span>
                        </span>
                        <span class="section-card__spacer" />
                        <span class={`section-card__chevron${openRedirects() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                      </button>
                      <Show when={openRedirects()}>
                        <div class="section-card__body" id="section-redirects-body">
                          <RedirectChain
                            redirects={data.redirects}
                            finalUrl={data.final_url}
                            httpUpgrade={data.http_upgrade}
                          />
                        </div>
                      </Show>
                    </div>
                  </Show>

                  {/* Security Headers */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenSecurity(!openSecurity())} aria-expanded={openSecurity()} aria-controls="section-security-body">
                      <span class={`section-card__status section-card__status--${securityWorst(data.security)}`} />
                      <span class="section-card__title">Security Headers</span>
                      <span class="section-card__badges">
                        {(() => {
                          const counts = countChips(securityStatuses(data.security));
                          return (
                            <>
                              <Show when={(counts.pass ?? 0) > 0}><span class="badge badge--pass">{counts.pass} passed</span></Show>
                              <Show when={(counts.skip ?? 0) > 0}><span class="badge badge--skip">{counts.skip} skipped</span></Show>
                              <Show when={(counts.warn ?? 0) > 0}><span class="badge badge--warn">{counts.warn} warnings</span></Show>
                              <Show when={(counts.fail ?? 0) > 0}><span class="badge badge--fail">{counts.fail} failed</span></Show>
                            </>
                          );
                        })()}
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openSecurity() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openSecurity()}>
                      <div class="section-card__body" id="section-security-body">
                        <SecurityAudit
                          security={data.security}
                          qualityChecks={data.quality.checks}
                          showExplanations={showExplanations}
                        />
                      </div>
                    </Show>
                  </div>

                  {/* CSP */}
                  <Show when={hasCsp}>
                    <div class="section-card" data-card>
                      <button class="section-card__header" onClick={() => setOpenCsp(!openCsp())} aria-expanded={openCsp()} aria-controls="section-csp-body">
                        <span class={`section-card__status section-card__status--${data.security.csp.status}`} />
                        <span class="section-card__title">
                          Content Security Policy
                          {data.security.csp.report_only ? ' (Report-Only)' : ''}
                        </span>
                        <span class="section-card__badges">
                          <span class={verdictClass(data.security.csp.status)}>{data.security.csp.status}</span>
                          <Show when={data.security.csp.issues.length > 0}>
                            <span class="badge badge--warn">{data.security.csp.issues.length} issue{data.security.csp.issues.length !== 1 ? 's' : ''}</span>
                          </Show>
                        </span>
                        <Show when={!openCsp() && data.security.csp.issues.length > 0}>
                          <span class="section-card__summary">
                            {data.security.csp.issues.slice(0, 2).join(' · ')}
                            {data.security.csp.issues.length > 2 ? ` · +${data.security.csp.issues.length - 2} more` : ''}
                          </span>
                        </Show>
                        <span class="section-card__spacer" />
                        <span class={`section-card__chevron${openCsp() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                      </button>
                      <Show when={openCsp()}>
                        <div class="section-card__body" id="section-csp-body">
                          <CspAnalysis csp={data.security.csp} />
                        </div>
                      </Show>
                    </div>
                  </Show>

                  {/* CORS */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenCors(!openCors())} aria-expanded={openCors()} aria-controls="section-cors-body">
                      <span class={`section-card__status section-card__status--${data.cors.status}`} />
                      <span class="section-card__title">CORS</span>
                      <span class="section-card__badges">
                        <span class={verdictClass(data.cors.status)}>{data.cors.status}</span>
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openCors() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openCors()}>
                      <div class="section-card__body" id="section-cors-body">
                        <CorsReport
                          cors={data.cors}
                          explanation={data.quality.checks.find(c => c.name === 'cors')?.explanation}
                          showExplanations={showExplanations}
                        />
                      </div>
                    </Show>
                  </div>

                  {/* Cookies */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenCookies(!openCookies())} aria-expanded={openCookies()} aria-controls="section-cookies-body">
                      <span class="section-card__status section-card__status--skip" />
                      <span class="section-card__title">Cookies</span>
                      <span class="section-card__badges">
                        <Show when={hasCookies} fallback={<span class="badge badge--skip">none</span>}>
                          <span class="badge badge--skip">{data.cookies.length}</span>
                        </Show>
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openCookies() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openCookies()}>
                      <div class="section-card__body" id="section-cookies-body">
                        <Show when={hasCookies} fallback={<p style="color: var(--text-muted); margin: 0; font-size: 0.875rem;">No Set-Cookie headers — no cookies set by this URL.</p>}>
                          <CookieInspector cookies={data.cookies} />
                        </Show>
                      </div>
                    </Show>
                  </div>

                  {/* Caching + CDN */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenCaching(!openCaching())} aria-expanded={openCaching()} aria-controls="section-caching-body">
                      {(() => {
                        const check = data.quality.checks.find(c => c.name === 'caching');
                        const st = check?.status ?? 'skip';
                        return <span class={`section-card__status section-card__status--${st}`} />;
                      })()}
                      <span class="section-card__title">Caching</span>
                      <span class="section-card__badges">
                        {(() => {
                          const check = data.quality.checks.find(c => c.name === 'caching');
                          if (!check) return null;
                          return <span class={verdictClass(check.status)}>{check.status}</span>;
                        })()}
                      </span>
                      {(() => {
                        const check = data.quality.checks.find(c => c.name === 'caching');
                        if (!openCaching() && check?.message && (check.status === 'warn' || check.status === 'fail')) {
                          return <span class="section-card__summary">{check.message}</span>;
                        }
                        return null;
                      })()}
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openCaching() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openCaching()}>
                      <div class="section-card__body" id="section-caching-body">
                        <CachingView
                          caching={data.caching}
                          cdn={data.cdn}
                          check={data.quality.checks.find(c => c.name === 'caching')}
                          showExplanations={showExplanations}
                        />
                      </div>
                    </Show>
                  </div>

                  {/* Fingerprint + deprecated */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenFingerprint(!openFingerprint())} aria-expanded={openFingerprint()} aria-controls="section-fingerprint-body">
                      <span class={`section-card__status section-card__status--${data.fingerprint.info_leakage.status}`} />
                      <span class="section-card__title">Server Fingerprint</span>
                      <span class="section-card__badges">
                        <span class={verdictClass(data.fingerprint.info_leakage.status)}>{data.fingerprint.info_leakage.status}</span>
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openFingerprint() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openFingerprint()}>
                      <div class="section-card__body" id="section-fingerprint-body">
                        <FingerprintView
                          fingerprint={data.fingerprint}
                          deprecatedHeaders={data.deprecated_headers}
                        />
                      </div>
                    </Show>
                  </div>

                  {/* Response Headers */}
                  <div class="section-card" data-card>
                    <button class="section-card__header" onClick={() => setOpenHeaders(!openHeaders())} aria-expanded={openHeaders()} aria-controls="section-headers-body">
                      <span class="section-card__status section-card__status--skip" />
                      <span class="section-card__title">Response Headers</span>
                      <span class="section-card__badges">
                        <span class="badge badge--skip">{Object.keys(data.headers).length}</span>
                      </span>
                      <span class="section-card__spacer" />
                      <span class={`section-card__chevron${openHeaders() ? ' section-card__chevron--open' : ''}`}>&#9660;</span>
                    </button>
                    <Show when={openHeaders()}>
                      <div class="section-card__body" id="section-headers-body">
                        <HeadersView headers={data.headers} />
                      </div>
                    </Show>
                  </div>
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
          <div class="help-section__title">About</div>
          <p class="help-desc">
            spectra inspects HTTP response headers, security posture, cookies, caching, CORS policy,
            and CSP configuration for any URL. Fires three probes in parallel: HTTPS chain, HTTP
            port-80 redirect, and CORS with an evil origin.{' '}
            <a href="https://netray.info/guide/" target="_blank" rel="noopener noreferrer">Reference guides ↗</a>
          </p>
        </div>

        <div class="help-section">
          <div class="help-section__title">Keyboard shortcuts</div>
          <table class="shortcuts-table">
            <thead>
              <tr><th>Key</th><th>Action</th></tr>
            </thead>
            <tbody>
              <tr><td class="shortcut-key">/</td><td>Focus input</td></tr>
              <tr><td class="shortcut-key">Enter</td><td>Submit URL (when input focused)</td></tr>
              <tr><td class="shortcut-key">r</td><td>Re-run last inspection</td></tr>
              <tr><td class="shortcut-key">e</td><td>Toggle explanations</td></tr>
              <tr><td class="shortcut-key">j / k</td><td>Navigate result sections</td></tr>
              <tr><td class="shortcut-key">Enter</td><td>Expand / collapse active section</td></tr>
              <tr><td class="shortcut-key">Ctrl+L</td><td>Focus input</td></tr>
              <tr><td class="shortcut-key">Escape</td><td>Blur input / close help</td></tr>
              <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
            </tbody>
          </table>
        </div>
      </Modal>

      <SiteFooter
        aboutText={
          <>
            <em>spectra</em> inspects HTTP response headers, security posture, cookies, caching,
            CORS policy, and CSP configuration for any URL. Built in{' '}
            <a href="https://www.rust-lang.org" target="_blank" rel="noopener noreferrer">Rust</a> with{' '}
            <a href="https://github.com/tokio-rs/axum" target="_blank" rel="noopener noreferrer">Axum</a> and{' '}
            <a href="https://www.solidjs.com" target="_blank" rel="noopener noreferrer">SolidJS</a>.
            Open to use — rate limiting applies. Part of the{' '}
            <a href="https://netray.info"><strong>netray.info</strong></a> suite.
          </>
        }
        links={[
          { href: 'https://github.com/netray-info/spectra', label: 'GitHub', external: true },
          { href: '/docs', label: 'API Docs' },
          { href: 'https://lukas.pustina.de', label: 'Author', external: true },
        ]}
        version={meta()?.version}
      />
      </div>
  );
}
