import { Show, For } from 'solid-js';
import CategoryCard from './CategoryCard';
import type { Ecosystem } from './helpers';
import type { Category, CheckResult, Verdict } from '../lib/types';
import {
  CATEGORY_LABELS, VERDICT_ORDER,
  GROUP_ORDER, GROUP_LABELS, GROUP_CATEGORIES,
} from '../lib/types';

function NoMailNotice(props: { mxResult?: CheckResult }) {
  const reason = (): string | null => {
    if (!props.mxResult) return null;
    const names = props.mxResult.sub_checks.map((sc) => sc.name);
    if (names.includes('null_mx')) return 'This domain explicitly declares it does not accept email (null MX).';
    if (names.includes('no_mx')) return 'This domain does not appear to receive email — no MX records found.';
    return null;
  };

  return (
    <Show when={reason()}>
      {(msg) => <div class="notice-banner" role="status">{msg()}</div>}
    </Show>
  );
}

function GroupHeading(props: {
  group: import('../lib/types').Group;
  categories: Map<Category, CheckResult>;
}) {
  const groupCats = () => GROUP_CATEGORIES[props.group];
  const completedResults = () =>
    groupCats().map((c) => props.categories.get(c)).filter(Boolean) as CheckResult[];
  const worstVerdict = (): Verdict | undefined => {
    const results = completedResults();
    if (results.length === 0) return undefined;
    return results.reduce<Verdict>(
      (worst, r) => (VERDICT_ORDER[r.verdict] > VERDICT_ORDER[worst] ? r.verdict : worst),
      'skip',
    );
  };

  return (
    <div class="group-heading">
      <span class="group-heading__title">{GROUP_LABELS[props.group]}</span>
      <Show when={worstVerdict()}>
        {(v) => <span class={`badge badge--${v()} badge--small`}>{v()}</span>}
      </Show>
      <span class="group-heading__divider" />
    </div>
  );
}

interface ResultsGridProps {
  categories: Map<Category, CheckResult>;
  loading: boolean;
  openSections: Set<Category>;
  onToggleSection: (cat: Category) => void;
  showExplanations: boolean;
  domain: string;
  ecosystem?: Ecosystem;
  mxResult?: CheckResult;
  completedCount: number;
  expandAll: boolean;
  onSetExpandAll: (v: boolean) => void;
  onSetShowExplanations: (fn: (v: boolean) => boolean) => void;
}

export default function ResultsGrid(props: ResultsGridProps) {
  return (
    <>
      <Show when={props.categories.size > 0}>
        <NoMailNotice mxResult={props.mxResult} />

        <div class="section-controls">
          <div class="section-controls__left">
            <Show when={props.loading}>
              <span class="progress-text">{props.completedCount} of 12 checks complete</span>
            </Show>
            <button
              type="button"
              class="filter-toggle"
              classList={{ 'filter-toggle--active': props.showExplanations }}
              onClick={() => props.onSetShowExplanations((v) => !v)}
              aria-pressed={props.showExplanations}
              title="Toggle explanations (e)"
            >explain</button>
          </div>
          <div class="section-controls__right">
            <button
              type="button"
              class="filter-toggle"
              onClick={() => props.onSetExpandAll(!props.expandAll)}
              aria-pressed={props.expandAll}
            >
              {props.expandAll ? 'Collapse all' : 'Expand all'}
            </button>
          </div>
        </div>
      </Show>

      <div class="category-list" role="list" aria-live="polite" aria-busy={props.loading}>
        <For each={GROUP_ORDER}>
          {(group) => (
            <>
              <GroupHeading group={group} categories={props.categories} />
              <For each={GROUP_CATEGORIES[group]}>
                {(cat) => {
                  const result = () => props.categories.get(cat);
                  return (
                    <Show
                      when={result()}
                      fallback={
                        <Show when={props.loading}>
                          <div
                            class="section-card card--pending"
                            aria-busy="true"
                            aria-label={`Loading ${CATEGORY_LABELS[cat]}…`}
                          >
                            <div class="section-card__header">
                              <span class="section-card__status section-card__status--skip" />
                              <span class="section-card__title">{CATEGORY_LABELS[cat]}</span>
                              <span class="section-card__spacer" />
                            </div>
                          </div>
                        </Show>
                      }
                    >
                      {(r) => (
                        <CategoryCard
                          result={r()}
                          open={props.openSections.has(cat)}
                          onToggle={() => props.onToggleSection(cat)}
                          showExplanations={props.showExplanations}
                          domain={props.domain}
                          ecosystem={props.ecosystem}
                        />
                      )}
                    </Show>
                  );
                }}
              </For>
            </>
          )}
        </For>
      </div>
    </>
  );
}
