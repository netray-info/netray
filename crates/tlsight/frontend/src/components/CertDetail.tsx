import { createSignal, createEffect } from 'solid-js';
import Explain from './Explain';
import type { CertInfo } from '../lib/types';
import { certDisplayName } from '../lib/cert';

interface Props {
  cert: CertInfo;
  expanded?: boolean;
  explain?: boolean;
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
              <tr><th>SANs</th><td class="mono">{props.cert.sans.join(', ') || 'none'}</td></tr>
              <tr><th>Serial</th><td class="mono">{props.cert.serial}</td></tr>
              <tr><th>Valid</th><td>{props.cert.not_before} — {props.cert.not_after}</td></tr>
              <tr><th>Days remaining</th><td>{props.cert.days_remaining}</td></tr>
              <tr><th>Key</th><td>{props.cert.key_type} {props.cert.key_size}</td></tr>
              <tr><th>Signature</th><td>{props.cert.signature_algorithm}</td></tr>
              <tr><th>SHA-256</th><td class="mono">{props.cert.fingerprint_sha256}</td></tr>
              <tr><th>Self-signed</th><td>{props.cert.is_self_signed ? 'yes' : 'no'}</td></tr>
              <tr><th>Expired</th><td>{props.cert.is_expired ? 'yes' : 'no'}</td></tr>
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
