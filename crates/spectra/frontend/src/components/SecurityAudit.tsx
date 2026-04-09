import { For } from 'solid-js';
import type { SecurityReport, HeaderCheck, CheckStatus } from '../lib/types';

function StatusBadge(props: { status: CheckStatus }) {
  return <span class={`badge badge--${props.status}`}>{props.status}</span>;
}

function HeaderRow(props: { name: string; check: HeaderCheck }) {
  return (
    <li class="check-list__item">
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
    <div class="section">
      <div class="section__title">Security Headers</div>
      <ul class="check-list">
        <li class="check-list__item">
          <StatusBadge status={s().hsts.status} />
          <span class="check-list__name">HSTS</span>
          <span class="check-list__message">
            {s().hsts.max_age != null
              ? `max-age=${s().hsts.max_age}${s().hsts.include_sub_domains ? '; includeSubDomains' : ''}${s().hsts.preload ? '; preload' : ''}`
              : 'Not set'}
          </span>
        </li>
        <li class="check-list__item">
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
    </div>
  );
}
