import { createSignal, createEffect, For, Show } from 'solid-js';
import Explain from './Explain';
import type { CertInfo } from '../lib/types';
import { certDisplayName } from '../lib/cert';

function CertTimeline(props: { cert: CertInfo }) {
  const notBefore = () => new Date(props.cert.not_before);
  const notAfter = () => new Date(props.cert.not_after);
  const now = new Date();

  const totalMs = () => notAfter().getTime() - notBefore().getTime();
  const elapsedMs = () => Math.min(Math.max(now.getTime() - notBefore().getTime(), 0), totalMs());
  const elapsedPct = () => totalMs() > 0 ? (elapsedMs() / totalMs()) * 100 : 0;
  const todayPct = () => Math.min(Math.max(elapsedPct(), 0), 100);

  const colorClass = () => {
    if (props.cert.is_expired) return 'cert-timeline__fill--expired';
    if (props.cert.days_remaining <= 30) return 'cert-timeline__fill--warn';
    return 'cert-timeline__fill--ok';
  };

  const fmt = (d: Date) =>
    `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

  return (
    <div class="cert-timeline">
      <div class="cert-timeline__bar">
        <div class={`cert-timeline__fill ${colorClass()}`} style={{ width: `${elapsedPct()}%` }} />
        <div class="cert-timeline__marker" style={{ left: `${todayPct()}%` }} />
      </div>
      <div class="cert-timeline__labels">
        <span class="cert-timeline__label">{fmt(notBefore())}</span>
        <span class="cert-timeline__label">{fmt(notAfter())}</span>
      </div>
    </div>
  );
}

interface Props {
  cert: CertInfo;
  expanded?: boolean;
  explain?: boolean;
  dnsUrl?: string | null;
  host?: string;
  port?: number;
}

/** Returns true if the SAN looks like a DNS hostname (not an IP address or email). */
function isDnsSan(san: string): boolean {
  if (san.includes('@')) return false;
  // IPv4: four decimal octets
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(san)) return false;
  // IPv6: contains colons
  if (san.includes(':')) return false;
  return true;
}

function CopyBtn(props: { value: string }) {
  const [copied, setCopied] = createSignal(false);
  const copy = () => {
    navigator.clipboard.writeText(props.value);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <button class="copy-btn" onClick={copy} aria-label="Copy" title="Copy">
      {copied() ? '\u2713' : '\u29c9'}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Cert history (Feature 5: localStorage-based change diff)
// ---------------------------------------------------------------------------

interface StoredCertSnapshot {
  fingerprint: string;
  not_after: string;
  sans: string[];
  issuer: string;
}

function certHistoryKey(host: string, port: number): string {
  return `tlsight:cert:${host}:${port}`;
}

function loadStoredSnapshot(host: string, port: number): StoredCertSnapshot | null {
  try {
    const raw = localStorage.getItem(certHistoryKey(host, port));
    return raw ? (JSON.parse(raw) as StoredCertSnapshot) : null;
  } catch {
    return null;
  }
}

function saveSnapshot(host: string, port: number, cert: CertInfo) {
  try {
    const snap: StoredCertSnapshot = {
      fingerprint: cert.fingerprint_sha256,
      not_after: cert.not_after,
      sans: cert.sans.slice().sort(),
      issuer: cert.issuer,
    };
    localStorage.setItem(certHistoryKey(host, port), JSON.stringify(snap));
  } catch {
    // Storage may be unavailable; ignore
  }
}

function clearSnapshot(host: string, port: number) {
  try {
    localStorage.removeItem(certHistoryKey(host, port));
  } catch {}
}

interface CertDiff {
  expiry_changed: boolean;
  prev_expiry: string;
  cur_expiry: string;
  issuer_changed: boolean;
  prev_issuer: string;
  cur_issuer: string;
  sans_added: string[];
  sans_removed: string[];
}

function computeDiff(prev: StoredCertSnapshot, cur: CertInfo): CertDiff | null {
  const curSansSorted = cur.sans.slice().sort();
  const prevSansSorted = prev.sans.slice().sort();

  const expiry_changed = prev.not_after !== cur.not_after;
  const issuer_changed = prev.issuer !== cur.issuer;
  const sans_added = curSansSorted.filter(s => !prevSansSorted.includes(s));
  const sans_removed = prevSansSorted.filter(s => !curSansSorted.includes(s));

  if (!expiry_changed && !issuer_changed && sans_added.length === 0 && sans_removed.length === 0) {
    return null;
  }

  return {
    expiry_changed,
    prev_expiry: prev.not_after,
    cur_expiry: cur.not_after,
    issuer_changed,
    prev_issuer: prev.issuer,
    cur_issuer: cur.issuer,
    sans_added,
    sans_removed,
  };
}

function CertChangeBanner(props: { diff: CertDiff; onClear: () => void }) {
  return (
    <div class="cert-change-banner" role="alert">
      <div class="cert-change-banner__header">
        <strong>Certificate changed since last visit</strong>
        <button class="cert-change-banner__clear" onClick={props.onClear} title="Clear history">clear</button>
      </div>
      <table class="cert-change-banner__table">
        <tbody>
          <Show when={props.diff.expiry_changed}>
            <tr>
              <th>Expiry</th>
              <td><del>{props.diff.prev_expiry}</del> &rarr; {props.diff.cur_expiry}</td>
            </tr>
          </Show>
          <Show when={props.diff.issuer_changed}>
            <tr>
              <th>Issuer</th>
              <td><del>{props.diff.prev_issuer}</del> &rarr; {props.diff.cur_issuer}</td>
            </tr>
          </Show>
          <Show when={props.diff.sans_added.length > 0}>
            <tr>
              <th>SANs added</th>
              <td class="mono cert-change-banner__added">{props.diff.sans_added.join(', ')}</td>
            </tr>
          </Show>
          <Show when={props.diff.sans_removed.length > 0}>
            <tr>
              <th>SANs removed</th>
              <td class="mono cert-change-banner__removed">{props.diff.sans_removed.join(', ')}</td>
            </tr>
          </Show>
        </tbody>
      </table>
    </div>
  );
}

export default function CertDetail(props: Props) {
  const [expanded, setExpanded] = createSignal(props.expanded ?? false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  // Cert change diff: only for leaf certs with host/port context
  const isLeaf = () =>
    props.cert.position === 'leaf' || props.cert.position === 'leaf_self_signed';

  const [diff, setDiff] = createSignal<CertDiff | null>(null);
  const [historyHost, setHistoryHost] = createSignal('');
  const [historyPort, setHistoryPort] = createSignal(0);

  createEffect(() => {
    const host = props.host;
    const port = props.port;
    if (!host || !port || !isLeaf()) return;

    setHistoryHost(host);
    setHistoryPort(port);

    const prev = loadStoredSnapshot(host, port);
    if (prev) {
      if (prev.fingerprint !== props.cert.fingerprint_sha256) {
        setDiff(computeDiff(prev, props.cert));
      } else {
        setDiff(null);
      }
    }
    // Always save current snapshot
    saveSnapshot(host, port, props.cert);
  });

  const handleClearHistory = () => {
    clearSnapshot(historyHost(), historyPort());
    setDiff(null);
  };

  return (
    <div class="cert-detail" data-card>
      <Show when={diff()}>
        {(d) => <CertChangeBanner diff={d()} onClear={handleClearHistory} />}
      </Show>
      <button class="cert-detail__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
        {expanded() ? '\u25BC' : '\u25B6'} {certDisplayName(props.cert.subject)} ({props.cert.position})
      </button>
      {expanded() && (
        <div class="cert-detail__body">
          <Explain when={!!props.explain}>Full details for this certificate. SANs list all hostnames this certificate covers. The SHA-256 fingerprint uniquely identifies this certificate.</Explain>
          <div class="cert-table-scroll">
          <table class="cert-detail__table">
            <tbody>
              <tr>
                <th>Subject</th>
                <td>{props.cert.subject} <CopyBtn value={props.cert.subject} /></td>
              </tr>
              <tr>
                <th>Issuer</th>
                <td>{props.cert.issuer} <CopyBtn value={props.cert.issuer} /></td>
              </tr>
              <tr><th>Policy</th><td>{props.cert.cert_policy}</td></tr>
              <tr><th>SANs</th><td class="mono">
                <Show when={props.cert.sans.length > 0} fallback="none">
                  <For each={props.cert.sans}>
                    {(san, i) => (
                      <>
                        {i() > 0 ? ', ' : ''}
                        {san}
                        {props.dnsUrl && isDnsSan(san) && (
                          <>
                            {' '}
                            <a
                              class="eco-link eco-link--badge"
                              href={`${props.dnsUrl}/?q=${encodeURIComponent(san)}&ref=tlsight`}
                              target="_blank"
                              rel="noopener noreferrer"
                              title={`Inspect DNS for ${san}`}
                            >DNS &#x2197;</a>
                          </>
                        )}
                      </>
                    )}
                  </For>
                  {' '}
                  <CopyBtn value={props.cert.sans.join(', ')} />
                </Show>
              </td></tr>
              <tr>
                <th>Serial</th>
                <td class="mono">{props.cert.serial} <CopyBtn value={props.cert.serial} /></td>
              </tr>
              <tr>
                <th>Valid</th>
                <td>
                  {props.cert.not_before} — {props.cert.not_after}
                  <CertTimeline cert={props.cert} />
                </td>
              </tr>
              <tr><th>Days remaining</th><td>{props.cert.days_remaining}</td></tr>
              <tr><th>Key</th><td>{props.cert.key_type} {props.cert.key_size}</td></tr>
              <tr><th>Signature</th><td>{props.cert.signature_algorithm}</td></tr>
              <tr>
                <th>SHA-256</th>
                <td class="mono">
                  {props.cert.fingerprint_sha256}
                  {' '}
                  <a
                    class="eco-link eco-link--badge"
                    href={`https://crt.sh/?q=${props.cert.fingerprint_sha256.replace(/:/g, '')}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >crt.sh &#x2197;</a>
                  {' '}
                  <CopyBtn value={props.cert.fingerprint_sha256} />
                </td>
              </tr>
              <tr>
                <th>SHA-1</th>
                <td class="mono">{props.cert.fingerprint_sha1} <CopyBtn value={props.cert.fingerprint_sha1} /></td>
              </tr>
              <tr><th>Self-signed</th><td>{props.cert.is_self_signed ? 'yes' : 'no'}</td></tr>
              <tr><th>Expired</th><td>{props.cert.is_expired ? 'yes' : 'no'}</td></tr>
              <Show when={props.cert.ocsp_url}>
                <tr>
                  <th>OCSP URL</th>
                  <td class="mono">
                    <a href={props.cert.ocsp_url} target="_blank" rel="noopener noreferrer">
                      {props.cert.ocsp_url}
                    </a>
                  </td>
                </tr>
              </Show>
              <Show when={props.cert.ca_issuers_url}>
                <tr>
                  <th>CA Issuers URL</th>
                  <td class="mono">
                    <a href={props.cert.ca_issuers_url} target="_blank" rel="noopener noreferrer">
                      {props.cert.ca_issuers_url}
                    </a>
                  </td>
                </tr>
              </Show>
            </tbody>
          </table>
          </div>
        </div>
      )}
    </div>
  );
}
