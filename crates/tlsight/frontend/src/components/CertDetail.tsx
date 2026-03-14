import { createSignal, createEffect, For, Show } from 'solid-js';
import Explain from './Explain';
import type { CertInfo } from '../lib/types';
import { certDisplayName } from '../lib/cert';

interface Props {
  cert: CertInfo;
  expanded?: boolean;
  explain?: boolean;
  dnsUrl?: string | null;
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

export default function CertDetail(props: Props) {
  const [expanded, setExpanded] = createSignal(props.expanded ?? false);
  createEffect(() => { if (props.expanded !== undefined) setExpanded(props.expanded); });

  return (
    <div class="cert-detail" data-card>
      <button class="cert-detail__toggle" onClick={() => setExpanded(!expanded())} aria-expanded={expanded() ? "true" : "false"}>
        {expanded() ? '\u25BC' : '\u25B6'} {certDisplayName(props.cert.subject)} ({props.cert.position})
      </button>
      {expanded() && (
        <div class="cert-detail__body">
          <Explain when={!!props.explain}>Full details for this certificate. SANs list all hostnames this certificate covers. The SHA-256 fingerprint uniquely identifies this certificate.</Explain>
          <table class="cert-detail__table">
            <tbody>
              <tr><th>Issuer</th><td>{props.cert.issuer}</td></tr>
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
                            >DNS ↗</a>
                          </>
                        )}
                      </>
                    )}
                  </For>
                </Show>
              </td></tr>
              <tr><th>Serial</th><td class="mono">{props.cert.serial}</td></tr>
              <tr><th>Valid</th><td>{props.cert.not_before} — {props.cert.not_after}</td></tr>
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
                  >crt.sh ↗</a>
                </td>
              </tr>
              <tr><th>SHA-1</th><td class="mono">{props.cert.fingerprint_sha1}</td></tr>
              <tr><th>Self-signed</th><td>{props.cert.is_self_signed ? 'yes' : 'no'}</td></tr>
              <tr><th>Expired</th><td>{props.cert.is_expired ? 'yes' : 'no'}</td></tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
