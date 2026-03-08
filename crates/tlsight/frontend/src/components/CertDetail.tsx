import { createSignal } from 'solid-js';
import type { CertInfo } from '../lib/types';

interface Props {
  cert: CertInfo;
}

export default function CertDetail(props: Props) {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <div class="cert-detail">
      <button class="cert-detail__toggle" onClick={() => setExpanded(!expanded())}>
        {expanded() ? '▼' : '▶'} {props.cert.subject} ({props.cert.position})
      </button>
      {expanded() && (
        <div class="cert-detail__body">
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
