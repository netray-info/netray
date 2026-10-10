import { Show, For } from 'solid-js';
import { categoryHeaderLink } from './helpers';
import type { Ecosystem } from './helpers';
import type { CheckResult, IpEnrichment, Verdict } from '../lib/types';
import {
  CATEGORY_LABELS, CATEGORY_EXPLANATIONS,
  subCheckLabel, subCheckExplanation,
} from '../lib/types';

interface CategoryCardProps {
  result: CheckResult;
  open: boolean;
  onToggle: () => void;
  showExplanations: boolean;
  domain: string;
  ecosystem?: Ecosystem;
}

export default function CategoryCard(props: CategoryCardProps) {
  const headerLink = () =>
    categoryHeaderLink(props.result.category, props.domain, props.ecosystem, props.result);
  const explanation = () => CATEGORY_EXPLANATIONS[props.result.category];
  const counts = () => {
    const c: Partial<Record<Verdict, number>> = {};
    for (const sc of props.result.sub_checks) {
      c[sc.verdict] = (c[sc.verdict] ?? 0) + 1;
    }
    return c;
  };

  const headerId = `card-header-${props.result.category}`;
  const cardId = `card-body-${props.result.category}`;

  return (
    <div class="section-card" data-card role="listitem">
      <button
        id={headerId}
        type="button"
        class="section-card__header"
        onClick={props.onToggle}
        aria-expanded={props.open}
        aria-controls={cardId}
      >
        <span class={`section-card__status section-card__status--${props.result.verdict}`} />
        <span class="section-card__title">{CATEGORY_LABELS[props.result.category]}</span>
        <span class="section-card__badges">
          <Show when={(counts().fail ?? 0) > 0}>
            <span class="badge badge--fail">{counts().fail} failed</span>
          </Show>
          <Show when={(counts().warn ?? 0) > 0}>
            <span class="badge badge--warn">{counts().warn} warnings</span>
          </Show>
          <Show when={(counts().info ?? 0) > 0}>
            <span class="badge badge--info">{counts().info} info</span>
          </Show>
          <Show when={(counts().pass ?? 0) > 0}>
            <span class="badge badge--pass">{counts().pass} passed</span>
          </Show>
        </span>
        <Show when={!props.open && props.result.detail}>
          <span class="section-card__summary">{props.result.detail}</span>
        </Show>
        <span class="section-card__spacer" />
        <Show when={headerLink()}>
          {(l) => (
            <a
              class="section-card__link"
              href={l().href}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => e.stopPropagation()}
            >{l().label}</a>
          )}
        </Show>
        <span class={`section-card__chevron${props.open ? ' section-card__chevron--open' : ''}`}>
          &#9660;
        </span>
      </button>

      <Show when={props.open}>
        <div
          id={cardId}
          class="section-card__body"
          role="region"
          aria-labelledby={headerId}
        >
          <Show when={props.showExplanations && explanation()}>
            {(e) => (
              <div class="explain-card">
                {e().summary}
                <Show when={e().guideUrl}>
                  {' '}<a href={e().guideUrl} target="_blank" rel="noopener noreferrer" class="explain-card__guide-link">Learn more ↗</a>
                </Show>
              </div>
            )}
          </Show>
          <Show when={props.result.detail}>
            <div class="section-card__detail">{props.result.detail}</div>
          </Show>
          <Show
            when={props.result.sub_checks.length > 0}
            fallback={<p class="section-empty">No sub-checks</p>}
          >
            <ul class="check-list">
              <For each={props.result.sub_checks}>
                {(sc) => (
                  <li
                    class={`check-list__item${
                      sc.verdict === 'fail' ? ' check-row--fail' :
                      sc.verdict === 'warn' ? ' check-row--warn' : ''
                    }${props.showExplanations ? ' check-list__item--explainable' : ''}`}
                  >
                    <span class={`badge badge--${sc.verdict}`}>{sc.verdict}</span>
                    <span class="check-list__name">{subCheckLabel(sc.name)}</span>
                    <span class="check-list__message">{sc.detail}</span>
                    <Show when={props.showExplanations && subCheckExplanation(sc.name)}>
                      <span class="check-explain">{subCheckExplanation(sc.name)}</span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
          </Show>

          <Show when={props.result.enrichment && props.result.enrichment.length > 0}>
            <div class="enrichment-list">
              <For each={props.result.enrichment!}>
                {(e: IpEnrichment) => (
                  <span class="enrichment-item">
                    <span class="enrichment-item__ip">{e.ip}</span>
                    <Show when={e.asn}><span class="badge badge--skip">AS{e.asn}</span></Show>
                    <Show when={e.org}><span class="badge badge--skip">{e.org}</span></Show>
                    <Show when={e.ip_type}><span class="badge badge--skip">{e.ip_type}</span></Show>
                  </span>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}
