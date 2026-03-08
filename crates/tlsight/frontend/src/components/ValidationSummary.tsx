import Explain from './Explain';
import type { Summary, CheckStatus } from '../lib/types';

interface Props {
  summary: Summary;
  explain?: boolean;
}

const CHECK_LABELS: Record<string, string> = {
  chain_trusted: 'Chain trusted',
  not_expired: 'Not expired',
  hostname_match: 'Hostname match',
  caa_compliant: 'CAA compliant',
  dane_valid: 'DANE valid',
  ct_logged: 'CT logged',
  ocsp_stapled: 'OCSP stapled',
  consistency: 'Consistency',
};

const CHECK_EXPLANATIONS: Record<string, string> = {
  chain_trusted: 'The certificate chain is complete and all signatures verify up to a trusted root CA.',
  not_expired: 'No certificate in the chain has expired or is not yet valid.',
  hostname_match: 'The leaf certificate\'s SANs (Subject Alternative Names) include the queried hostname.',
  caa_compliant: 'The issuing CA is authorized by the domain\'s CAA DNS records (or no CAA records exist).',
  dane_valid: 'TLSA records in DNS match the presented certificate (requires DNSSEC).',
  ct_logged: 'The certificate has Signed Certificate Timestamps (SCTs) proving it was logged in CT logs.',
  ocsp_stapled: 'The server includes an OCSP response, allowing clients to check revocation without contacting the CA.',
  consistency: 'All IP addresses for this hostname serve the same certificate and TLS configuration.',
};

const STATUS_ICON: Record<CheckStatus, string> = {
  pass: '*',
  warn: '!',
  fail: 'X',
  skip: '-',
};

export default function ValidationSummary(props: Props) {
  return (
    <div class="validation-summary">
      <h2 class="validation-summary__title">
        Validation: <span class={`verdict verdict--${props.summary.verdict}`}>{props.summary.verdict}</span>
      </h2>
      <Explain when={!!props.explain}>This is the overall health check. Green = good, orange = warning, red = problem, grey = skipped.</Explain>
      <div class="validation-summary__checks">
        {Object.entries(props.summary.checks).map(([key, status]) => (
          <span class={`check check--${status}`} title={CHECK_EXPLANATIONS[key]}>
            {STATUS_ICON[status]} {CHECK_LABELS[key] ?? key}
          </span>
        ))}
      </div>
      <Explain when={!!props.explain}>
        <dl class="check-explanations">
          {Object.entries(props.summary.checks).map(([key]) => (
            <>
              <dt>{CHECK_LABELS[key] ?? key}</dt>
              <dd>{CHECK_EXPLANATIONS[key]}</dd>
            </>
          ))}
        </dl>
      </Explain>
    </div>
  );
}
