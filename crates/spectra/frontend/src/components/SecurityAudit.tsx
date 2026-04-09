import type { SecurityReport, HeaderCheck, CheckStatus } from '../lib/types';

function StatusBadge(props: { status: CheckStatus }) {
  return <span class={`badge badge--${props.status}`}>{props.status}</span>;
}

function rowClass(status: CheckStatus): string {
  if (status === 'fail') return 'check-list__item check-row--fail';
  if (status === 'warn') return 'check-list__item check-row--warn';
  return 'check-list__item';
}

function HeaderRow(props: { name: string; check: HeaderCheck }) {
  return (
    <li class={rowClass(props.check.status)}>
      <StatusBadge status={props.check.status} />
      <span class="check-list__name">{props.name}</span>
      <span class="check-list__message">
        {props.check.value ?? props.check.message ?? ''}
      </span>
    </li>
  );
}

interface Props {
  security: SecurityReport;
}

export default function SecurityAudit(props: Props) {
  const s = () => props.security;

  return (
    <ul class="check-list">
      <li class={rowClass(s().hsts.status)}>
        <StatusBadge status={s().hsts.status} />
        <span class="check-list__name">HSTS</span>
        <span class="check-list__message">
          {s().hsts.max_age != null
            ? `max-age=${s().hsts.max_age}${s().hsts.include_sub_domains ? '; includeSubDomains' : ''}${s().hsts.preload ? '; preload' : ''}`
            : 'Not set'}
        </span>
      </li>
      <li class={rowClass(s().csp.status)}>
        <StatusBadge status={s().csp.status} />
        <span class="check-list__name">CSP</span>
        <span class="check-list__message">
          {s().csp.enforced ? 'Enforced' : s().csp.report_only ? 'Report-only' : 'Missing'}
          {s().csp.issues.length > 0 ? ` — ${s().csp.issues.length} issue(s)` : ''}
        </span>
      </li>
      <HeaderRow name="X-Frame-Options" check={s().x_frame_options} />
      <HeaderRow name="X-Content-Type-Options" check={s().x_content_type_options} />
      <HeaderRow name="Referrer-Policy" check={s().referrer_policy} />
      <HeaderRow name="Permissions-Policy" check={s().permissions_policy} />
      <HeaderRow name="COOP" check={s().coop} />
      <HeaderRow name="COEP" check={s().coep} />
      <HeaderRow name="CORP" check={s().corp} />
    </ul>
  );
}
