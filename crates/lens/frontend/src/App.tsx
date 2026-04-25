import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createTheme } from '@netray-info/common-frontend/theme';
import ThemeToggle from '@netray-info/common-frontend/components/ThemeToggle';
import SiteFooter from '@netray-info/common-frontend/components/SiteFooter';
import Modal from '@netray-info/common-frontend/components/Modal';
import { createKeyboardShortcuts } from '@netray-info/common-frontend/keyboard';
import SuiteNav from '@netray-info/common-frontend/components/SuiteNav';
import DomainInput from './components/DomainInput';
import DnsSection from './components/DnsSection';
import TlsSection from './components/TlsSection';
import HttpSection from './components/HttpSection';
import EmailSection from './components/EmailSection';
import IpSection from './components/IpSection';
import Landing from './components/Landing';
import DeeperCallout from './components/DeeperCallout';
import Summary from './components/Summary';
import { startCheck } from './lib/sse';
import { fetchMeta } from './lib/api';
import { addToHistory } from './lib/history';
import { toJson, toMarkdown } from './lib/export';
import { CHECK_LABELS, CHECK_DESCRIPTIONS } from './lib/checkMeta';
import type {
  CheckState,
  DnsEvent,
  TlsEvent,
  HttpEvent,
  EmailEvent,
  IpEvent,
  SummaryEvent,
  DoneEvent,
  MetaResponse,
} from './lib/types';

export default function App() {
  const themeResult = createTheme('lens_theme', 'system');

  const [meta, setMeta] = createSignal<MetaResponse | null>(null);
  const [showHelp, setShowHelp] = createSignal(false);
  const [checkState, setCheckState] = createSignal<CheckState>('idle');
  const [dns, setDns] = createSignal<DnsEvent | null>(null);
  const [tls, setTls] = createSignal<TlsEvent | null>(null);
  const [http, setHttp] = createSignal<HttpEvent | null>(null);
  const [email, setEmail] = createSignal<EmailEvent | null>(null);
  const [ip, setIp] = createSignal<IpEvent | null>(null);
  const [summary, setSummary] = createSignal<SummaryEvent | null>(null);
  const [done, setDone] = createSignal<DoneEvent | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [currentDomain, setCurrentDomain] = createSignal('');
  const [toastMsg, setToastMsg] = createSignal<string | null>(null);
  const [allExpanded, setAllExpanded] = createSignal<boolean | undefined>(undefined);

  let inputEl: HTMLInputElement | undefined;
  let ssCleanup: (() => void) | null = null;

  onMount(() => {
    fetchMeta().then(m => {
      setMeta(m);
      const title = m?.features?.site?.title ?? m?.site_name;
      if (title) document.title = title;
    });

    const params = new URLSearchParams(window.location.search);
    const initialDomain = params.get('d');
    if (initialDomain) handleSubmit(initialDomain);

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
      '/':      (e) => { e.preventDefault(); inputEl?.focus(); },
      '?':      (e) => { e.preventDefault(); setShowHelp(v => !v); },
      'r':      (e) => {
        const d = currentDomain();
        if (d && checkState() !== 'loading') { e.preventDefault(); handleSubmit(d); }
      },
      'j':      navigateCards,
      'k':      navigateCards,
      'Enter':  expandActiveCard,
      'Escape': () => setShowHelp(false),
    });

    onCleanup(() => {
      cleanupShortcuts();
      document.removeEventListener('mousedown', clearCardActive);
    });
  });

  function clearState() {
    setDns(null);
    setTls(null);
    setHttp(null);
    setEmail(null);
    setIp(null);
    setSummary(null);
    setDone(null);
    setError(null);
  }

  function handleSubmit(domain: string) {
    if (ssCleanup) { ssCleanup(); ssCleanup = null; }
    clearState();
    setAllExpanded(undefined);
    setCurrentDomain(domain);
    setCheckState('loading');
    addToHistory(domain);

    const url = new URL(window.location.href);
    url.searchParams.set('d', domain);
    window.history.replaceState(null, '', url.toString());

    ssCleanup = startCheck(domain, {
      onDns:     (data) => setDns(data),
      onTls:     (data) => setTls(data),
      onHttp:    (data) => setHttp(data),
      onEmail:   (data) => setEmail(data),
      onIp:      (data) => setIp(data),
      onSummary: (data) => setSummary(data),
      onDone:    (data) => { setDone(data); setCheckState('done'); },
      onError:   (err)  => { setError(err); setCheckState('error'); },
    });
  }

  function handleCopyMd() {
    const md = toMarkdown(currentDomain(), dns(), tls(), ip(), summary(), done());
    navigator.clipboard.writeText(md).then(() => {
      setToastMsg('Copied!');
      setTimeout(() => setToastMsg(null), 2000);
    });
  }

  function handleDownloadJson() {
    const blob = new Blob(
      [toJson(currentDomain(), dns(), tls(), ip(), summary(), done())],
      { type: 'application/json' },
    );
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${currentDomain()}-lens.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function handleClear() {
    if (ssCleanup) { ssCleanup(); ssCleanup = null; }
    clearState();
    setCurrentDomain('');
    setCheckState('idle');
    window.history.replaceState(null, '', window.location.pathname);
  }

  onCleanup(() => { if (ssCleanup) ssCleanup(); });

  const isLoading  = () => checkState() === 'loading';
  const hasResults = () => dns() !== null || tls() !== null || ip() !== null || email() !== null;

  const allChecks = () => [
    ...(tls()?.checks ?? []),
    ...(dns()?.checks ?? []),
    ...(http()?.checks ?? []),
    ...(email()?.checks ?? []),
    ...(ip()?.checks ?? []),
  ];

  return (
    <>
      <a href="#main-content" class="skip-link">Skip to results</a>
      <div class="app">
        <SuiteNav current="lens" meta={meta()?.ecosystem} />

        <header class="header">
          <h1 class="logo">{meta()?.features?.site?.brand_name ?? 'lens'}</h1>
          <span class="tagline">
            {meta()?.features?.site?.brand_tagline ?? 'domains, in focus'}
          </span>
          <div class="header-actions">
            <ThemeToggle theme={themeResult} class="header-btn" />
            <button
              class="header-btn"
              type="button"
              aria-label="Open help"
              onClick={() => setShowHelp(true)}
              title="Help (?)"
            >
              ?
            </button>
          </div>
        </header>

        <main class="main" id="main-content">
          {/* Idle state: Landing places the hero ABOVE the input, then chips
              + legend + trust + callout BELOW it (SDD §3 Requirement 10).
              Non-idle: input renders at the top so the user always sees the
              term they typed when results stream in. */}
          <Show
            when={!hasResults() && !isLoading() && !error()}
            fallback={
              <DomainInput
                onSubmit={handleSubmit}
                onClear={handleClear}
                loading={isLoading()}
                value={currentDomain()}
                inputRef={(el) => { inputEl = el; }}
                showCopyLink={hasResults() || checkState() === 'done' || checkState() === 'error'}
              />
            }
          >
            <Landing
              site={meta()?.features?.site}
              onExampleClick={handleSubmit}
            >
              <DomainInput
                onSubmit={handleSubmit}
                onClear={handleClear}
                loading={isLoading()}
                value={currentDomain()}
                inputRef={(el) => { inputEl = el; }}
                showCopyLink={false}
              />
            </Landing>
          </Show>

          <Show when={error()}>
            <div class="error-banner" role="alert">{error()}</div>
          </Show>

          <Show when={summary()}>
            {(s) => (
              <Summary
                summary={s()}
                done={done()}
                addresses={ip()?.addresses}
                ipDetailUrl={ip()?.detail_url}
                httpServerIp={http()?.server_ip}
                httpServerOrg={http()?.server_org}
                httpServerNetworkType={http()?.server_network_type}
                checks={allChecks()}
                onCopyMd={done() ? handleCopyMd : undefined}
                onDownloadJson={done() ? handleDownloadJson : undefined}
              />
            )}
          </Show>

          <Show when={hasResults() || isLoading()}>
            <div class="chips-row">
              <button
                class="filter-toggle"
                type="button"
                onClick={() => setAllExpanded(v => v === true ? false : true)}
              >{allExpanded() === true ? 'collapse all' : 'expand all'}</button>
            </div>
            <div class="section-grid" role="status" aria-live="polite" aria-label="Check results">
              {/* Top-down: Email → HTTP → TLS → DNS → IP (application layer down to network layer) */}
              <Show when={email() !== null || (isLoading() && summary()?.sections['email'] !== undefined)}>
                <div data-card>
                  <EmailSection
                    data={email()}
                    loading={isLoading() && email() === null}
                    error={error() ?? undefined}
                    expanded={allExpanded()}
                    grade={summary()?.section_grades['email']}
                  />
                </div>
              </Show>
              <Show when={http() !== null || (isLoading() && meta()?.ecosystem?.http_base_url !== undefined)}>
                <div data-card>
                  <HttpSection
                    data={http()}
                    loading={isLoading() && http() === null}
                    error={error() ?? undefined}
                    expanded={allExpanded()}
                    grade={summary()?.section_grades['http']}
                  />
                </div>
              </Show>
              <div data-card>
                <TlsSection
                  data={tls()}
                  loading={isLoading() && tls() === null}
                  error={error() ?? undefined}
                  expanded={allExpanded()}
                  grade={summary()?.section_grades['tls']}
                />
              </div>
              <div data-card>
                <DnsSection
                  data={dns()}
                  loading={isLoading() && dns() === null}
                  error={error() ?? undefined}
                  expanded={allExpanded()}
                  grade={summary()?.section_grades['dns']}
                />
              </div>
              <div data-card>
                <IpSection
                  data={ip()}
                  loading={isLoading() && ip() === null}
                  error={error() ?? undefined}
                  expanded={allExpanded()}
                  grade={summary()?.section_grades['ip']}
                />
              </div>
            </div>
          </Show>

          {/* Deeper-callout persists across idle and result states so users
              who got a failure always have a visible path to /tools. */}
          <Show when={hasResults() || isLoading() || error()}>
            <DeeperCallout />
          </Show>
        </main>

        <SiteFooter
          aboutText={
            meta()?.features?.site?.footer_about ?? (
              <>
                <em>lens</em> checks TLS certificate validity, DNS health, and IP reputation for any
                domain — results stream in as each check completes. Built in{' '}
                <a href="https://www.rust-lang.org" target="_blank" rel="noopener noreferrer">Rust</a> with{' '}
                <a href="https://github.com/tokio-rs/axum" target="_blank" rel="noopener noreferrer">Axum</a> and{' '}
                <a href="https://www.solidjs.com" target="_blank" rel="noopener noreferrer">SolidJS</a>.
                Open to use — rate limiting applies. Part of the{' '}
                <a href="https://netray.info"><strong>netray.info</strong></a> suite.
              </>
            )
          }
          links={
            meta()?.features?.site?.footer_links ?? [
              { href: 'https://github.com/netray-info/lens', label: 'GitHub',   external: true },
              { href: '/docs',                               label: 'API Docs', external: true },
              { href: 'https://lukas.pustina.de',            label: 'Author',   external: true },
            ]
          }
          version={meta()?.version}
        />

        <Show when={toastMsg()}>
          <div class="toast" role="status" aria-live="polite">{toastMsg()}</div>
        </Show>

        <Modal open={showHelp()} onClose={() => setShowHelp(false)} title="Help">
          <div class="help-section">
            <div class="help-section__title">About</div>
            <p class="help-desc">
              lens checks TLS certificate validity, DNS health, and IP reputation for any domain —
              all three run in parallel and stream in as they complete.{' '}
              <a href="https://netray.info/guide/" target="_blank" rel="noopener noreferrer">Reference guides ↗</a>
            </p>
          </div>

          <div class="help-section">
            <div class="help-section__title">Scoring</div>
            <p class="help-desc">
              Weighted average across IP (10%), DNS (20%), TLS (35%), HTTP (20%), and Email (15%).
              HTTP and Email are optional — only scored when the respective backends are configured.
              For domains without MX records, the three email receiving buckets are excluded from scoring automatically.
            </p>
            <Show when={meta()?.features?.profile}>
              {(profile) => (
                <div class="help-scoring">
                  <div class="help-scoring__thresholds">
                    {Object.entries(profile().thresholds)
                      .sort((a, b) => b[1] - a[1])
                      .map(([grade, min]) => (
                        <span class="help-scoring__grade-chip">{grade} ≥ {min}%</span>
                      ))}
                  </div>
                  <div class="help-hard-fail">
                    <p class="help-hard-fail__heading">Hard fail checks — any of these force grade F:</p>
                    <For each={[...profile().hard_fail.tls, ...profile().hard_fail.dns]}>
                      {(name) => (
                        <div class="help-hard-fail__item">
                          <span class="help-hard-fail__label">{CHECK_LABELS[name] ?? name}</span>
                          <Show when={CHECK_DESCRIPTIONS[name]}>
                            <span class="help-hard-fail__desc">{CHECK_DESCRIPTIONS[name]}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </Show>
          </div>

          <div class="help-section">
            <div class="help-section__title">Keyboard shortcuts</div>
            <table class="shortcuts-table">
              <thead>
                <tr><th>Key</th><th>Action</th></tr>
              </thead>
              <tbody>
                <tr><td class="shortcut-key">/</td><td>Focus input</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Submit domain (when input focused)</td></tr>
                <tr><td class="shortcut-key">r</td><td>Re-run last check</td></tr>
                <tr><td class="shortcut-key">j / k</td><td>Navigate result sections</td></tr>
                <tr><td class="shortcut-key">Enter</td><td>Expand / collapse active section</td></tr>
                <tr><td class="shortcut-key">Escape</td><td>Close help</td></tr>
                <tr><td class="shortcut-key">?</td><td>Toggle this help</td></tr>
              </tbody>
            </table>
          </div>
        </Modal>
      </div>
    </>
  );
}
