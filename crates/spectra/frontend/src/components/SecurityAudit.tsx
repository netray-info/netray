import { Show } from 'solid-js';
import type { Accessor } from 'solid-js';
import type { SecurityReport, HeaderCheck, CheckStatus, QualityCheck } from '../lib/types';

function StatusBadge(props: { status: CheckStatus }) {
  return <span class={`badge badge--${props.status}`}>{props.status}</span>;
}

function rowClass(status: CheckStatus, explainable: boolean): string {
  const base = status === 'fail' ? 'check-list__item check-row--fail' :
               status === 'warn' ? 'check-list__item check-row--warn' :
               'check-list__item';
  return explainable ? `${base} check-list__item--explainable` : base;
}

function HeaderRow(props: {
  name: string;
  check: HeaderCheck;
  explanation?: string;
  showExplanations: Accessor<boolean>;
}) {
  return (
    <li class={rowClass(props.check.status, props.showExplanations())}>
      <StatusBadge status={props.check.status} />
      <span class="check-list__name">{props.name}</span>
      <span class="check-list__message">
        {props.check.value ?? props.check.message ?? ''}
      </span>
      <Show when={props.showExplanations() && props.explanation}>
        <span class="check-explain">{props.explanation}</span>
      </Show>
    </li>
  );
}

interface Props {
  security: SecurityReport;
  qualityChecks: QualityCheck[];
  showExplanations: Accessor<boolean>;
}

export default function SecurityAudit(props: Props) {
  const s = () => props.security;

  function explanationFor(name: string): string | undefined {
    return props.qualityChecks.find(c => c.name === name)?.explanation;
  }

  return (
    <ul class="check-list">
      <li class={rowClass(s().hsts.status, props.showExplanations())}>
        <StatusBadge status={s().hsts.status} />
        <span class="check-list__name">HSTS</span>
        <span class="check-list__message">
          {s().hsts.max_age != null
            ? `max-age=${s().hsts.max_age}${s().hsts.include_sub_domains ? '; includeSubDomains' : ''}${s().hsts.preload ? '; preload' : ''}`
            : 'Not set'}
        </span>
        <Show when={props.showExplanations() && explanationFor('hsts')}>
          <span class="check-explain">{explanationFor('hsts')}</span>
        </Show>
      </li>
      <HeaderRow name="X-Frame-Options" check={s().x_frame_options} explanation={explanationFor('x_frame_options')} showExplanations={props.showExplanations} />
      <HeaderRow name="X-Content-Type-Options" check={s().x_content_type_options} explanation={explanationFor('x_content_type_options')} showExplanations={props.showExplanations} />
      <HeaderRow name="Referrer-Policy" check={s().referrer_policy} explanation={explanationFor('referrer_policy')} showExplanations={props.showExplanations} />
      <HeaderRow name="Permissions-Policy" check={s().permissions_policy} explanation={explanationFor('permissions_policy')} showExplanations={props.showExplanations} />
      <HeaderRow name="COOP" check={s().coop} explanation={undefined} showExplanations={props.showExplanations} />
      <HeaderRow name="COEP" check={s().coep} explanation={undefined} showExplanations={props.showExplanations} />
      <HeaderRow name="CORP" check={s().corp} explanation={undefined} showExplanations={props.showExplanations} />
    </ul>
  );
}
