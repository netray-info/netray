import type { Summary, CheckStatus } from '../lib/types';

interface Props {
  summary: Summary;
}

const CHECK_LABELS: Record<string, string> = {
  chain_trusted: 'Chain trusted',
  not_expired: 'Not expired',
  hostname_match: 'Hostname match',
  caa_compliant: 'CAA compliant',
  dane_valid: 'DANE valid',
  ocsp_stapled: 'OCSP stapled',
  consistency: 'Consistency',
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
      <div class="validation-summary__checks">
        {Object.entries(props.summary.checks).map(([key, status]) => (
          <span class={`check check--${status}`}>
            {STATUS_ICON[status]} {CHECK_LABELS[key] ?? key}
          </span>
        ))}
      </div>
    </div>
  );
}
