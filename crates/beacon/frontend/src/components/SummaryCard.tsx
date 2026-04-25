import { createSignal, Show } from 'solid-js';
import { copyToClipboard, downloadFile } from '@netray-info/common-frontend/utils';
import GradeDisplay from './GradeDisplay';
import { durationClass } from './helpers';
import type {
  Category,
  CheckResult,
  SummaryEvent,
  Verdict,
} from '../lib/types';
import {
  VERDICT_ORDER,
  GROUP_ORDER, GROUP_LABELS, GROUP_CATEGORIES,
  CATEGORY_LABELS,
  subCheckLabel,
} from '../lib/types';

interface SummaryCardProps {
  summary: SummaryEvent;
  mxResult?: CheckResult;
  ipBaseUrl?: string;
  domain: string;
  categories: Map<Category, CheckResult>;
  clientDurationMs?: number;
}

function ExportButtons(props: {
  domain: string;
  summary: SummaryEvent;
  categories: Map<Category, CheckResult>;
}) {
  const [copyStatus, setCopyStatus] = createSignal<'idle' | 'success' | 'error'>('idle');

  const downloadJson = () => {
    const data = {
      domain: props.domain,
      duration_ms: props.summary.duration_ms,
      verdicts: props.summary.verdicts,
      categories: Object.fromEntries(
        Array.from(props.categories.entries()).map(([k, v]) => [k, v]),
      ),
    };
    downloadFile(JSON.stringify(data, null, 2), `beacon-${props.domain}.json`, 'application/json');
  };

  const copyMarkdown = async () => {
    const lines: string[] = [
      `# Email Security: ${props.domain}`,
      '',
      `**Verdict**: ${Object.values(props.summary.verdicts).reduce((w, v) =>
        VERDICT_ORDER[v as Verdict] > VERDICT_ORDER[w as Verdict] ? v : w,
      )}`,
    ];
    if (props.summary.duration_ms !== undefined) {
      lines.push(`**Duration**: ${props.summary.duration_ms}ms`);
    }
    for (const group of GROUP_ORDER) {
      lines.push(`## ${GROUP_LABELS[group]}`, '');
      for (const cat of GROUP_CATEGORIES[group]) {
        const result = props.categories.get(cat);
        if (!result) continue;
        lines.push(`### ${CATEGORY_LABELS[cat]} — ${result.verdict}`);
        if (result.detail) lines.push(result.detail, '');
        for (const sc of result.sub_checks) {
          lines.push(`- **${sc.verdict.toUpperCase()}** ${subCheckLabel(sc.name)}${sc.detail ? ` — ${sc.detail}` : ''}`);
        }
        lines.push('');
      }
    }
    lines.push(`_Inspected via [beacon](https://email.netray.info)_`);

    const ok = await copyToClipboard(lines.join('\n'));
    setCopyStatus(ok ? 'success' : 'error');
    setTimeout(() => setCopyStatus('idle'), 2000);
  };

  return (
    <div class="export-buttons">
      <button type="button" class="export-buttons__btn" onClick={copyMarkdown} aria-label="Copy as Markdown">
        {copyStatus() === 'success' ? 'copied!' : copyStatus() === 'error' ? 'failed' : 'copy MD'}
      </button>
      <button type="button" class="export-buttons__btn" onClick={downloadJson} aria-label="Download as JSON">
        JSON
      </button>
    </div>
  );
}

export default function SummaryCard(props: SummaryCardProps) {
  const overallVerdict = (): Verdict => {
    const vs = Object.values(props.summary.verdicts) as Verdict[];
    return vs.reduce<Verdict>(
      (worst, v) => (VERDICT_ORDER[v] > VERDICT_ORDER[worst] ? v : worst),
      'skip',
    );
  };

  const counts = () => {
    const c: Partial<Record<Verdict, number>> = {};
    for (const v of Object.values(props.summary.verdicts) as Verdict[]) {
      c[v] = (c[v] ?? 0) + 1;
    }
    return c;
  };

  const firstEnrichment = () => props.mxResult?.enrichment?.[0];

  return (
    <div class="overview">
      <div class="overview__row overview__row--grade">
        <GradeDisplay summary={props.summary} />
        <div class="overview__item overview__item--verdict">
          <span class="overview__label">Verdict</span>
          <span class={`badge badge--${overallVerdict()}`}>{overallVerdict()}</span>
        </div>
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
        {(() => {
          const ms = props.summary.duration_ms ?? props.clientDurationMs;
          return ms !== undefined ? (
            <div class="overview__item">
              <span class="overview__label">Duration</span>
              <span class={durationClass(ms)}>{ms}ms</span>
            </div>
          ) : null;
        })()}
        <ExportButtons domain={props.domain} summary={props.summary} categories={props.categories} />
      </div>

      <Show when={firstEnrichment()}>
        {(e) => (
          <div class="overview__row overview__row--enrichment">
            <div class="overview__item">
              <span class="overview__label">MX</span>
              <span class="overview__value">{e().ip}</span>
            </div>
            <Show when={e().org}>
              <div class="overview__item">
                <span class="overview__label">Hosted by</span>
                <span class="overview__value">
                  {e().org}
                  <Show when={e().ip_type}>
                    {' '}
                    <span class="overview__value--qualifier">({e().ip_type})</span>
                  </Show>
                </span>
              </div>
            </Show>
            <a
              href={`${props.ipBaseUrl ?? 'https://ip.netray.info'}/${encodeURIComponent(e().ip)}`}
              class="overview__ip-link"
              target="_blank"
              rel="noopener noreferrer"
            >IP ↗</a>
          </div>
        )}
      </Show>
    </div>
  );
}
